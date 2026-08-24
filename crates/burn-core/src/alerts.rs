//! Reglas de alerta.
//!
//! Puro calculo sobre datos ya leidos: no toca disco ni notifica. El shell
//! decide como mostrarlas, y esta separacion es lo que las hace testeables.

use crate::pricing;
use crate::profiles::{Billing, LiveSession, PlanUsage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Budget,
    Context,
    PlanLimit,
    ExpensiveModel,
}

impl AlertKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertKind::Budget => "budget",
            AlertKind::Context => "context",
            AlertKind::PlanLimit => "plan_limit",
            AlertKind::ExpensiveModel => "expensive_model",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub kind: AlertKind,
    /// Identidad estable de *esta* alerta, para el cooldown. Incluye el escalon
    /// alcanzado: pasar de 75% a 90% es una alerta nueva, no la misma repetida.
    pub key: String,
    pub title: String,
    pub body: String,
    pub severity: Severity,
    /// De donde vino. Sin esto una alerta de contexto solo dice un nombre de
    /// sesion, que no alcanza para saber en que cuenta ni en que proyecto
    /// mirar — ni para volver a encontrarla despues.
    pub account: Option<String>,
    pub project: Option<String>,
    pub session_id: Option<String>,
}

fn yes() -> bool {
    true
}

/// Solo el diario por defecto. El mensual es un techo que ya puede estar
/// pasado cuando lo configuras, y arrancar bloqueando todo no ayuda a nadie.
fn default_guard_periods() -> Vec<String> {
    vec!["daily".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub budget_daily_usd: Option<f64>,
    pub budget_weekly_usd: Option<f64>,
    pub budget_monthly_usd: Option<f64>,
    /// Escalones de presupuesto y de limite de plan, en porcentaje.
    pub budget_steps: Vec<u32>,
    /// Si el hook `UserPromptSubmit` corta el turno al pasarse del techo.
    ///
    /// Es lo unico que *frena* en vez de avisar, asi que tiene que poder
    /// apagarse sin editar shell: un dia que de verdad haya que seguir, el
    /// bloqueo tambien se come el mensaje que pediria desbloquearlo.
    #[serde(default = "yes")]
    pub guard_enabled: bool,
    /// Que techos hace cumplir el bloqueo: `daily`, `weekly`, `monthly`.
    #[serde(default = "default_guard_periods")]
    pub guard_periods: Vec<String>,
    pub limit_steps: Vec<u32>,
    pub context_warn_tokens: i64,
    pub context_critical_tokens: i64,
    /// Fraccion del gasto del dia en modelos premium que dispara el aviso.
    pub expensive_share: f64,
    /// Piso en dolares para no avisar por centavos al empezar el dia.
    pub expensive_min_usd: f64,
    pub cooldown_minutes: i64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            budget_daily_usd: None,
            budget_weekly_usd: None,
            budget_monthly_usd: None,
            budget_steps: vec![50, 75, 90, 100],
            guard_enabled: true,
            guard_periods: default_guard_periods(),
            limit_steps: vec![75, 90],
            // 250K es donde el cache_read empieza a dominar la factura;
            // 500K es donde cada turno cuesta mas que el trabajo que hace.
            context_warn_tokens: 250_000,
            context_critical_tokens: 500_000,
            expensive_share: 0.5,
            expensive_min_usd: 25.0,
            cooldown_minutes: 45,
        }
    }
}

/// Todo lo que las reglas necesitan saber, ya reunido por el caller.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub today_usd: f64,
    pub week_usd: f64,
    pub month_usd: f64,
    /// (sesion viva, contexto actual en tokens)
    pub live: Vec<(LiveSession, i64)>,
    /// (nombre de cuenta, facturacion, uso del plan)
    pub plans: Vec<(String, Billing, PlanUsage)>,
    /// Gasto de hoy por modelo canonico.
    pub today_by_model: Vec<(String, f64)>,
}

