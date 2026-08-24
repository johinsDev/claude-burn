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
use tauri::{AppHandle, Emitter, Manager, State};

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
    /// El mes calendario contra el techo: gastado, proyeccion y cuanto queda
    /// por dia. Es la respuesta a "no quiero pasarme de X al mes".
    pub month: MonthPace,
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

/// El mes calendario medido contra el techo mensual.
///
/// Anthropic factura por mes calendario, asi que la ventana movil de 30 dias
/// no sirve para un techo: nunca vuelve a cero.
#[derive(Serialize)]
pub struct MonthPace {
    pub month: String,
    /// Gasto facturable del mes hasta ahora.
    pub spent_usd: f64,
    pub budget_usd: Option<f64>,
    pub day: u32,
    pub days_in_month: u32,
    /// A donde llega el mes si se sigue al ritmo promedio de lo que va.
    pub projected_usd: f64,
    /// Lo que queda del techo repartido en los dias que faltan. `None` sin
    /// techo, `Some(0.0)` cuando ya se paso.
    pub daily_allowance_usd: Option<f64>,
    /// Como vamos hoy y esta semana, no solo en el mes.
    pub today: PeriodPace,
    pub week: PeriodPace,
    /// `true` cuando el filtro apunta a una cuenta de tarifa plana: el techo
    /// no le aplica y mostrarlo ahi seria mentir.
    pub scoped_flat_account: Option<String>,
}

/// Un periodo corto medido contra su techo.
#[derive(Serialize)]
pub struct PeriodPace {
    pub spent_usd: f64,
    pub budget_usd: Option<f64>,
    /// Etiqueta de cuanto va del periodo, p.ej. "dia 3 de 7".
    pub elapsed_label: String,
}

struct PaceInput {
    month_usd: f64,
    today_usd: f64,
    week_usd: f64,
    monthly_budget: Option<f64>,
    daily_budget: Option<f64>,
    weekly_budget: Option<f64>,
    flat_account: Option<String>,
}

