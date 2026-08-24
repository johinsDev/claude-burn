//! Descubrimiento de cuentas y lectura de los datos "oficiales" que Claude Code
//! ya deja en disco. Nada de red: ni la API de Anthropic ni los tokens de
//! `.credentials.json`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Como se factura una cuenta. Determina si el $ calculado es plata real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Billing {
    /// Suscripcion con overage deshabilitado: el $ es valor teorico, no facturado.
    Flat,
    /// Overage habilitado: cada token por encima del plan se factura.
    Overage,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub config_dir: PathBuf,
    pub billing: Billing,
    pub email: Option<String>,
    pub org: Option<String>,
    pub plan: Option<String>,
}

impl Profile {
    pub fn projects_dir(&self) -> PathBuf {
        self.config_dir.join("projects")
    }
    pub fn sessions_dir(&self) -> PathBuf {
        self.config_dir.join("sessions")
    }
    pub fn config_json(&self) -> PathBuf {
        self.config_dir.join(".claude.json")
    }
}

/// Deriva un nombre de perfil desde el nombre del directorio:
/// `.claude-cruisebound` -> `cruisebound`, `.claude` -> `default`.
fn profile_name(dir: &Path) -> String {
    let base = dir.file_name().and_then(|s| s.to_str()).unwrap_or("claude");
    match base.strip_prefix(".claude-") {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => "default".to_string(),
    }
}

/// Busca directorios de configuracion de Claude Code en el home: `.claude` y
/// cualquier `.claude-*`. Solo cuenta los que tienen un `projects/` adentro.
pub fn discover() -> Result<Vec<Profile>> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("sin home dir"))?;
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&home)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name != ".claude" && !name.starts_with(".claude-") {
            continue;
        }
        if !path.join("projects").is_dir() {
            continue;
        }
        found.push(from_config_dir(path));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Construye un perfil leyendo `oauthAccount` de `.claude.json`.
///
/// `hasExtraUsageEnabled` es lo que distingue plata real de tarifa plana: una
/// cuenta con overage deshabilitado no puede pasarse del precio del plan.
pub fn from_config_dir(config_dir: PathBuf) -> Profile {
    let name = profile_name(&config_dir);
    let mut p = Profile {
        name,
        config_dir: config_dir.clone(),
        billing: Billing::Unknown,
        email: None,
        org: None,
        plan: None,
    };
    let Ok(text) = std::fs::read_to_string(config_dir.join(".claude.json")) else {
        return p;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return p;
    };
    let acct = &v["oauthAccount"];
    p.email = acct["emailAddress"].as_str().map(str::to_string);
    p.org = acct["organizationName"].as_str().map(str::to_string);
    p.plan = acct["organizationType"].as_str().map(str::to_string);
    p.billing = match acct["hasExtraUsageEnabled"].as_bool() {
        Some(true) => Billing::Overage,
        Some(false) => Billing::Flat,
        None => Billing::Unknown,
    };
    p
}

/// Un limite del plan tal como lo reporta Anthropic, leido del cache que
/// Claude Code refresca solo en `.claude.json`.
#[derive(Debug, Clone, Serialize)]
pub struct PlanLimit {
    /// `session` (5h), `weekly_all` (7 dias), `weekly_scoped` (por modelo)
    pub kind: String,
    pub group: String,
    pub percent: f64,
    pub severity: String,
    pub resets_at: Option<String>,
    pub scope: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanUsage {
    /// Cuando Claude Code refresco este cache. La UI muestra la antiguedad.
    pub fetched_at_ms: Option<i64>,
    pub limits: Vec<PlanLimit>,
    /// Gasto de overage en USD, cuando la cuenta lo tiene habilitado.
    pub extra_usage_spent: Option<f64>,
    pub extra_usage_limit: Option<f64>,
    pub extra_usage_enabled: bool,
}

/// Lee `cachedUsageUtilization` de `.claude.json`. Es el numero oficial de
/// Anthropic — se muestra tal cual, con un badge de antiguedad, y nunca se
/// mezcla con el $ calculado desde los transcripts.
pub fn read_plan_usage(profile: &Profile) -> Option<PlanUsage> {
    let text = std::fs::read_to_string(profile.config_json()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let cached = v.get("cachedUsageUtilization")?;
    if cached.is_null() {
        return None;
    }
    let util = cached.get("utilization")?;

    let limits = util["limits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|l| PlanLimit {
                    kind: l["kind"].as_str().unwrap_or("").to_string(),
                    group: l["group"].as_str().unwrap_or("").to_string(),
                    percent: l["percent"].as_f64().unwrap_or(0.0),
                    severity: l["severity"].as_str().unwrap_or("normal").to_string(),
                    resets_at: l["resets_at"].as_str().map(str::to_string),
                    scope: l["scope"]["model"]["display_name"]
                        .as_str()
                        .map(str::to_string),
                    is_active: l["is_active"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();

    let extra = &util["extra_usage"];
    let spend = &util["spend"];
    // `spend.used` viene en unidades menores (centavos) con su exponente.
    let extra_usage_spent = spend["used"]["amount_minor"].as_f64().map(|minor| {
        let exp = spend["used"]["exponent"].as_i64().unwrap_or(2) as i32;
        minor / 10f64.powi(exp)
    });

    Some(PlanUsage {
        fetched_at_ms: cached["fetchedAtMs"].as_i64(),
        limits,
        extra_usage_spent,
        extra_usage_limit: extra["monthly_limit"].as_f64(),
        extra_usage_enabled: extra["is_enabled"].as_bool().unwrap_or(false),
    })
}

/// Una sesion de Claude Code corriendo ahora mismo.
#[derive(Debug, Clone, Serialize)]
pub struct LiveSession {
    pub pid: i64,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    /// `busy` o `idle`
    pub status: Option<String>,
    pub started_at_ms: Option<i64>,
    pub version: Option<String>,
    pub account: String,
}

/// Lee `<config_dir>/sessions/<pid>.json`, el registro que Claude Code mantiene
/// de sus procesos vivos. Es lo que permite alertar de contexto inflado
/// mientras la sesion sigue abierta, en vez de despues.
pub fn read_live_sessions(profile: &Profile) -> Vec<LiveSession> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(profile.sessions_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let (Some(pid), Some(session_id)) = (v["pid"].as_i64(), v["sessionId"].as_str()) else {
            continue;
        };
        out.push(LiveSession {
            pid,
            session_id: session_id.to_string(),
            cwd: v["cwd"].as_str().unwrap_or("").to_string(),
            name: v["name"].as_str().map(str::to_string),
            status: v["status"].as_str().map(str::to_string),
            started_at_ms: v["startedAt"].as_i64(),
            version: v["version"].as_str().map(str::to_string),
            account: profile.name.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deriva_nombre_de_perfil() {
        assert_eq!(
            profile_name(Path::new("/Users/x/.claude-cruisebound")),
            "cruisebound"
        );
        assert_eq!(
            profile_name(Path::new("/Users/x/.claude-personal")),
            "personal"
        );
        assert_eq!(profile_name(Path::new("/Users/x/.claude")), "default");
    }
}
