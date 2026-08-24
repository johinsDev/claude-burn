//! Lectura incremental de los transcripts.
//!
//! Hoy son ~435 archivos y ~1 GB. Releerlos enteros en cada tick no es opcion,
//! asi que cada archivo guarda un offset de bytes y solo se lee lo nuevo.

use crate::pricing;
use crate::profiles::Profile;
use crate::record::{RawLine, Turn};
use anyhow::Result;
use memchr::memmem;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn finder_assistant() -> &'static memmem::Finder<'static> {
    static F: OnceLock<memmem::Finder<'static>> = OnceLock::new();
    F.get_or_init(|| memmem::Finder::new(b"\"assistant\""))
}

fn finder_compact() -> &'static memmem::Finder<'static> {
    static F: OnceLock<memmem::Finder<'static>> = OnceLock::new();
    F.get_or_init(|| memmem::Finder::new(b"\"isCompactSummary\""))
}

#[derive(Debug)]
pub struct TranscriptFile {
    pub path: PathBuf,
    pub project: String,
    /// Sesion a la que pertenece. Para un subagente es la sesion *padre*, asi
    /// que su costo se suma a la sesion que lo lanzo.
    pub session_id: String,
    /// `Some(id)` si el archivo es el transcript de un subagente.
    pub agent_id: Option<String>,
    /// `Some(wf_...)` si el subagente corrio dentro de un workflow.
    pub workflow_id: Option<String>,
}

/// Lista los `.jsonl` de un perfil.
///
/// Hay tres formas, y perderse las dos ultimas subestima el gasto:
///
///   `<proyecto>/<uuid>.jsonl`                                  sesion principal
///   `<proyecto>/<uuid>/subagents/agent-<id>.jsonl`             subagente
///   `<proyecto>/<uuid>/subagents/workflows/wf_<id>/*.jsonl`    agente de workflow
///
/// Los archivos de subagente son hoy 306 y 107 MB de turnos facturados; los de
/// workflow, otros 86. Todos se atribuyen a la sesion `<uuid>` que los lanzo.
pub fn walk_transcripts(profile: &Profile) -> Vec<TranscriptFile> {
    let root = profile.projects_dir();
    let mut out = Vec::new();

    for entry in walkdir::WalkDir::new(&root)
        .min_depth(2)
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|s| s.to_str()) != Some("jsonl")
        {
            continue;
        }
        let Ok(rel) = path.strip_prefix(&root) else {
            continue;
        };
        let parts: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
        let Some(project) = parts.first() else {
            continue;
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        let (session_id, agent_id, workflow_id) = match parts.as_slice() {
            // <proyecto>/<uuid>.jsonl
            [_, _file] => (stem.to_string(), None, None),
            // <proyecto>/<uuid>/subagents/**/<file>.jsonl — a cualquier profundidad,
            // que es lo que cubre tanto los subagentes sueltos como los de workflow.
            [_, session, "subagents", rest @ ..] if !rest.is_empty() => (
                session.to_string(),
                Some(stem.to_string()),
                rest.iter()
                    .find(|p| p.starts_with("wf_"))
                    .map(|p| p.to_string()),
            ),
            _ => continue,
        };

        out.push(TranscriptFile {
            path: path.to_path_buf(),
            project: project.to_string(),
            session_id,
            agent_id,
            workflow_id,
        });
    }
    out
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub turns: Vec<Turn>,
    pub compactions: u32,
    /// Offset tras la ultima linea *completa*. Nunca apunta a media linea.
    pub new_offset: u64,
    pub lines_seen: u64,
    pub unknown_models: Vec<String>,
}

/// Lee un transcript desde `from_offset` y devuelve los turnos nuevos.
///
/// Solo avanza el offset sobre lineas terminadas en `\n`: si Claude esta
/// escribiendo en este momento, la ultima linea parcial queda para la proxima
/// pasada en vez de perderse o parsearse a medias.
pub fn read_incremental(
    file: &TranscriptFile,
    account: &str,
    from_offset: u64,
) -> Result<ScanResult> {
    let mut out = ScanResult {
        new_offset: from_offset,
        ..Default::default()
    };
    let mut fh = File::open(&file.path)?;
    fh.seek(SeekFrom::Start(from_offset))?;
    let mut reader = BufReader::with_capacity(256 * 1024, fh);
    let mut buf = Vec::with_capacity(8192);

    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if !buf.ends_with(b"\n") {
            // linea incompleta: la sesion sigue escribiendo. No avanzamos.
            break;
        }
        out.new_offset += n as u64;
        out.lines_seen += 1;

        let has_assistant = finder_assistant().find(&buf).is_some();
        let has_compact = finder_compact().find(&buf).is_some();
        if !has_assistant && !has_compact {
            continue;
        }

        let Ok(line) = serde_json::from_slice::<RawLine>(&buf) else {
            continue;
        };

        if has_compact && line.is_compact_summary == Some(true) {
            out.compactions += 1;
        }
        if line.kind.as_deref() != Some("assistant") {
            continue;
        }
        if let Some(turn) = build_turn(&line, file, account, &mut out.unknown_models) {
            out.turns.push(turn);
        }
    }

    Ok(out)
}

