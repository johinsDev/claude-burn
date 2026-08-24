//! Formas serde de las lineas de transcript que nos importan.
//!
//! Un `.jsonl` de Claude Code mezcla mas de quince tipos de linea
//! (`attachment`, `mode`, `ai-title`, `file-history-delta`, ...). Solo dos
//! aportan: `assistant` (un turno facturado) y `user` con `isCompactSummary`
//! (una compactacion). Todo lo demas se descarta antes de tocar serde.

use crate::pricing::{Cost, Usage};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct RawCacheCreation {
    #[serde(default)]
    pub ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawOutputDetails {
    #[serde(default)]
    pub thinking_tokens: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawServerToolUse {
    #[serde(default)]
    pub web_search_requests: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation: Option<RawCacheCreation>,
    #[serde(default)]
    pub output_tokens_details: Option<RawOutputDetails>,
    #[serde(default)]
    pub server_tool_use: Option<RawServerToolUse>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub inference_geo: Option<String>,
    #[serde(default)]
    pub speed: Option<String>,
}

impl RawUsage {
    /// Convierte los contadores crudos en la forma que consume el motor de precios.
    ///
    /// El split 5m/1h vive en `cache_creation`; cuando falta (transcripts viejos)
    /// se cae a `cache_creation_input_tokens` contado como escritura de 5 minutos,
    /// que es el default de la API.
    pub fn to_usage(&self) -> Usage {
        let (w5, w1) = match &self.cache_creation {
            Some(c) if c.ephemeral_5m_input_tokens > 0 || c.ephemeral_1h_input_tokens > 0 => {
                (c.ephemeral_5m_input_tokens, c.ephemeral_1h_input_tokens)
            }
            _ => (self.cache_creation_input_tokens, 0),
        };
        Usage {
            input: self.input_tokens,
            cache_write_5m: w5,
            cache_write_1h: w1,
            cache_read: self.cache_read_input_tokens,
            output: self.output_tokens,
            web_searches: self
                .server_tool_use
                .as_ref()
                .map_or(0, |s| s.web_search_requests),
            geo_us: self.inference_geo.as_deref() == Some("us"),
            batch: self.service_tier.as_deref() == Some("batch"),
            fast: self.speed.as_deref() == Some("fast"),
        }
    }

    pub fn thinking_tokens(&self) -> u64 {
        self.output_tokens_details
            .as_ref()
            .map_or(0, |d| d.thinking_tokens)
    }

    /// El tamano real del prompt en este turno: todo lo que el modelo leyo.
    /// Es la metrica central del proyecto — de aca sale la alerta de contexto inflado.
    pub fn context_tokens(&self) -> u64 {
        self.input_tokens + self.cache_read_input_tokens + self.cache_creation_input_tokens
    }
}

#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawLine {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub is_compact_summary: Option<bool>,
    #[serde(default)]
    pub version: Option<String>,
    /// Titulo generado por Claude Code para la sesion (lineas `ai-title`).
    #[serde(default)]
    pub ai_title: Option<String>,
    /// Prompt del usuario en el punto de retome (lineas `last-prompt`).
    #[serde(default)]
    pub last_prompt: Option<String>,
    #[serde(default)]
    pub message: Option<RawMessage>,
}

/// Un turno facturado, listo para persistir.
#[derive(Debug, Clone)]
pub struct Turn {
    /// Clave de deduplicacion: identifica una llamada a la API, no una linea.
    pub request_id: String,
    pub session_id: String,
    pub account: String,
    pub project: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub ts: String,
    /// Id canonico, o `None` si el modelo no esta en la tabla de precios.
    pub model: Option<&'static str>,
    pub raw_model: String,
    pub effort: Option<String>,
    pub usage: Usage,
    pub thinking_tokens: u64,
    pub context_tokens: u64,
    pub is_sidechain: bool,
    /// Id del subagente cuando el turno viene de un transcript de subagente.
    pub agent_id: Option<String>,
    /// Desglose por componente; `cost.total()` es el costo del turno.
    pub cost: Cost,
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = r#"{"input_tokens":2,"cache_creation_input_tokens":41552,
      "cache_read_input_tokens":25272,"output_tokens":431,
      "output_tokens_details":{"thinking_tokens":243},
      "server_tool_use":{"web_search_requests":0,"web_fetch_requests":0},
      "service_tier":"standard",
      "cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":41552},
      "inference_geo":"not_available","speed":"standard"}"#;

    #[test]
    fn parsea_usage_real_de_un_transcript() {
        let raw: RawUsage = serde_json::from_str(REAL).unwrap();
        let u = raw.to_usage();
        assert_eq!(u.input, 2);
        assert_eq!(u.cache_write_1h, 41552);
        assert_eq!(u.cache_write_5m, 0);
        assert_eq!(u.cache_read, 25272);
        assert_eq!(u.output, 431);
        // "not_available" no es "us": sin multiplicador
        assert!(!u.geo_us);
        assert!(!u.batch);
        assert!(!u.fast);
        assert_eq!(raw.thinking_tokens(), 243);
        assert_eq!(raw.context_tokens(), 2 + 25272 + 41552);
    }

    #[test]
    fn cae_a_cache_creation_plano_cuando_falta_el_split() {
        let raw: RawUsage =
            serde_json::from_str(r#"{"cache_creation_input_tokens":1000}"#).unwrap();
        let u = raw.to_usage();
        assert_eq!(u.cache_write_5m, 1000);
        assert_eq!(u.cache_write_1h, 0);
    }

    #[test]
    fn usage_vacio_no_explota() {
        let raw: RawUsage = serde_json::from_str("{}").unwrap();
        assert_eq!(raw.to_usage(), Usage::default());
    }
}
