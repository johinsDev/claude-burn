//! Nucleo de claude-burn: ingesta, deduplicacion y costeo de los transcripts
//! de Claude Code. No depende de Tauri a proposito — se compila igual como
//! biblioteca para la app y como CLI (`burn-cli`) para verificar los numeros.

pub mod alerts;
pub mod demo;
pub mod ingest;
pub mod pricing;
pub mod profiles;
pub mod record;
pub mod store;

use anyhow::Result;
use profiles::Profile;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use store::Store;

pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("claude-burn")
        .join("burn.sqlite")
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub files_scanned: usize,
    pub files_changed: usize,
    pub turns_new: usize,
    pub turns_duplicate: usize,
    pub compactions: u32,
    pub unknown_models: Vec<String>,
    pub elapsed_ms: u128,
}

/// Sincroniza todos los perfiles contra la base.
///
/// Un archivo se salta entero si su tamano no cambio desde la ultima pasada,
/// que es lo que hace barata la lectura incremental: en estado estable solo se
/// tocan los transcripts de las sesiones vivas.
pub fn sync(db: &mut Store, profiles: &[Profile]) -> Result<SyncReport> {
    let started = Instant::now();
    let mut report = SyncReport::default();

    for profile in profiles {
        for file in ingest::walk_transcripts(profile) {
            report.files_scanned += 1;
            let path_str = file.path.to_string_lossy().to_string();
            let (stored_offset, stored_size) = db.cursor_for(&path_str)?;

            let Ok(md) = std::fs::metadata(&file.path) else {
                continue;
            };
            let size = md.len();
            let needs_meta = db.needs_meta_scan(&path_str)?;
            if size == stored_size && stored_offset > 0 && !needs_meta {
                continue;
            }
            report.files_changed += 1;

            // Si al archivo nunca se le busco titulo, hay que leerlo entero:
            // las lineas `ai-title` estan detras del offset guardado. Es una
            // sola vez por archivo, no en cada pasada.
            let from = if needs_meta {
                0
            } else {
                ingest::resume_offset(&file.path, stored_offset)
            };
            let scan = ingest::read_incremental(&file, &profile.name, from)?;
            db.save_session_meta(
                &file.session_id,
                scan.title.as_deref(),
                scan.first_prompt.as_deref(),
            )?;

            let new = db.insert_turns(&scan.turns)?;
            report.turns_new += new;
            report.turns_duplicate += scan.turns.len() - new;
            report.compactions += scan.compactions;
            for m in scan.unknown_models {
                if !report.unknown_models.contains(&m) {
                    report.unknown_models.push(m);
                }
            }

            let mtime_ms = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as i64);

            db.save_cursor(&store::FileCursor {
                path: &path_str,
                account: &profile.name,
                project: &file.project,
                session_id: &file.session_id,
                size,
                mtime_ms,
                offset: scan.new_offset,
                compactions_delta: scan.compactions,
                meta_scanned: from == 0,
            })?;
        }
    }

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

/// Todo el pipeline con la configuracion por defecto: descubrir perfiles,
/// abrir la base y sincronizar.
pub fn sync_default(db: &mut Store) -> Result<(Vec<Profile>, SyncReport)> {
    let profiles = profiles::discover()?;
    let report = sync(db, &profiles)?;
    Ok((profiles, report))
}