/// El escalon mas alto alcanzado, o `None` si no llego al primero.
fn step_reached(pct: f64, steps: &[u32]) -> Option<u32> {
    steps.iter().copied().filter(|s| pct >= f64::from(*s)).max()
}

fn severity_for(pct: f64) -> Severity {
    if pct >= 90.0 {
        Severity::Critical
    } else if pct >= 75.0 {
        Severity::Warn
    } else {
        Severity::Info
    }
}

fn fmt_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else {
        format!("{}k", n / 1000)
    }
}

pub fn evaluate(snap: &Snapshot, cfg: &AlertConfig) -> Vec<Alert> {
    let mut out = Vec::new();
    budget_alerts(snap, cfg, &mut out);
    context_alerts(snap, cfg, &mut out);
    plan_limit_alerts(snap, cfg, &mut out);
    expensive_model_alerts(snap, cfg, &mut out);
    out
}

fn budget_alerts(snap: &Snapshot, cfg: &AlertConfig, out: &mut Vec<Alert>) {
    let periods: [(&str, &str, Option<f64>, f64); 3] = [
        ("dia", "daily", cfg.budget_daily_usd, snap.today_usd),
        ("semana", "weekly", cfg.budget_weekly_usd, snap.week_usd),
        ("mes", "monthly", cfg.budget_monthly_usd, snap.month_usd),
    ];
    for (label, id, limit, spent) in periods {
        let Some(limit) = limit.filter(|l| *l > 0.0) else {
            continue;
        };
        let pct = spent / limit * 100.0;
        let Some(step) = step_reached(pct, &cfg.budget_steps) else {
            continue;
        };
        out.push(Alert {
            account: None,
            project: None,
            session_id: None,
            kind: AlertKind::Budget,
            key: format!("{id}:{step}"),
            title: if step >= 100 {
                format!("Presupuesto del {label} agotado")
            } else {
                format!("{step}% del presupuesto del {label}")
            },
            body: format!("${spent:.2} de ${limit:.2}"),
            severity: severity_for(pct),
        });
    }
}

fn context_alerts(snap: &Snapshot, cfg: &AlertConfig, out: &mut Vec<Alert>) {
    for (session, ctx) in &snap.live {
        let (severity, threshold) = if *ctx >= cfg.context_critical_tokens {
            (Severity::Critical, cfg.context_critical_tokens)
        } else if *ctx >= cfg.context_warn_tokens {
            (Severity::Warn, cfg.context_warn_tokens)
        } else {
            continue;
        };
        let name = session.name.clone().unwrap_or_else(|| {
            session
                .cwd
                .rsplit('/')
                .next()
                .unwrap_or(&session.cwd)
                .to_string()
        });
        out.push(Alert {
            account: Some(session.account.clone()),
            project: Some(session.cwd.clone()),
            session_id: Some(session.session_id.clone()),
            kind: AlertKind::Context,
            key: format!("{}:{}", session.session_id, threshold),
            title: format!("Contexto inflado en {name}"),
            body: format!(
                "{} · {} de contexto. A este tamano casi todo el costo del turno es releer lo mismo: /compact o sesion nueva.",
                session.account,
                fmt_tokens(*ctx)
            ),
            severity,
        });
    }
}

fn plan_limit_alerts(snap: &Snapshot, cfg: &AlertConfig, out: &mut Vec<Alert>) {
    for (account, _billing, usage) in &snap.plans {
        for l in &usage.limits {
            // Un limite inactivo ya se reinicio: su porcentaje esta congelado
            // y avisar por el seria ruido.
            if !l.is_active {
                continue;
            }
            let Some(step) = step_reached(l.percent, &cfg.limit_steps) else {
                continue;
            };
            let window = match l.kind.as_str() {
                "session" => "de 5 horas",
                "weekly_all" => "semanal",
                "weekly_scoped" => "semanal del modelo",
                other => other,
            };
            out.push(Alert {
                account: Some(account.clone()),
                project: None,
                session_id: None,
                kind: AlertKind::PlanLimit,
                key: format!("{account}:{}:{step}", l.kind),
                title: format!("{}% del limite {window} en {account}", l.percent.round()),
                body: match &l.resets_at {
                    Some(r) => format!("reinicia {r}"),
                    None => String::new(),
                },
                severity: severity_for(l.percent),
            });
        }
    }
}

