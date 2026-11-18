//! App settings (step 2c): tray/background/notification preferences and UI locale.

use crate::state::AppState;
use omnyra_core::{
    db::repo::settings::{self, keys},
    rusqlite::Connection,
    AppError,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Keep running in the tray when the window is closed.
    pub close_to_tray: bool,
    /// Launch at login.
    pub autostart: bool,
    /// Run periodic background syncs.
    pub background_sync: bool,
    /// Raise system notifications.
    pub notifications: bool,
    /// UI language (`"es"` | `"en"`).
    pub locale: String,
}

pub fn load(conn: &Connection) -> Result<AppSettings, AppError> {
    Ok(AppSettings {
        close_to_tray: settings::get_bool(conn, keys::CLOSE_TO_TRAY, false)?,
        autostart: settings::get_bool(conn, keys::AUTOSTART, false)?,
        background_sync: settings::get_bool(conn, keys::BACKGROUND_SYNC, true)?,
        notifications: settings::get_bool(conn, keys::NOTIFICATIONS, true)?,
        locale: settings::get(conn, keys::LOCALE)?.unwrap_or_else(|| "es".into()),
    })
}

fn lock<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, AppError> {
    load(&*lock(&state)?)
}

/// Persists all settings and reconciles the OS autostart entry with the plugin.
#[tauri::command]
#[specta::specta]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    {
        let conn = lock(&state)?;
        settings::set_bool(&conn, keys::CLOSE_TO_TRAY, settings.close_to_tray)?;
        settings::set_bool(&conn, keys::BACKGROUND_SYNC, settings.background_sync)?;
        settings::set_bool(&conn, keys::NOTIFICATIONS, settings.notifications)?;
        let locale = if settings.locale == "en" { "en" } else { "es" };
        settings::set(&conn, keys::LOCALE, locale)?;
    }

    // Autostart lives in the OS (registry / autostart dir); the plugin is the source of
    // truth. Store the resulting state so the UI reflects what actually happened.
    let autostart = reconcile_autostart(&app, settings.autostart);
    settings::set_bool(&*lock(&state)?, keys::AUTOSTART, autostart)?;

    load(&*lock(&state)?)
}

/// Mirrors the UI language so Rust can localise system notifications. Called by the
/// frontend whenever the language changes (and once on startup).
#[tauri::command]
#[specta::specta]
pub fn set_locale(state: State<'_, AppState>, locale: String) -> Result<(), AppError> {
    let locale = if locale == "en" { "en" } else { "es" };
    settings::set(&*lock(&state)?, keys::LOCALE, locale)
}

/// Best-effort: enable/disable launch-at-login. Returns the state actually achieved.
fn reconcile_autostart(app: &AppHandle, want: bool) -> bool {
    let mgr = app.autolaunch();
    let result = if want { mgr.enable() } else { mgr.disable() };
    if let Err(e) = result {
        tracing::warn!(error = %e, want, "could not change autostart");
    }
    mgr.is_enabled().unwrap_or(want)
}
