mod commands;
mod state;

use omnyra_core::{clock::SystemClock, connectors::registry::Registry, db};
use state::AppState;
use std::sync::Mutex;
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::platforms::list_platforms,
        commands::accounts::list_accounts,
        commands::accounts::add_mock_account,
        commands::accounts::sync_account,
        commands::accounts::get_account_series,
    ])
}

const BINDINGS_PATH: &str = "../src/lib/bindings.ts";

pub fn run() {
    let builder = specta_builder();

    // Regenerate TS bindings on every dev run (src/lib/bindings.ts is committed).
    #[cfg(debug_assertions)]
    builder
        .export(specta_typescript::Typescript::default(), BINDINGS_PATH)
        .expect("failed to export TypeScript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("omnyra.db")).map_err(|e| e.to_string())?;
            app.manage(AppState {
                db: Mutex::new(conn),
                registry: Registry::new(),
                clock: SystemClock,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Omnyra");
}

#[cfg(test)]
mod tests {
    /// `cargo test` regenerates `src/lib/bindings.ts` without launching the GUI.
    #[test]
    fn export_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                super::BINDINGS_PATH,
            )
            .expect("failed to export TypeScript bindings");
    }
}
