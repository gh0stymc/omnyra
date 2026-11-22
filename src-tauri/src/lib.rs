mod background;
mod commands;
mod events;
mod live;
mod notify;
mod renders;
mod state;
mod sync;
mod tray;
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
            commands::dashboard::get_summary,
            commands::dashboard::get_combined_series,
            commands::dashboard::get_top_contents,
            commands::live::get_live_overview,
            commands::planning::list_ideas,
            commands::planning::create_idea,
            commands::planning::update_idea,
            commands::planning::move_idea,
            commands::planning::delete_idea,
            commands::planning::attach_idea_file,
            commands::planning::detach_idea_file,
            commands::planning::open_idea_file,
            commands::planning::get_calendar,
            commands::planning::move_calendar_event,
            commands::planning::create_reminder,
            commands::planning::delete_reminder,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_locale,
            commands::alerts::list_alerts,
            commands::alerts::unread_alerts_count,
            commands::alerts::mark_alert_read,
            commands::alerts::mark_all_alerts_read,
            commands::publish::rendition_preset,
            commands::publish::validate_target,
            commands::publish::publish_batch,
            commands::publish::export_rendition,
            commands::community::sync_comments,
            commands::community::list_inbox,
            commands::community::inbox_counts,
            commands::community::set_comment_status,
            commands::community::reply_to_comment,
            commands::community::comment_scopes,
            commands::community::authorize_comments,
            commands::community::list_question_clusters,
            commands::community::cluster_to_idea,
            commands::community::set_comment_category,
            commands::creation::list_assets,
            commands::creation::import_asset,
            commands::creation::update_asset,
            commands::creation::delete_asset,
            commands::creation::asset_data_url,
            commands::creation::list_thumbnail_templates,
            commands::creation::save_thumbnail_template,
            commands::creation::delete_thumbnail_template,
            commands::creation::save_thumbnail_export,
            commands::creation::write_thumbnail_file,
            commands::creation::list_text_templates,
            commands::creation::save_text_template,
            commands::creation::delete_text_template,
            commands::creation::llm_status,
            commands::creation::set_llm_key,
            commands::creation::clear_llm_key,
            commands::creation::llm_generate,
            commands::moments::get_moments,
            commands::moments::cut_moment,
            commands::sponsors::list_sponsorships,
            commands::sponsors::create_sponsorship,
            commands::sponsors::update_sponsorship,
            commands::sponsors::delete_sponsorship,
            commands::sponsors::attach_sponsorship_file,
            commands::sponsors::detach_sponsorship_file,
            commands::sponsors::open_sponsorship_file,
            commands::sponsors::media_kit_html,
            commands::sponsors::save_media_kit,
        ])
        .events(collect_events![
            events::AccountChanged,
            events::SyncStatus,
            events::UploadProgress,
            events::UploadChanged,
            events::DeviceCodeIssued,
            events::AlertsChanged,
            events::RenderProgress,
            events::CommentsChanged
        ])
}

/// OAuth client IDs baked in at build time (`src-tauri/build.rs` reads `.env`).
fn credentials() -> Credentials {
    Credentials {
        google: ClientCredentials::from_parts(
            option_env!("OMNYRA_GOOGLE_CLIENT_ID"),
            option_env!("OMNYRA_GOOGLE_CLIENT_SECRET"),
        ),
        twitch: ClientCredentials::from_parts(option_env!("OMNYRA_TWITCH_CLIENT_ID"), None),
        // Meta and TikTok secrets live in the token broker, never here (AUTH_STRATEGY.md).
        instagram: ClientCredentials::from_parts(option_env!("OMNYRA_INSTAGRAM_CLIENT_ID"), None),
        tiktok: ClientCredentials::from_parts(option_env!("OMNYRA_TIKTOK_CLIENT_KEY"), None),
        broker_url: option_env!("OMNYRA_BROKER_URL")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    }
}

#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../src/lib/bindings.ts";

/// When "keep running in the background" is on, closing the window hides it to the tray
/// instead of quitting; the background scheduler keeps running.
fn install_close_handler(app: &tauri::AppHandle) {
    use omnyra_core::db::repo::settings::{self, keys};
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.clone();
    window.clone().on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            let keep = handle
                .state::<AppState>()
                .db
                .lock()
                .ok()
                .and_then(|c| settings::get_bool(&c, keys::CLOSE_TO_TRAY, false).ok())
                .unwrap_or(false);
            if keep {
                api.prevent_close();
                let _ = window.hide();
            }
        }
    });
}

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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
                render_wake: Default::default(),
                data_dir: dir.clone(),
            });
            tray::create(app.handle())?;
            install_close_handler(app.handle());

            sync::spawn_startup_sync(app.handle().clone());
            uploads::spawn_worker(app.handle().clone());
            renders::spawn_worker(app.handle().clone());
            live::spawn_sampler(app.handle().clone());
            background::spawn_scheduler(app.handle().clone());
            background::spawn_reminder_notifier(app.handle().clone());
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
