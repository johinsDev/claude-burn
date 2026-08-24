//! Vigila los archivos que Claude Code escribe y reacciona en el momento.
//!
//! Sin esto la alerta de contexto inflado llegaria tarde, que es lo mismo que
//! no llegar: el punto es avisar *mientras* la sesion sigue abierta.

use crate::alerts::evaluate_and_notify;
use crate::state::AppState;
use notify::{Event, RecursiveMode, Watcher};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Ventana de espera tras el ultimo cambio antes de procesar.
///
/// Claude escribe muchas lineas seguidas; sincronizar en cada una desperdicia
/// trabajo y puede leer un JSON a medio escribir.
const DEBOUNCE: Duration = Duration::from_millis(900);

/// Aun sin cambios, se revisa cada tanto: los limites del plan se refrescan
/// en `.claude.json` sin que nada mas se mueva.
const IDLE_TICK: Duration = Duration::from_secs(90);

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        if let Err(e) = run(app) {
            eprintln!("[claude-burn] watcher detenido: {e}");
        }
    });
}

fn run(app: AppHandle) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;

    {
        let state = app.state::<AppState>();
        for profile in state.profiles.lock().unwrap().iter() {
            // Los transcripts en recursivo (los subagentes viven anidados);
            // sessions/ para saber que corre ahora; .claude.json para los
            // limites del plan.
            for (path, mode) in [
                (profile.projects_dir(), RecursiveMode::Recursive),
                (profile.sessions_dir(), RecursiveMode::NonRecursive),
                (profile.config_dir.clone(), RecursiveMode::NonRecursive),
            ] {
                if path.exists() {
                    if let Err(e) = watcher.watch(&path, mode) {
                        eprintln!("[claude-burn] no se pudo vigilar {}: {e}", path.display());
                    }
                }
            }
        }
    }

    let mut pending = false;
    let mut last_event = Instant::now();

    loop {
        match rx.recv_timeout(DEBOUNCE) {
            Ok(Ok(event)) if is_relevant(&event) => {
                pending = true;
                last_event = Instant::now();
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // El watcher se cayo: sin el, la app seguiria mostrando datos
            // viejos en silencio, asi que mejor terminar el hilo y decirlo.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        let idle_due = last_event.elapsed() >= IDLE_TICK;
        if (pending && last_event.elapsed() >= DEBOUNCE) || idle_due {
            pending = false;
            if idle_due {
                last_event = Instant::now();
            }
            process(&app);
        }
    }
    Ok(())
}

/// Filtra el ruido: solo importan los transcripts, el registro de sesiones
/// vivas y la config con los limites del plan.
fn is_relevant(event: &Event) -> bool {
    event.paths.iter().any(|p| {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        name.ends_with(".jsonl") || name.ends_with(".json")
    })
}

fn process(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match state.sync() {
        Ok(new_turns) => {
            crate::tray::refresh_tray(app);
            evaluate_and_notify(app, &state);
            if new_turns > 0 {
                let _ = app.emit("burn://refreshed", new_turns);
            }
        }
        Err(e) => eprintln!("[claude-burn] fallo la sincronizacion: {e}"),
    }
}
