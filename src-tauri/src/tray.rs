//! System tray icon (step 2c): open the window, sync everything, or quit. Combined with the
//! "keep running in the background" setting, closing the window hides it here instead of
//! exiting (see the close handler in `lib.rs`).

use crate::state::AppState;
use omnyra_core::db::repo::{
    accounts::{self, AccountStatus},
    settings::{self, keys},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

/// Brings the main window to the front (shown, unminimised, focused).
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn labels(app: &AppHandle) -> (&'static str, &'static str, &'static str) {
    let state = app.state::<AppState>();
    let en = match state.db.lock() {
        Ok(conn) => settings::get(&conn, keys::LOCALE)
            .ok()
            .flatten()
            .map(|l| l == "en")
            .unwrap_or(false),
        Err(_) => false,
    };
    if en {
        ("Open Omnyra", "Sync all", "Quit")
    } else {
        ("Abrir Omnyra", "Sincronizar todo", "Salir")
    }
}

fn sync_all(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let ids: Vec<String> = {
            let state = app.state::<AppState>();
            let Ok(conn) = state.db.lock() else { return };
            accounts::list(&conn)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a.status == AccountStatus::Active)
                .map(|a| a.id)
                .collect()
        };
        for id in ids {
            let _ = crate::sync::run(&app, &id).await;
        }
    });
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let (open_l, sync_l, quit_l) = labels(app);
    let open = MenuItem::with_id(app, "open", open_l, true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", sync_l, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_l, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &sync, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Omnyra")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "sync" => sync_all(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