fn build_turn(
    line: &RawLine,
    file: &TranscriptFile,
    account: &str,
    unknown: &mut Vec<String>,
) -> Option<Turn> {
    let message = line.message.as_ref()?;
    let raw_usage = message.usage.as_ref()?;

    // `requestId` identifica una llamada facturada. Cuando falta (transcripts
    // viejos) el id del mensaje sirve igual como clave de deduplicacion.
    let request_id = line.request_id.clone().or_else(|| message.id.clone())?;

    let raw_model = message.model.clone().unwrap_or_default();
    let model = pricing::normalize_model_id(&raw_model);
    if model.is_none()
        && !raw_model.is_empty()
        && !pricing::is_not_billed(&raw_model)
        && !unknown.contains(&raw_model)
    {
        unknown.push(raw_model.clone());
    }

    let usage = raw_usage.to_usage();
    let cost = model
        .and_then(|m| pricing::cost_of(m, &usage))
        .unwrap_or_default();

    Some(Turn {
        request_id,
        session_id: line
            .session_id
            .clone()
            .unwrap_or_else(|| file.session_id.clone()),
        account: account.to_string(),
        project: file.project.clone(),
        cwd: line.cwd.clone(),
        git_branch: line.git_branch.clone(),
        ts: line.timestamp.clone().unwrap_or_default(),
        model,
        raw_model,
        effort: line.effort.clone(),
        usage,
        thinking_tokens: raw_usage.thinking_tokens(),
        context_tokens: raw_usage.context_tokens(),
        is_sidechain: line.is_sidechain.unwrap_or(false),
        agent_id: file.agent_id.clone(),
        cost,
    })
}

/// Decide desde que offset leer, dado lo que sabemos del archivo.
///
/// Si el archivo encogio respecto al offset guardado fue reescrito (una sesion
/// reanudada, un truncado), asi que se relee entero.
pub fn resume_offset(path: &Path, stored_offset: u64) -> u64 {
    match std::fs::metadata(path) {
        Ok(md) if md.len() >= stored_offset => stored_offset,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(lines: &[&str]) -> (tempdir::Dir, TranscriptFile) {
        let dir = tempdir::Dir::new();
        let path = dir.path().join("session-a.jsonl");
        let mut f = File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        let tf = TranscriptFile {
            path,
            project: "proj".into(),
            session_id: "session-a".into(),
            agent_id: None,
            workflow_id: None,
        };
        (dir, tf)
    }

    const ASSISTANT: &str = r#"{"type":"assistant","sessionId":"s1","requestId":"req_1","timestamp":"2026-08-21T17:22:49.154Z","cwd":"/tmp","gitBranch":"main","effort":"high","isSidechain":false,"message":{"id":"msg_1","model":"claude-opus-5","usage":{"input_tokens":1000,"cache_read_input_tokens":2000000,"output_tokens":1000,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":0}}}}"#;

    #[test]
    fn extrae_turno_y_calcula_costo() {
        let (_d, tf) = fixture(&[ASSISTANT]);
        let r = read_incremental(&tf, "personal", 0).unwrap();
        assert_eq!(r.turns.len(), 1);
        let t = &r.turns[0];
        assert_eq!(t.request_id, "req_1");
        assert_eq!(t.model, Some("claude-opus-5"));
        assert_eq!(t.context_tokens, 1000 + 2_000_000);
        // 1000·5 + 2M·0.5 + 1000·25 en $/MTok = 0.005 + 1.0 + 0.025
        assert!((t.cost.total() - 1.03).abs() < 1e-9);
    }

    #[test]
    fn ignora_lineas_que_no_son_turnos() {
        let (_d, tf) = fixture(&[
            r#"{"type":"mode","mode":"default","sessionId":"s1"}"#,
            r#"{"type":"ai-title","aiTitle":"algo","sessionId":"s1"}"#,
            ASSISTANT,
        ]);
        let r = read_incremental(&tf, "personal", 0).unwrap();
        assert_eq!(r.lines_seen, 3);
        assert_eq!(r.turns.len(), 1);
    }

    #[test]
    fn cuenta_compactaciones() {
        let (_d, tf) = fixture(&[
            r#"{"type":"user","isCompactSummary":true,"sessionId":"s1","message":{}}"#,
            ASSISTANT,
        ]);
        let r = read_incremental(&tf, "personal", 0).unwrap();
        assert_eq!(r.compactions, 1);
    }

    #[test]
    fn lectura_incremental_no_repite_turnos() {
        let (_d, tf) = fixture(&[ASSISTANT]);
        let first = read_incremental(&tf, "personal", 0).unwrap();
        assert_eq!(first.turns.len(), 1);
        let second = read_incremental(&tf, "personal", first.new_offset).unwrap();
        assert_eq!(second.turns.len(), 0, "no debe releer lo ya procesado");
    }

    #[test]
    fn linea_incompleta_no_avanza_el_offset() {
        let dir = tempdir::Dir::new();
        let path = dir.path().join("s.jsonl");
        // segunda linea sin \n: Claude esta escribiendo ahora mismo
        std::fs::write(&path, format!("{ASSISTANT}\n{{\"type\":\"assist")).unwrap();
        let tf = TranscriptFile {
            path,
            project: "p".into(),
            session_id: "s".into(),
            agent_id: None,
            workflow_id: None,
        };
        let r = read_incremental(&tf, "personal", 0).unwrap();
        assert_eq!(r.turns.len(), 1);
        assert_eq!(r.new_offset as usize, ASSISTANT.len() + 1);
    }

    #[test]
    fn archivo_reescrito_se_relee_entero() {
        let dir = tempdir::Dir::new();
        let path = dir.path().join("s.jsonl");
        std::fs::write(&path, "corto\n").unwrap();
        assert_eq!(resume_offset(&path, 99_999), 0);
        assert_eq!(resume_offset(&path, 3), 3);
    }

    /// tempdir minimo para no sumar una dependencia por seis tests.
    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!(
                    "burn-test-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
