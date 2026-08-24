//! Comandos que expone el backend al frontend.
//!
//! Cada uno devuelve datos ya agregados: el frontend no calcula costos, solo
//! los dibuja. Asi la unica implementacion del precio vive en `burn-core`.

use crate::state::{AppState, TraySummary};
use burn_core::profiles::{self, Billing, LiveSession, PlanUsage};
use burn_core::store::{
    Composition, DayRow, Filter, ModelRow, MonthRow, SessionRow, SubagentSplit, TurnPoint,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct AccountInfo {
    pub name: String,
    pub email: Option<String>,
    pub org: Option<String>,
    pub plan: Option<String>,
    pub billing: Billing,
    /// `false` cuando el gasto es tarifa plana: la UI lo etiqueta como valor
    /// teorico para no reportar plata que nadie factura.
    pub is_billable: bool,
    pub plan_usage: Option<PlanUsage>,
    pub live_sessions: Vec<LiveSession>,
}

#[derive(Serialize)]
pub struct Overview {
    pub accounts: Vec<AccountInfo>,
    pub today_usd: f64,
    pub today_billable_usd: f64,
    pub week_usd: f64,
    /// La mitad de `week_usd` que de verdad se factura. Sin esto el titular
    /// mezcla la tarifa plana con el overage y deja de significar nada.
    pub week_billable_usd: f64,
    pub month_usd: f64,
    pub month_billable_usd: f64,
    /// Cuanto del gasto se lo llevaron los subagentes.
    pub subagents: SubagentSplit,
    pub by_day: Vec<DayRow>,
    pub by_month: Vec<MonthRow>,
    pub composition: Composition,
    pub tray: TraySummary,
    /// Cuentas conocidas, para poblar el filtro sin una consulta aparte.
    pub known_accounts: Vec<String>,
    /// Primer y ultimo dia con datos. Claude Code poda transcripts viejos, asi
    /// que el historico no arranca donde el usuario cree.
    pub data_from: Option<String>,
    pub data_to: Option<String>,
}

/// Los totales se acumulan por cuenta y no de una sola consulta, porque cada
/// uno se parte en dos: lo que se factura y lo que es tarifa plana.
#[derive(Default)]
struct Totals {
    today: f64,
    today_billable: f64,
    week: f64,
    week_billable: f64,
    month: f64,
    month_billable: f64,
}

impl Totals {
    fn add(&mut self, billable: bool, today: f64, week: f64, month: f64) {
        self.today += today;
        self.week += week;
        self.month += month;
        if billable {
            self.today_billable += today;
            self.week_billable += week;
            self.month_billable += month;
        }
    }
}

fn day_string(offset_days: i64) -> String {
    let now = chrono::Local::now() - chrono::Duration::days(offset_days);
    now.format("%Y-%m-%d").to_string()
}

/// Instante UTC desde el que contar, N dias atras. Los transcripts guardan la
/// hora en UTC ISO-8601, asi que la comparacion es lexicografica.
fn utc_since(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

pub fn build_overview(state: &AppState, filter: &Filter) -> anyhow::Result<Overview> {
    let profs = state.profiles.lock().unwrap().clone();
    let db = state.db.lock().unwrap();

    let today = day_string(0);
    let (week_from, month_from) = (utc_since(7), utc_since(30));
    let mut totals = Totals::default();

    let mut accounts = Vec::new();
    let mut live_total = 0usize;
    let mut worst: Option<(f64, String)> = None;

    for p in &profs {
        let is_billable = p.billing == Billing::Overage;
        totals.add(
            is_billable,
            db.cost_on_day(&today, Some(&p.name))?,
            db.cost_since(&week_from, Some(&p.name))?,
            db.cost_since(&month_from, Some(&p.name))?,
        );

        let plan_usage = profiles::read_plan_usage(p);
        if let Some(u) = &plan_usage {
            for l in &u.limits {
                // Solo los limites activos dicen algo util; los inactivos ya se
                // reiniciaron y su porcentaje esta congelado.
                if l.is_active && worst.as_ref().is_none_or(|(pct, _)| l.percent > *pct) {
                    worst = Some((l.percent, l.kind.clone()));
                }
            }
        }

        let live = profiles::read_live_sessions(p);
        live_total += live.len();

        accounts.push(AccountInfo {
            name: p.name.clone(),
            email: p.email.clone(),
            org: p.org.clone(),
            plan: p.plan.clone(),
            billing: p.billing,
            is_billable,
            plan_usage,
            live_sessions: live,
        });
    }

    let (worst_limit_pct, worst_limit_kind) = match worst {
        Some((p, k)) => (Some(p), Some(k)),
        None => (None, None),
    };

    let (data_from, data_to) = db.data_range()?;

    // Contexto de la sesion viva mas cargada: el numero que dispara la alerta
    // de contexto inflado.
    let max_live_ctx = accounts
        .iter()
        .flat_map(|a| a.live_sessions.iter())
        .filter_map(|s| db.session_timeline(&s.session_id).ok())
        .filter_map(|pts| pts.last().map(|p| p.ctx_tok))
        .max();

    let tray = TraySummary {
        today_usd: totals.today,
        today_billable_usd: totals.today_billable,
        worst_limit_pct,
        worst_limit_kind,
        live_sessions: live_total,
        max_live_ctx,
    };

    Ok(Overview {
        accounts,
        today_usd: totals.today,
        today_billable_usd: totals.today_billable,
        week_usd: totals.week,
        week_billable_usd: totals.week_billable,
        month_usd: totals.month,
        month_billable_usd: totals.month_billable,
        subagents: db.subagent_split(filter)?,
        by_day: db.by_day(filter, 120)?,
        by_month: db.by_month()?,
        composition: db.composition(filter)?,
        tray,
        known_accounts: profs.iter().map(|p| p.name.clone()).collect(),
        data_from,
        data_to,
    })
}

#[tauri::command]
pub fn overview(state: State<'_, AppState>, filter: Option<Filter>) -> Res<Overview> {
    build_overview(&state, &filter.unwrap_or_default()).map_err(err)
}

#[tauri::command]
pub fn sync_now(state: State<'_, AppState>) -> Res<usize> {
    state.sync().map_err(err)
}

/// Una sesion mas el dato de si su cuenta factura de verdad.
///
/// Sin esto la tabla muestra $1.317 para una sesion de una cuenta de tarifa
/// plana como si fuera una factura, cuando el techo de esa cuenta son $100.
#[derive(Serialize)]
pub struct SessionWithBilling {
    #[serde(flatten)]
    pub row: SessionRow,
    pub is_billable: bool,
}

#[tauri::command]
pub fn sessions(
    state: State<'_, AppState>,
    filter: Option<Filter>,
    limit: Option<i64>,
) -> Res<Vec<SessionWithBilling>> {
    let filter = filter.unwrap_or_default();
    let billable: Vec<String> = state
        .profiles
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.billing == Billing::Overage)
        .map(|p| p.name.clone())
        .collect();
    let rows = state
        .db
        .lock()
        .unwrap()
        .top_sessions(&filter, limit.unwrap_or(300))
        .map_err(err)?;
    Ok(rows
        .into_iter()
        .map(|row| SessionWithBilling {
            is_billable: billable.contains(&row.account),
            row,
        })
        .collect())
}

#[tauri::command]
pub fn session_timeline(state: State<'_, AppState>, session_id: String) -> Res<Vec<TurnPoint>> {
    state
        .db
        .lock()
        .unwrap()
        .session_timeline(&session_id)
        .map_err(err)
}

#[tauri::command]
pub fn models(state: State<'_, AppState>, filter: Option<Filter>) -> Res<Vec<ModelRow>> {
    state
        .db
        .lock()
        .unwrap()
        .by_model(&filter.unwrap_or_default())
        .map_err(err)
}

#[tauri::command]
pub fn context_histogram(
    state: State<'_, AppState>,
    filter: Option<Filter>,
) -> Res<Vec<(i64, i64)>> {
    state
        .db
        .lock()
        .unwrap()
        .context_histogram(&filter.unwrap_or_default())
        .map_err(err)
}

#[tauri::command]
pub fn budgets(state: State<'_, AppState>) -> Res<Vec<(String, String, f64)>> {
    state.db.lock().unwrap().budgets().map_err(err)
}

#[tauri::command]
pub fn set_budget(
    state: State<'_, AppState>,
    scope: String,
    period: String,
    limit_usd: f64,
) -> Res<()> {
    state
        .db
        .lock()
        .unwrap()
        .set_budget(&scope, &period, limit_usd)
        .map_err(err)
}

/// Abre la ventana principal y la trae al frente.
///
/// No es un comando: lo llaman tanto el frontend (via `show_main_window`)
/// como el arranque y el guard de instancia unica, que no tienen `Res<()>`
/// donde poner un error.
pub fn reveal_main_window(app: &AppHandle) {
    if let Some(tray) = app.get_webview_window("tray") {
        let _ = tray.hide();
    }
    // Con ActivationPolicy::Accessory la app no roba el foco sola; hay que
    // pedirle a macOS que la ponga adelante explicitamente, y *antes* de
    // mostrar la ventana para que aparezca ya al frente.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
}

/// Trae la ventana principal al frente y esconde el popover.
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Res<()> {
    if app.get_webview_window("main").is_none() {
        return Err("no existe la ventana principal".to_string());
    }
    reveal_main_window(&app);
    Ok(())
}

/// Vuelve a esconder la app del Dock cuando se cierra la ventana principal.
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Res<()> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    Ok(())
}

#[tauri::command]
pub fn alert_config(state: State<'_, AppState>) -> Res<burn_core::alerts::AlertConfig> {
    Ok(crate::alerts::load_config(&state))
}

#[tauri::command]
pub fn set_alert_config(
    state: State<'_, AppState>,
    config: burn_core::alerts::AlertConfig,
) -> Res<()> {
    crate::alerts::save_config(&state, &config).map_err(err)
}

/// Las alertas que ya se dispararon, mas recientes primero.
#[tauri::command]
pub fn recent_alerts(state: State<'_, AppState>, limit: Option<i64>) -> Res<Vec<FiredAlert>> {
    state
        .db
        .lock()
        .unwrap()
        .recent_alerts(limit.unwrap_or(50))
        .map(|rows| {
            rows.into_iter()
                .filter_map(|(kind, fired_at_ms, payload)| {
                    serde_json::from_str::<serde_json::Value>(&payload)
                        .ok()
                        .map(|alert| FiredAlert {
                            kind,
                            fired_at_ms,
                            alert,
                        })
                })
                .collect()
        })
        .map_err(err)
}

#[derive(Serialize)]
pub struct FiredAlert {
    pub kind: String,
    pub fired_at_ms: i64,
    pub alert: serde_json::Value,
}
