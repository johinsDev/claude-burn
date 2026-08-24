//! Puente entre las reglas de `burn-core` y las notificaciones del sistema.

use crate::state::AppState;
use burn_core::alerts::{evaluate, Alert, AlertConfig, Severity, Snapshot};
use burn_core::profiles::{self, Billing};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

const CONFIG_KEY: &str = "alert_config";

pub fn load_config(state: &AppState) -> AlertConfig {
    let db = state.db.lock().unwrap();
    db.get_setting(CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_config(state: &AppState, cfg: &AlertConfig) -> anyhow::Result<()> {
    let db = state.db.lock().unwrap();
    db.set_setting(CONFIG_KEY, &serde_json::to_string(cfg)?)?;
    Ok(())
}

/// Reune el estado actual en la forma que consumen las reglas.
fn snapshot(state: &AppState) -> anyhow::Result<Snapshot> {
    let profs = state.profiles.lock().unwrap().clone();
    let db = state.db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut live = Vec::new();
    let mut plans = Vec::new();
    for p in &profs {
        if let Some(usage) = profiles::read_plan_usage(p) {
            plans.push((p.name.clone(), p.billing, usage));
        }
        for s in profiles::read_live_sessions(p) {
            let ctx = db.latest_context(&s.session_id)?.unwrap_or(0);
            live.push((s, ctx));
        }
    }

    // Solo cuenta el gasto que alguien factura: avisar de un presupuesto por
    // consumo de tarifa plana seria una alarma falsa.
    let billable: Vec<&str> = profs
        .iter()
        .filter(|p| p.billing == Billing::Overage)
        .map(|p| p.name.as_str())
        .collect();
    let sum_over = |f: &dyn Fn(&str) -> anyhow::Result<f64>| -> anyhow::Result<f64> {
        billable.iter().try_fold(0.0, |acc, a| Ok(acc + f(a)?))
    };

    Ok(Snapshot {
        today_usd: sum_over(&|a| db.cost_on_day(&today, Some(a)))?,
        week_usd: sum_over(&|a| db.cost_since(&since(7), Some(a)))?,
        month_usd: sum_over(&|a| db.cost_since(&since(30), Some(a)))?,
        live,
        plans,
        today_by_model: db.cost_by_model_on_day(&today)?,
    })
}

fn since(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Evalua las reglas y notifica lo que no este en cooldown.
pub fn evaluate_and_notify(app: &AppHandle, state: &AppState) {
    let cfg = load_config(state);
    let Ok(snap) = snapshot(state) else { return };
    let alerts = evaluate(&snap, &cfg);
    if alerts.is_empty() {
        return;
    }

    let now = burn_core::now_ms();
    let cooldown_ms = cfg.cooldown_minutes * 60_000;
    let mut fired = Vec::new();

    {
        let db = state.db.lock().unwrap();
        for a in alerts {
            let kind = a.kind.as_str();
            // El cooldown va por (tipo, clave), y la clave incluye el escalon:
            // subir de 75% a 90% notifica aunque el 75% sea reciente.
            let recent = db
                .last_alert_ms(kind, &a.key)
                .ok()
                .flatten()
                .is_some_and(|last| now - last < cooldown_ms);
            if recent {
                continue;
            }
            let payload = serde_json::to_string(&a).unwrap_or_default();
            let _ = db.record_alert(kind, &a.key, now, &payload);
            fired.push(a);
        }
    }

    for a in &fired {
        notify(app, a);
    }
    if !fired.is_empty() {
        let _ = app.emit("burn://alerts", &fired);
    }
}

fn notify(app: &AppHandle, alert: &Alert) {
    let prefix = match alert.severity {
        Severity::Critical => "🔴 ",
        Severity::Warn => "🟠 ",
        Severity::Info => "",
    };
    let _ = app
        .notification()
        .builder()
        .title(format!("{prefix}{}", alert.title))
        .body(&alert.body)
        .show();
}
