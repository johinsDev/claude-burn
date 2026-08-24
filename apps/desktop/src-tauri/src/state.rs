//! Estado compartido de la app: la base y el snapshot que alimenta el tray.

use anyhow::Result;
use burn_core::profiles::Profile;
use burn_core::store::Store;
use serde::Serialize;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Store>,
    pub profiles: Mutex<Vec<Profile>>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let db = Store::open(&burn_core::default_db_path())?;
        let profiles = burn_core::profiles::discover()?;
        Ok(Self {
            db: Mutex::new(db),
            profiles: Mutex::new(profiles),
        })
    }

    /// Sincroniza y devuelve cuantos turnos nuevos entraron.
    pub fn sync(&self) -> Result<usize> {
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
        // Con un techo mensual, el porcentaje que importa es ese. El del plan
        // sale de la cuenta de tarifa plana y no dice nada sobre la factura.
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
