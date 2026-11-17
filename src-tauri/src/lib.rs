mod commands;
mod events;
mod state;
mod sync;
mod uploads;

use omnyra_core::{
    auth::{AuthService, ClientCredentials, KeyringStore},
    clock::{Clock, SystemClock},
    connectors::{
        http::default_http_client,
        registry::{Credentials, Registry},
    },
    db,
};
use state::AppState;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::platforms::list_platforms,
            commands::accounts::list_accounts,
            commands::accounts::add_mock_account,
            commands::accounts::connect_account,
            commands::accounts::cancel_connect,
            commands::accounts::disconnect_account,
            commands::accounts::sync_account,
            commands::accounts::get_account_series,
            commands::accounts::list_contents,
            commands::accounts::get_quota_usage,
            commands::accounts::get_best_time,
            commands::accounts::authorize_upload,
            commands::uploads::validate_upload,
            commands::uploads::enqueue_upload,
            commands::uploads::get_upload_options,
            commands::uploads::upload_scopes,
            commands::uploads::list_uploads,
            commands::uploads::cancel_upload,
            commands::uploads::retry_upload,
            commands::uploads::remove_upload,
        ])
        .events(collect_events![
            events::AccountChanged,
            events::SyncStatus,
            events::UploadProgress,
            events::UploadChanged
        ])
}

/// OAuth client IDs baked in at build time (`src-tauri/build.rs` reads `.env`).
fn credentials() -> Credentials {
    Credentials {
        google: ClientCredentials::from_parts(
            option_env!("OMNYRA_GOOGLE_CLIENT_ID"),
            option_env!("OMNYRA_GOOGLE_CLIENT_SECRET"),
        ),
    }
}

#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../src/lib/bindings.ts";

pub fn run() {
    // Logs to stderr; filter with RUST_LOG (default: warnings and above).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

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
            let clock: Arc<dyn Clock> = Arc::new(SystemClock);
            let auth = Arc::new(AuthService::new(
                Arc::new(KeyringStore::new()),
                default_http_client(),
                clock.clone(),
            ));
            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                registry: Registry::new(credentials()),
                clock,
                auth,
                connect_cancel: Mutex::new(None),
                sync_locks: Default::default(),
                upload_wake: Default::default(),
                upload_cancel: Mutex::new(None),
            });
            sync::spawn_startup_sync(app.handle().clone());
            uploads::spawn_worker(app.handle().clone());
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