/// Avisa cuando el dia se esta yendo en modelos de tarifa premium.
///
/// "Caro" no es una lista fija: es cualquier modelo cuyo precio de salida
/// supere al de la familia Opus, que se lee de la misma tabla de precios.
fn expensive_model_alerts(snap: &Snapshot, cfg: &AlertConfig, out: &mut Vec<Alert>) {
    let baseline = pricing::table()
        .models
        .iter()
        .find(|m| m.id == "claude-opus-5")
        .map(|m| m.output)
        .unwrap_or(25.0);

    let total: f64 = snap.today_by_model.iter().map(|(_, c)| c).sum();
    if total < cfg.expensive_min_usd {
        return;
    }

    for (model, cost) in &snap.today_by_model {
        let Some(rate) = pricing::table().models.iter().find(|m| &m.id == model) else {
            continue;
        };
        if rate.output <= baseline {
            continue;
        }
        let share = cost / total;
        if share < cfg.expensive_share {
            continue;
        }
        let factor = rate.output / baseline;
        out.push(Alert {
            account: None,
            project: None,
            session_id: None,
            kind: AlertKind::ExpensiveModel,
            key: format!("{model}:{}", (share * 10.0) as u32),
            title: format!("{} se lleva el {:.0}% de hoy", rate.label, share * 100.0),
            body: format!(
                "${cost:.0} de ${total:.0}. Cuesta {factor:.0}x lo que Opus 5 por token: en tareas que no lo necesiten, cambiar de modelo corta esa linea."
            ),
            severity: Severity::Warn,
        });
    }
}

