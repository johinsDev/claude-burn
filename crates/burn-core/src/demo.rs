//! Base de datos de ejemplo, para capturas y para probar la UI sin datos
//! propios.
//!
//! Genera turnos plausibles en vez de copiar reales: un repo publico no tiene
//! por que llevar los nombres de proyecto ni el gasto de nadie.

use crate::pricing::{cost_of, Usage};
use crate::record::Turn;
use crate::store::Store;
use anyhow::Result;

/// Marca en `settings` que esta base es de demo. La app la lee para no
/// sincronizar encima con los transcripts reales de la maquina.
pub const DEMO_KEY: &str = "demo_profiles";

struct Account {
    name: &'static str,
    email: &'static str,
    org: &'static str,
    billing: &'static str,
    /// Cuanto pesa esta cuenta en el gasto generado.
    weight: f64,
}

const ACCOUNTS: &[Account] = &[
    Account {
        name: "trabajo",
        email: "tu@empresa.com",
        org: "Empresa",
        billing: "overage",
        weight: 1.0,
    },
    Account {
        name: "personal",
        email: "vos@ejemplo.com",
        org: "Personal",
        billing: "flat",
        weight: 0.55,
    },
];

/// Sesiones de ejemplo: titulo, proyecto, cuenta, turnos y contexto maximo.
/// Los numeros estan elegidos para que se vea el patron real — las caras son
/// las que dejan crecer el contexto, no las que trabajan mas.
const SESSIONS: &[(&str, &str, &str, u32, u64)] = &[
    (
        "refactor-checkout-a-server-components",
        "tienda-web",
        "trabajo",
        420,
        940_000,
    ),
    ("migrar-auth-a-oauth", "tienda-web", "trabajo", 310, 880_000),
    (
        "bug-de-zona-horaria-en-reportes",
        "panel-admin",
        "trabajo",
        180,
        410_000,
    ),
    (
        "audit-de-accesibilidad",
        "tienda-web",
        "trabajo",
        260,
        620_000,
    ),
    ("importador-de-csv", "panel-admin", "trabajo", 140, 250_000),
    (
        "tests-e2e-del-carrito",
        "tienda-web",
        "trabajo",
        95,
        180_000,
    ),
    ("blog-con-astro", "sitio-personal", "personal", 210, 700_000),
    ("cli-de-notas", "notas", "personal", 150, 320_000),
    ("scraper-de-precios", "notas", "personal", 88, 190_000),
];

const MODELS: &[(&str, f64)] = &[
    ("claude-opus-5", 0.55),
    ("claude-fable-5", 0.25),
    ("claude-sonnet-5", 0.15),
    ("claude-haiku-4-5", 0.05),
];

