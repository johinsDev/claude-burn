//! Estado compartido de la app: la base y el snapshot que alimenta el tray.

use anyhow::Result;
use burn_core::profiles::{Profile, ProfileSettings};
use burn_core::store::Store;
use serde::Serialize;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Store>,
    /// `true` cuando la base es de demo: no se sincroniza ni se vigila disco.
    pub demo: bool,
    pub profiles: Mutex<Vec<Profile>>,
}

/// Clave de los ajustes de cuentas dentro de `settings`.
pub const PROFILES_KEY: &str = "profile_settings";

impl AppState {
    pub fn new() -> Result<Self> {
        // `BURN_DB` apunta a otra base: es lo que permite abrir la de demo sin
        // tocar la real.
        let path = std::env::var("BURN_DB")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| burn_core::default_db_path());
        let db = Store::open(&path)?;
        // Una base de demo trae sus propias cuentas y no se sincroniza: la
        // primera pasada la llenaria con los transcripts reales de la maquina.
        let demo = burn_core::demo::profiles(&db)?;
        let is_demo = !demo.is_empty();
        let profiles = if is_demo {
            demo
        } else {
            burn_core::profiles::active(&read_profile_settings(&db))?
        };
        Ok(Self {
            db: Mutex::new(db),
            demo: is_demo,
            profiles: Mutex::new(profiles),
        })
    }

    /// Relee los ajustes de cuentas y rearma la lista activa.
    ///
    /// Se llama despues de agregar u ocultar una cuenta: sin esto el cambio
    /// no se veria hasta reiniciar la app.
    pub fn reload_profiles(&self) -> Result<Vec<Profile>> {
        let profiles = {
            let db = self.db.lock().unwrap();
            burn_core::profiles::active(&read_profile_settings(&db))?
        };
        *self.profiles.lock().unwrap() = profiles.clone();
        Ok(profiles)
    }

    pub fn profile_settings(&self) -> ProfileSettings {
        let db = self.db.lock().unwrap();
        read_profile_settings(&db)
    }

    pub fn save_profile_settings(&self, s: &ProfileSettings) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.set_setting(PROFILES_KEY, &serde_json::to_string(s)?)?;
        Ok(())
    }
}

fn read_profile_settings(db: &Store) -> ProfileSettings {
    db.get_setting(PROFILES_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

impl AppState {
    /// Sincroniza y devuelve cuantos turnos nuevos entraron.
    pub fn sync(&self) -> Result<usize> {
        if self.demo {
            return Ok(0);
        }
        let profiles = self.profiles.lock().unwrap().clone();
        let mut db = self.db.lock().unwrap();
        Ok(burn_core::sync(&mut db, &profiles)?.turns_new)
    }
}

/// Lo minimo que necesita la barra de menu: gasto de hoy y el limite mas apretado.
#[derive(Debug, Serialize, Clone, Default)]
pub struct TraySummary {
    pub today_usd: f64,
    /// Solo el gasto de cuentas con overage: la plata que realmente se factura.
    pub today_billable_usd: f64,
    /// Gasto facturable del mes calendario en curso.
    pub month_billable_usd: f64,
    /// Techo mensual configurado, si hay uno.
    pub month_budget_usd: Option<f64>,
    /// Techo diario, que es el que acompana al $ de hoy en el icono.
    pub day_budget_usd: Option<f64>,
    pub worst_limit_pct: Option<f64>,
    pub worst_limit_kind: Option<String>,
    pub live_sessions: usize,
    /// Contexto de la sesion viva mas cargada, en tokens.
    pub max_live_ctx: Option<i64>,
}

impl TraySummary {
    /// Texto que va al lado del icono. Corto a proposito: comparte la barra.
    pub fn title(&self) -> String {
        let money = if self.today_billable_usd >= 100.0 {
            format!("${:.0}", self.today_billable_usd)
        } else {
            format!("${:.1}", self.today_billable_usd)
        };
        // El $ del icono es el de hoy, asi que el % tambien: dos periodos
        // distintos pegados no se leen, se confunden.
        if let Some(budget) = self.day_budget_usd.filter(|b| *b > 0.0) {
            let pct = self.today_billable_usd / budget * 100.0;
            return format!("{money} · {pct:.0}%");
        }
        // Sin techo diario, el mensual es lo mejor que hay.
        if let Some(budget) = self.month_budget_usd.filter(|b| *b > 0.0) {
            let pct = self.month_billable_usd / budget * 100.0;
            return format!("{money} · {pct:.0}% mes");
        }
        match self.worst_limit_pct {
            Some(p) => format!("{money} · {p:.0}%"),
            None => money,
        }
    }
}
