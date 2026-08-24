//! Shell de escritorio de claude-burn.
//!
//! Toda la logica de costeo vive en `burn-core`; aca solo estan la ventana, el
//! icono de la barra de menu y el puente hacia el frontend.

mod alerts;
mod commands;
mod state;
mod tray;
mod watcher;

use state::AppState;
use tauri::{Manager, WindowEvent};

/// Marca que el sistema arranco la app sola al iniciar sesion.
const AUTOSTART_FLAG: &str = "--autostart";

pub fn run() {
    tauri::Builder::default()
        // Va primero, como pide el plugin. Sin esto un segundo doble-click
        // arranca *otra* copia: dos iconos en la barra, dos watchers
        // escribiendo la misma base y las alertas duplicadas.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // La copia que ya corria es la que le contesta al usuario.
            commands::reveal_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        // El arranque automatico se marca con un argumento para poder
        // distinguirlo de un doble-click: uno arranca escondido, el otro no.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::sync_now,
            commands::sessions,
            commands::session_timeline,
            commands::models,
            commands::context_histogram,
            commands::budgets,
            commands::set_budget,
            commands::open_session,
            commands::session_row,
            commands::show_main_window,
            commands::hide_main_window,
            commands::alert_config,
            commands::set_alert_config,
            commands::recent_alerts,
        ])
        .setup(|app| {
            let state = AppState::new()?;
            // Primera sincronizacion antes de mostrar nada, para que el icono
            // no arranque con un cero enganoso.
            let _ = state.sync();
            app.manage(state);

            tray::build(app.handle())?;
            tray::refresh_tray(app.handle());
            watcher::spawn(app.handle().clone());

            {
                let state = app.state::<AppState>();
                let db = state.db.lock().unwrap();
                let _ = db.set_setting("onboarded", "1");
            }

            // Si el usuario abrio la app a mano, quiere ver algo. Antes se
            // guardaba un flag `onboarded` y a partir del segundo arranque la
            // app se escondia siempre: sin ventana, sin icono en el Dock, el
            // doble-click no hacia *nada* visible y parecia que no abria.
            // Ahora lo unico que arranca escondido es el autostart.
            if std::env::args().any(|a| a == AUTOSTART_FLAG) {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            } else {
                commands::reveal_main_window(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Cerrar la ventana esconde la app en vez de matarla: el medidor
            // tiene que seguir contando.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                if window.label() == "main" {
                    let _ = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
            // El popover se cierra solo al perder el foco, como cualquier
            // menu de la barra.
            WindowEvent::Focused(false) if window.label() == "tray" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar claude-burn");
}
