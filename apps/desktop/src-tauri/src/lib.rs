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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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

            // En el primer arranque se abre la ventana: si no, el usuario
            // lanza la app y no ve nada mas que un icono chico en la barra.
            let first_run = {
                let state = app.state::<AppState>();
                let db = state.db.lock().unwrap();
                let seen = db.get_setting("onboarded").ok().flatten().is_some();
                if !seen {
                    let _ = db.set_setting("onboarded", "1");
                }
                !seen
            };

            #[cfg(target_os = "macos")]
            if !first_run {
                // Sin icono en el Dock: esto vive en la barra de menu.
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            if first_run {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
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
