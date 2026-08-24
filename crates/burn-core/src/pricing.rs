//! Motor de precios. La tabla vive en `packages/pricing/pricing.json`, la misma
//! que consume el frontend, embebida en el binario con `include_str!` para que
//! Rust y TypeScript nunca queden desincronizados.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const PRICING_JSON: &str = include_str!("../../../packages/pricing/pricing.json");

#[derive(Debug, Deserialize)]
pub struct FastRate {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Deserialize)]
pub struct ModelRate {
    pub id: String,
    pub label: String,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub fast: Option<FastRate>,
}

#[derive(Debug, Deserialize)]
pub struct Multipliers {
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub inference_geo_us: f64,
    pub batch: f64,
}

#[derive(Debug, Deserialize)]
pub struct PricingTable {
    pub updated: String,
    pub multipliers: Multipliers,
    pub web_search_per_1k_requests: f64,
    pub models: Vec<ModelRate>,
}

pub fn table() -> &'static PricingTable {
    static T: OnceLock<PricingTable> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(PRICING_JSON).expect("pricing.json invalido"))
}

fn index() -> &'static HashMap<&'static str, &'static ModelRate> {
    static I: OnceLock<HashMap<&'static str, &'static ModelRate>> = OnceLock::new();
    I.get_or_init(|| table().models.iter().map(|m| (m.id.as_str(), m)).collect())
}

/// Modelos que aparecen en los transcripts pero no se facturan: mensajes que
/// Claude Code genera localmente. No son "modelo desconocido" — su costo real
/// es cero y no deben ensuciar el aviso de precios faltantes.
pub const NOT_BILLED: &[&str] = &["<synthetic>"];

pub fn is_not_billed(raw: &str) -> bool {
    NOT_BILLED.contains(&raw)
}

/// Normaliza el model id de un transcript al id canonico de la tabla.
///
/// Claude Code escribe variantes como `claude-opus-5[1m]` (sufijo de ventana de
/// contexto) y ocasionalmente ids con fecha (`claude-sonnet-4-5-20250929`).
/// Devuelve `None` si no hay coincidencia: el caller debe marcar el turno como
/// modelo desconocido en vez de inventar un precio.
pub fn normalize_model_id(raw: &str) -> Option<&'static str> {
    let stripped = raw.strip_suffix("[1m]").unwrap_or(raw);
    let idx = index();
    if let Some(m) = idx.get(stripped) {
        return Some(m.id.as_str());
    }
    // sufijo de fecha: -YYYYMMDD
    let undated = match stripped.rfind('-') {
        Some(p)
            if stripped[p + 1..].len() == 8
                && stripped[p + 1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            &stripped[..p]
        }
        _ => stripped,
    };
    if let Some(m) = idx.get(undated) {
        return Some(m.id.as_str());
    }
    // prefijo canonico mas largo que coincida
    idx.keys()
        .filter(|id| undated.starts_with(**id))
        .max_by_key(|id| id.len())
        .map(|id| idx[id].id.as_str())
}

pub fn label_for(model_id: &str) -> &'static str {
    index()
        .get(model_id)
        .map(|m| m.label.as_str())
        .unwrap_or("desconocido")
}

/// Los contadores de tokens de un turno, ya desambiguados.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub output: u64,
    pub web_searches: u64,
    /// `inference_geo == "us"` -> multiplicador 1.1 sobre todo
    pub geo_us: bool,
    /// `service_tier == "batch"` -> 0.5
    pub batch: bool,
    /// `speed == "fast"` -> tarifa premium en Opus 5 / 4.8
    pub fast: bool,
}

/// Desglose del costo de un turno, en USD. Alimenta la dona de composicion.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cost {
    pub fresh_input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
    pub web_search: f64,
}

impl Cost {
    pub fn total(&self) -> f64 {
        self.fresh_input
            + self.cache_write_5m
            + self.cache_write_1h
            + self.cache_read
            + self.output
            + self.web_search
    }
}