/// Llena una base vacia con ~6 semanas de datos inventados.
pub fn seed(db: &mut Store) -> Result<()> {
    let today = chrono::Local::now().date_naive();
    let mut turns = Vec::new();
    let mut seed = 0x5eed_u64;
    // Generador propio y determinista: la misma base sale igual dos veces, y
    // eso es lo que hace comparables dos capturas.
    let mut next = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        f64::from((seed >> 32) as u32) / f64::from(u32::MAX)
    };

    for (idx, (title, project, account, n_turns, max_ctx)) in SESSIONS.iter().enumerate() {
        let acct = ACCOUNTS.iter().find(|a| a.name == *account).unwrap();
        let session_id = format!("demo-{idx:04}-{}", &title[..title.len().min(8)]);
        // Repartidas en las ultimas 6 semanas, con las mas caras cerca de hoy.
        let days_ago = 2 + (idx as i64 * 4) % 38;
        let day = today - chrono::Duration::days(days_ago);

        for t in 0..*n_turns {
            let progress = f64::from(t) / f64::from(*n_turns);
            // El contexto crece con la sesion: es el patron que la app existe
            // para mostrar.
            let ctx = (*max_ctx as f64 * (0.12 + 0.88 * progress)) as u64;
            let model = pick(MODELS, next());
            let out = 300 + (next() * 1400.0) as u64;
            let write = if t % 12 == 0 { ctx / 8 } else { 0 };
            let usage = Usage {
                input: 4,
                cache_write_5m: 0,
                cache_write_1h: write,
                cache_read: ctx.saturating_sub(write),
                output: out,
                web_searches: 0,
                geo_us: false,
                batch: false,
                fast: false,
            };
            let Some(cost) = cost_of(model, &usage) else {
                continue;
            };
            let minute = (t * 3) % 1440;
            let ts = day
                .and_hms_opt(u32::from(9 + (minute / 60) as u16 % 12), minute % 60, 0)
                .map(|d| d.and_utc().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
                .unwrap_or_default();

            turns.push(Turn {
                request_id: format!("req_demo_{idx}_{t}"),
                session_id: session_id.clone(),
                account: acct.name.to_string(),
                project: format!("-Users-vos-proyectos-{project}"),
                cwd: Some(format!("/Users/vos/proyectos/{project}")),
                git_branch: Some("main".to_string()),
                ts,
                model: crate::pricing::normalize_model_id(model),
                raw_model: model.to_string(),
                effort: Some(if next() > 0.4 { "high" } else { "medium" }.to_string()),
                usage,
                thinking_tokens: out / 3,
                context_tokens: ctx,
                is_sidechain: false,
                agent_id: None,
                cost,
            });

            // Algunas sesiones lanzan subagentes, que se facturan aparte.
            if idx % 3 == 0 && t % 40 == 0 {
                let mut sub = turns.last().unwrap().clone();
                sub.request_id = format!("req_demo_sub_{idx}_{t}");
                sub.agent_id = Some(format!("agent-{idx}-{t}"));
                sub.is_sidechain = true;
                turns.push(sub);
            }
        }

        // Un par de sesiones compactan: la app las marca y sin ninguna no se
        // veria para que sirve la columna.
        if idx % 4 == 1 {
            db.save_cursor(&crate::store::FileCursor {
                path: &format!("/demo/{session_id}.jsonl"),
                account: acct.name,
                project,
                session_id: &session_id,
                size: 1,
                mtime_ms: 0,
                offset: 1,
                compactions_delta: 1 + (idx as u32 % 3),
                meta_scanned: true,
            })?;
        }
        db.save_session_meta(&session_id, Some(title), None)?;
    }

    // Peso por cuenta: se descartan turnos de las cuentas mas livianas.
    turns.retain(|t| {
        let w = ACCOUNTS
            .iter()
            .find(|a| a.name == t.account)
            .map_or(1.0, |a| a.weight);
        w >= 1.0 || next() < w
    });

    db.insert_turns(&turns)?;

    let profiles: Vec<_> = ACCOUNTS
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "email": a.email,
                "org": a.org,
                "billing": a.billing,
            })
        })
        .collect();
    db.set_setting(DEMO_KEY, &serde_json::to_string(&profiles)?)?;
    Ok(())
}

/// Los perfiles de la base de demo, en la forma que espera el resto de la app.
///
/// Devuelve `Vec` vacio si la base no es de demo, que es lo que hace de
/// interruptor: sin perfiles no hay nada que sincronizar.
pub fn profiles(db: &Store) -> Result<Vec<crate::profiles::Profile>> {
    use crate::profiles::{Billing, Profile};
    let Some(raw) = db.get_setting(DEMO_KEY)? else {
        return Ok(Vec::new());
    };
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    Ok(parsed
        .iter()
        .map(|v| Profile {
            name: v["name"].as_str().unwrap_or("demo").to_string(),
            config_dir: std::path::PathBuf::from("/demo"),
            billing: match v["billing"].as_str() {
                Some("overage") => Billing::Overage,
                Some("flat") => Billing::Flat,
                _ => Billing::Unknown,
            },
            email: v["email"].as_str().map(str::to_string),
            org: v["org"].as_str().map(str::to_string),
            plan: None,
        })
        .collect())
}

fn pick(items: &[(&'static str, f64)], r: f64) -> &'static str {
    let mut acc = 0.0;
    for (name, w) in items {
        acc += w;
        if r <= acc {
            return name;
        }
    }
    items[0].0
}
