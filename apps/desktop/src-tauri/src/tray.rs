//! Icono de la barra de menu: el numero siempre visible, y el popover.

use crate::commands::build_overview;
use crate::state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, WebviewWindow,
};

pub const TRAY_ID: &str = "burn-tray";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    let open = MenuItem::with_id(app, "open", "Abrir claude-burn", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Actualizar ahora", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .icon_as_template(true)
        .menu(&menu)
        // El menu solo con click derecho: el izquierdo abre el popover.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "refresh" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = state.sync();
                    refresh_tray(app);
                    let _ = app.emit("burn://refreshed", ());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), position.x, position.y);
            }
        })
        .build(app)
}

/// Refresca el texto del icono con el gasto de hoy y el limite mas apretado.
pub fn refresh_tray<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(overview) = build_overview(&state, &Default::default()) else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(overview.tray.title()));
    }
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Abre o cierra el popover, anclado debajo del icono.
fn toggle_popover<R: Runtime>(app: &AppHandle<R>, icon_x: f64, icon_y: f64) {
    let Some(win) = app.get_webview_window("tray") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    position_under_icon(&win, icon_x, icon_y);
    let _ = app.emit("burn://popover-open", ());
    let _ = win.show();
    let _ = win.set_focus();
}

fn position_under_icon<R: Runtime>(win: &WebviewWindow<R>, icon_x: f64, icon_y: f64) {
    let Ok(size) = win.outer_size() else { return };
    let scale = win.scale_factor().unwrap_or(1.0);
    let w = size.width as f64;

    // Centrado bajo el icono, sin salirse por el borde derecho de la pantalla.
    let mut x = icon_x - w / 2.0;
    if let Ok(Some(monitor)) = win.current_monitor() {
        let screen_right = (monitor.position().x as f64) + (monitor.size().width as f64);
        x = x
            .min(screen_right - w - 8.0 * scale)
            .max(monitor.position().x as f64 + 8.0 * scale);
    }
    let y = icon_y + 6.0 * scale;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}