#[cfg(test)]
mod tests {
    /// Una config guardada antes de que existiera el bloqueo no debe romper
    /// la deserializacion ni quedar con el guard apagado por accidente.
    #[test]
    fn config_vieja_estrena_el_guard_prendido() {
        let raw = r#"{"budget_daily_usd":33.0,"budget_weekly_usd":null,
            "budget_monthly_usd":1000.0,"budget_steps":[50,100],"limit_steps":[75],
            "context_warn_tokens":250000,"context_critical_tokens":500000,
            "expensive_share":0.5,"expensive_min_usd":25.0,"cooldown_minutes":45}"#;
        let cfg: super::AlertConfig = serde_json::from_str(raw).unwrap();
        assert!(cfg.guard_enabled);
        assert_eq!(cfg.guard_periods, vec!["daily".to_string()]);
        assert_eq!(cfg.budget_monthly_usd, Some(1000.0));
    }

    use super::*;
    use crate::profiles::PlanLimit;

    fn snap() -> Snapshot {
        Snapshot {
            today_usd: 0.0,
            week_usd: 0.0,
            month_usd: 0.0,
            live: vec![],
            plans: vec![],
            today_by_model: vec![],
        }
    }

    fn live(id: &str) -> LiveSession {
        LiveSession {
            pid: 1,
            session_id: id.into(),
            cwd: "/Users/x/proyecto".into(),
            name: Some("proyecto-a1".into()),
            status: Some("busy".into()),
            started_at_ms: None,
            version: None,
            account: "cruisebound".into(),
        }
    }

    #[test]
    fn presupuesto_avisa_en_el_escalon_mas_alto() {
        let cfg = AlertConfig {
            budget_daily_usd: Some(100.0),
            ..Default::default()
        };
        let s = Snapshot {
            today_usd: 78.0,
            ..snap()
        };
        let alerts = evaluate(&s, &cfg);
        assert_eq!(alerts.len(), 1);
        // 78% pasa 50 y 75, pero solo avisa una vez, por el escalon mas alto
        assert_eq!(alerts[0].key, "daily:75");
        assert_eq!(alerts[0].severity, Severity::Warn);
    }

    #[test]
    fn sin_presupuesto_no_hay_alerta_de_presupuesto() {
        let s = Snapshot {
            today_usd: 9_999.0,
            ..snap()
        };
        assert!(evaluate(&s, &AlertConfig::default()).is_empty());
    }

    #[test]
    fn contexto_escala_de_aviso_a_critico() {
        let cfg = AlertConfig::default();
        let warn = Snapshot {
            live: vec![(live("s1"), 300_000)],
            ..snap()
        };
        let a = &evaluate(&warn, &cfg)[0];
        assert_eq!(a.severity, Severity::Warn);
        assert_eq!(a.key, "s1:250000");

        let crit = Snapshot {
            live: vec![(live("s1"), 700_000)],
            ..snap()
        };
        let b = &evaluate(&crit, &cfg)[0];
        assert_eq!(b.severity, Severity::Critical);
        // clave distinta: cruzar el umbral critico es una alerta nueva
        assert_eq!(b.key, "s1:500000");
        assert!(b.body.contains("compact"));
    }

    #[test]
    fn la_alerta_de_contexto_dice_de_donde_vino() {
        let s = Snapshot {
            live: vec![(live("s1"), 700_000)],
            ..snap()
        };
        let a = &evaluate(&s, &AlertConfig::default())[0];
        assert_eq!(a.account.as_deref(), Some("cruisebound"));
        assert_eq!(a.session_id.as_deref(), Some("s1"));
        assert_eq!(a.project.as_deref(), Some("/Users/x/proyecto"));
        assert!(a.body.contains("cruisebound"));
    }

    #[test]
    fn contexto_chico_no_molesta() {
        let s = Snapshot {
            live: vec![(live("s1"), 80_000)],
            ..snap()
        };
        assert!(evaluate(&s, &AlertConfig::default()).is_empty());
    }

    #[test]
    fn limite_inactivo_se_ignora() {
        let limit = |is_active: bool, percent: f64| PlanLimit {
            kind: "weekly_all".into(),
            group: "weekly".into(),
            percent,
            severity: "normal".into(),
            resets_at: None,
            scope: None,
            is_active,
        };
        let usage = |l: PlanLimit| PlanUsage {
            fetched_at_ms: None,
            limits: vec![l],
            extra_usage_spent: None,
            extra_usage_limit: None,
            extra_usage_enabled: true,
        };
        let inactive = Snapshot {
            plans: vec![("cb".into(), Billing::Overage, usage(limit(false, 95.0)))],
            ..snap()
        };
        assert!(evaluate(&inactive, &AlertConfig::default()).is_empty());

        let active = Snapshot {
            plans: vec![("cb".into(), Billing::Overage, usage(limit(true, 95.0)))],
            ..snap()
        };
        let a = &evaluate(&active, &AlertConfig::default())[0];
        assert_eq!(a.severity, Severity::Critical);
        assert_eq!(a.key, "cb:weekly_all:90");
    }

    #[test]
    fn modelo_caro_se_deriva_del_precio_no_de_una_lista() {
        let cfg = AlertConfig::default();
        // Fable cuesta el doble que Opus 5 y se lleva el 80% del dia
        let s = Snapshot {
            today_by_model: vec![
                ("claude-fable-5".into(), 80.0),
                ("claude-opus-5".into(), 20.0),
            ],
            ..snap()
        };
        let alerts = evaluate(&s, &cfg);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].title.contains("Fable"));
        assert!(alerts[0].body.contains("2x"));
    }

    #[test]
    fn opus_dominando_el_dia_no_es_alerta() {
        let s = Snapshot {
            today_by_model: vec![("claude-opus-5".into(), 100.0)],
            ..snap()
        };
        assert!(evaluate(&s, &AlertConfig::default()).is_empty());
    }

    #[test]
    fn dia_barato_no_dispara_modelo_caro() {
        let s = Snapshot {
            today_by_model: vec![("claude-fable-5".into(), 3.0)],
            ..snap()
        };
        assert!(evaluate(&s, &AlertConfig::default()).is_empty());
    }
}
