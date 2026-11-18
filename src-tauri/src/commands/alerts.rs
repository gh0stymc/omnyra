//! Alerts inbox (step 2c): list and mark read. Alerts are computed in the core after each
//! sync; the bell in the UI reads these.

use crate::state::AppState;
use omnyra_core::{
    db::repo::alerts::{self, Alert},
    rusqlite::Connection,
    AppError,
};
use tauri::State;

fn lock<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

#[tauri::command]
#[specta::specta]
pub fn list_alerts(state: State<'_, AppState>, limit: u32) -> Result<Vec<Alert>, AppError> {
    alerts::list(&*lock(&state)?, limit.clamp(1, 200))
}

#[tauri::command]
#[specta::specta]
pub fn unread_alerts_count(state: State<'_, AppState>) -> Result<u32, AppError> {
    alerts::unread_count(&*lock(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn mark_alert_read(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    alerts::mark_read(&*lock(&state)?, &id, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn mark_all_alerts_read(state: State<'_, AppState>) -> Result<(), AppError> {
    alerts::mark_all_read(&*lock(&state)?, &state.clock.now_iso())
}