/// Calcula el costo de un turno a precio de lista.
///
/// `cost = in·base + w5m·base·1.25 + w1h·base·2.0 + read·base·0.1 + out·out_rate`
/// mas las busquedas web, todo escalado por los multiplicadores de geo y batch.
/// Devuelve `None` para modelos que no estan en la tabla.
pub fn cost_of(model_id: &str, u: &Usage) -> Option<Cost> {
    let rate = index().get(model_id)?;
    let (base_in, base_out) = match (&rate.fast, u.fast) {
        (Some(f), true) => (f.input, f.output),
        _ => (rate.input, rate.output),
    };

    let m = &table().multipliers;
    let mut scale = 1.0;
    if u.geo_us {
        scale *= m.inference_geo_us;
    }
    if u.batch {
        scale *= m.batch;
    }

    const MTOK: f64 = 1_000_000.0;
    let per = |tokens: u64, rate: f64| (tokens as f64) * rate * scale / MTOK;

    Some(Cost {
        fresh_input: per(u.input, base_in),
        cache_write_5m: per(u.cache_write_5m, base_in * m.cache_write_5m),
        cache_write_1h: per(u.cache_write_1h, base_in * m.cache_write_1h),
        cache_read: per(u.cache_read, base_in * m.cache_read),
        output: per(u.output, base_out),
        // las busquedas web no llevan el multiplicador de tokens
        web_search: (u.web_searches as f64) * table().web_search_per_1k_requests / 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_sufijo_de_ventana() {
        assert_eq!(
            normalize_model_id("claude-opus-5[1m]"),
            Some("claude-opus-5")
        );
        assert_eq!(normalize_model_id("claude-fable-5"), Some("claude-fable-5"));
    }

    #[test]
    fn normaliza_sufijo_de_fecha() {
        assert_eq!(
            normalize_model_id("claude-sonnet-4-5-20250929"),
            Some("claude-sonnet-4-5")
        );
    }

    #[test]
    fn sintetico_no_cuenta_como_desconocido() {
        assert!(is_not_billed("<synthetic>"));
        assert!(!is_not_billed("claude-opus-5"));
    }

    #[test]
    fn modelo_desconocido_no_inventa_precio() {
        assert_eq!(normalize_model_id("gpt-5-turbo"), None);
        assert!(cost_of("gpt-5-turbo", &Usage::default()).is_none());
    }

    #[test]
    fn costo_basico_opus5() {
        // 1M de cada categoria en Opus 5: 5 + 6.25 + 10 + 0.5 + 25
        let u = Usage {
            input: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            output: 1_000_000,
            ..Default::default()
        };
        let c = cost_of("claude-opus-5", &u).unwrap();
        assert!((c.fresh_input - 5.0).abs() < 1e-9);
        assert!((c.cache_write_5m - 6.25).abs() < 1e-9);
        assert!((c.cache_write_1h - 10.0).abs() < 1e-9);
        assert!((c.cache_read - 0.5).abs() < 1e-9);
        assert!((c.output - 25.0).abs() < 1e-9);
        assert!((c.total() - 46.75).abs() < 1e-9);
    }

    #[test]
    fn fable_cuesta_el_doble_que_opus() {
        let u = Usage {
            output: 1_000_000,
            ..Default::default()
        };
        let fable = cost_of("claude-fable-5", &u).unwrap().total();
        let opus = cost_of("claude-opus-5", &u).unwrap().total();
        assert!((fable - 2.0 * opus).abs() < 1e-9);
    }

    #[test]
    fn fast_mode_solo_donde_existe() {
        let u = Usage {
            output: 1_000_000,
            fast: true,
            ..Default::default()
        };
        // Opus 5 con fast mode se factura a tarifa premium
        assert!((cost_of("claude-opus-5", &u).unwrap().output - 50.0).abs() < 1e-9);
        // Opus 4.7 no tiene fast mode: la bandera no cambia nada
        assert!((cost_of("claude-opus-4-7", &u).unwrap().output - 25.0).abs() < 1e-9);
    }

    #[test]
    fn multiplicadores_de_geo_y_batch() {
        let u = Usage {
            output: 1_000_000,
            geo_us: true,
            ..Default::default()
        };
        assert!((cost_of("claude-opus-5", &u).unwrap().output - 27.5).abs() < 1e-9);
        let u = Usage {
            output: 1_000_000,
            batch: true,
            ..Default::default()
        };
        assert!((cost_of("claude-opus-5", &u).unwrap().output - 12.5).abs() < 1e-9);
    }

    #[test]
    fn busqueda_web_a_10_por_mil() {
        let u = Usage {
            web_searches: 100,
            ..Default::default()
        };
        assert!((cost_of("claude-opus-5", &u).unwrap().web_search - 1.0).abs() < 1e-9);
    }
}