fn month_pace(input: PaceInput) -> MonthPace {
    use chrono::Datelike;
    let PaceInput {
        month_usd: spent_usd,
        today_usd,
        week_usd,
        monthly_budget: budget_usd,
        daily_budget,
        weekly_budget,
        flat_account,
    } = input;
    let now = chrono::Local::now();
    let day = now.day();
    let days_in_month = days_in_month(now.year(), now.month());
    // El promedio se toma sobre los dias transcurridos, contando el de hoy
    // aunque este a medias: subestimar el ritmo seria el error caro.
    let projected_usd = spent_usd / f64::from(day) * f64::from(days_in_month);
    let daily_allowance_usd = budget_usd.map(|b| {
        let left_days = f64::from(days_in_month.saturating_sub(day).max(1));
        ((b - spent_usd) / left_days).max(0.0)
    });
    // La semana arranca el lunes, como la de Anthropic y como la lee
    // cualquiera que mire un calendario.
    let weekday = now.weekday().num_days_from_monday() + 1;
    MonthPace {
        month: now.format("%Y-%m").to_string(),
        spent_usd,
        budget_usd,
        day,
        days_in_month,
        projected_usd,
        daily_allowance_usd,
        today: PeriodPace {
            spent_usd: today_usd,
            budget_usd: daily_budget,
            elapsed_label: now.format("%H:%M").to_string(),
        },
        week: PeriodPace {
            spent_usd: week_usd,
            budget_usd: weekly_budget,
            elapsed_label: format!("dia {weekday} de 7"),
        },
        scoped_flat_account: flat_account,
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (y, m) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    chrono::NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|d| d.pred_opt())
        .map_or(30, |d| chrono::Datelike::day(&d))
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

/// Instante UTC del lunes de esta semana a las 00:00 locales.
///
/// La semana del techo es calendario, igual que el mes: una ventana movil de
/// 7 dias nunca se reinicia y no se puede leer contra un calendario.
fn utc_week_start() -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    let back = i64::from(now.weekday().num_days_from_monday());
    let monday = now.date_naive() - chrono::Duration::days(back);
    monday
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
        .map_or_else(
            || utc_since(7),
            |dt| {
                dt.with_timezone(&chrono::Utc)
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string()
            },
        )
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
    let this_month = crate::alerts::this_month();
    let week_start = utc_week_start();
    let mut totals = Totals::default();
    let mut month_billable = 0.0;
    let mut week_billable = 0.0;

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
        if is_billable {
            month_billable += db.cost_in_month(&this_month, Some(&p.name))?;
            week_billable += db.cost_since(&week_start, Some(&p.name))?;
        }

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

    let cfg = crate::alerts::config_from_db(&db);
    let month_budget = cfg.budget_monthly_usd.filter(|l| *l > 0.0);

    // Si el filtro apunta a una cuenta de tarifa plana, el techo no le aplica.
    // Antes el panel seguia mostrando la factura de la cuenta que si se factura aunque
    // estuvieras mirando personal, que es justo lo que confunde.
    let flat_account = filter.account.as_ref().and_then(|name| {
        profs
            .iter()
            .find(|p| &p.name == name && p.billing != Billing::Overage)
            .map(|p| p.name.clone())
    });
    let pace = match &flat_account {
        // Una cuenta de tarifa plana muestra su consumo, sin techo.
        Some(name) => PaceInput {
            month_usd: db.cost_in_month(&this_month, Some(name))?,
            today_usd: db.cost_on_day(&today, Some(name))?,
            week_usd: db.cost_since(&week_start, Some(name))?,
            monthly_budget: None,
            daily_budget: None,
            weekly_budget: None,
            flat_account: Some(name.clone()),
        },
        None => PaceInput {
            month_usd: month_billable,
            today_usd: totals.today_billable,
            week_usd: week_billable,
            monthly_budget: month_budget,
            daily_budget: cfg.budget_daily_usd.filter(|l| *l > 0.0),
            weekly_budget: cfg.budget_weekly_usd.filter(|l| *l > 0.0),
            flat_account: None,
        },
    };

    let tray = TraySummary {
        today_usd: totals.today,
        today_billable_usd: totals.today_billable,
        month_billable_usd: month_billable,
        month_budget_usd: month_budget,
        day_budget_usd: cfg.budget_daily_usd.filter(|l| *l > 0.0),
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
        month: month_pace(pace),
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

/// Cuanto disco ocupan los transcripts de subagente mas viejos que N dias.
#[derive(Serialize)]
pub struct CleanupPreview {
    pub files: usize,
    pub bytes: u64,
    pub older_than_days: u64,
}

fn cleanup_targets(state: &AppState, older_than_days: u64) -> Vec<(std::path::PathBuf, u64)> {
    let profs = state.profiles.lock().unwrap().clone();
    profs
        .iter()
        .flat_map(|p| profiles::subagent_files(p, older_than_days))
        .collect()
}

/// Que se borraria, sin borrar nada. Siempre se consulta antes de limpiar.
#[tauri::command]
pub fn cleanup_preview(state: State<'_, AppState>, older_than_days: u64) -> Res<CleanupPreview> {
    let targets = cleanup_targets(&state, older_than_days);
    Ok(CleanupPreview {
        files: targets.len(),
        bytes: targets.iter().map(|(_, len)| len).sum(),
        older_than_days,
    })
}

/// Borra los transcripts de subagente mas viejos que N dias.
///
/// No cambia ningun numero de la app: los turnos ya estan deduplicados en
/// SQLite y todas las consultas salen de ahi. Lo unico que se pierde es poder
/// hacer `--resume` de esas ramas de subagente.
#[tauri::command]
pub fn cleanup_subagents(state: State<'_, AppState>, older_than_days: u64) -> Res<CleanupPreview> {
    // Un dia de margen: nunca tocar lo que puede estar escribiendose ahora.
    if older_than_days < 1 {
        return Err("el minimo es 1 dia, para no tocar sesiones vivas".to_string());
    }
    let targets = cleanup_targets(&state, older_than_days);
    let mut files = 0;
    let mut bytes = 0;
    for (path, len) in targets {
        if std::fs::remove_file(&path).is_ok() {
            files += 1;
            bytes += len;
        }
    }
    Ok(CleanupPreview {
        files,
        bytes,
        older_than_days,
    })
}

/// Idioma de la interfaz y de las notificaciones. `en` por defecto.
///
/// Vive dentro de AlertConfig y no en su propia clave: los textos de alerta
/// los arma Rust, y dos fuentes de verdad para el idioma terminan en una
/// notificacion en un idioma y la ventana en el otro.
#[tauri::command]
pub fn lang(state: State<'_, AppState>) -> Res<String> {
    Ok(crate::alerts::load_config(&state).lang)
}

#[tauri::command]
pub fn set_lang(state: State<'_, AppState>, lang: String) -> Res<()> {
    if lang != "en" && lang != "es" {
        return Err(format!("unknown language: {lang}"));
    }
    let mut cfg = crate::alerts::load_config(&state);
    cfg.lang = lang;
    crate::alerts::save_config(&state, &cfg).map_err(err)
}

/// Las cuentas conocidas, incluidas las ocultas, para la pantalla de ajustes.
#[tauri::command]
pub fn profiles_list(state: State<'_, AppState>) -> Res<Vec<profiles::ProfileEntry>> {
    profiles::list_all(&state.profile_settings()).map_err(err)
}

/// Agrega un config dir a mano.
///
/// Se valida que tenga `projects/` adentro antes de guardarlo: una ruta mal
/// escrita que se acepta en silencio aparece despues como una cuenta vacia y
/// no hay forma de saber que fue un tipeo.
#[tauri::command]
pub fn profile_add(state: State<'_, AppState>, dir: String) -> Res<Vec<profiles::ProfileEntry>> {
    let path = profiles::expand_home(&dir);
    if !path.join("projects").is_dir() {
        return Err(format!(
            "{} no parece un config dir de Claude Code: no tiene projects/",
            path.display()
        ));
    }
    let mut settings = state.profile_settings();
    if !settings.extra_dirs.contains(&path) {
        settings.extra_dirs.push(path);
    }
    state.save_profile_settings(&settings).map_err(err)?;
    state.reload_profiles().map_err(err)?;
    profiles_list(state)
}

/// Muestra u oculta una cuenta.
///
/// Ocultar no borra nada: los turnos ya ingeridos siguen en la base y la
/// cuenta se puede volver a mostrar.
#[tauri::command]
pub fn profile_set_hidden(
    state: State<'_, AppState>,
    name: String,
    hidden: bool,
) -> Res<Vec<profiles::ProfileEntry>> {
    let mut settings = state.profile_settings();
    settings.hidden.retain(|n| n != &name);
    if hidden {
        settings.hidden.push(name);
    }
    state.save_profile_settings(&settings).map_err(err)?;
    state.reload_profiles().map_err(err)?;
    profiles_list(state)
}

/// Saca un config dir de la lista.
///
/// Los agregados a mano se olvidan; los descubiertos se anotan para que el
/// escaneo del home no los vuelva a traer. Ninguno de los dos borra datos: los
/// turnos ya ingeridos siguen en la base.
#[tauri::command]
pub fn profile_forget(state: State<'_, AppState>, dir: String) -> Res<Vec<profiles::ProfileEntry>> {
    let path = profiles::expand_home(&dir);
    let mut settings = state.profile_settings();
    let was_manual = settings.extra_dirs.iter().any(|d| d == &path);
    settings.extra_dirs.retain(|d| d != &path);
    if !was_manual && !settings.ignored_dirs.contains(&path) {
        settings.ignored_dirs.push(path);
    }
    state.save_profile_settings(&settings).map_err(err)?;
    state.reload_profiles().map_err(err)?;
    profiles_list(state)
}

/// Vuelve a traer todo lo que se habia quitado del escaneo.
#[tauri::command]
pub fn profiles_restore(state: State<'_, AppState>) -> Res<Vec<profiles::ProfileEntry>> {
    let mut settings = state.profile_settings();
    settings.ignored_dirs.clear();
    state.save_profile_settings(&settings).map_err(err)?;
    state.reload_profiles().map_err(err)?;
    profiles_list(state)
}

/// Cuantos config dirs se quitaron del escaneo, para poder ofrecer deshacer.
#[tauri::command]
pub fn profiles_ignored_count(state: State<'_, AppState>) -> Res<usize> {
    Ok(state.profile_settings().ignored_dirs.len())
}

/// Abre la ventana principal en el detalle de una sesion.
///
/// Es lo que hace util la lista de sesiones vivas del popover: ver que una se
/// esta inflando y poder mirarla sin buscarla a mano en la tabla.
#[tauri::command]
pub fn open_session(app: AppHandle, session_id: String) -> Res<()> {
    reveal_main_window(&app);
    app.emit("burn://open-session", session_id).map_err(err)?;
    Ok(())
}

/// Una sola sesion por id, sin filtro de periodo.
///
/// El popover puede pedir una sesion viva que no entra en el recorte actual
/// de la tabla; sin esto, abrir el detalle desde ahi fallaria justo cuando el
/// filtro esta en "Hoy".
#[tauri::command]
pub fn session_row(
    state: State<'_, AppState>,
    session_id: String,
) -> Res<Option<SessionWithBilling>> {
    let rows = sessions(state, Some(Filter::default()), Some(5000))?;
    Ok(rows.into_iter().find(|r| r.row.session_id == session_id))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dias_del_mes_incluye_febrero_bisiesto() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn el_ritmo_proyecta_sobre_los_dias_transcurridos() {
        // Mismo calculo que month_pace, con el dia fijo para poder afirmarlo.
        let (spent, day, days) = (300.0_f64, 10_u32, 30_u32);
        let projected = spent / f64::from(day) * f64::from(days);
        assert_eq!(projected, 900.0);

        // Lo que queda se reparte entre los dias que faltan, no entre todos.
        let budget = 1000.0_f64;
        let allowance = (budget - spent) / f64::from(days - day);
        assert_eq!(allowance, 35.0);
    }

    #[test]
    fn pasarse_del_techo_deja_el_diario_en_cero_y_no_en_negativo() {
        let (spent, budget, day, days) = (4920.0_f64, 1000.0_f64, 24_u32, 31_u32);
        let allowance = ((budget - spent) / f64::from(days - day)).max(0.0);
        assert_eq!(allowance, 0.0, "un diario negativo no significa nada");
        let projected = spent / f64::from(day) * f64::from(days);
        assert!(projected > budget * 6.0);
    }
}
