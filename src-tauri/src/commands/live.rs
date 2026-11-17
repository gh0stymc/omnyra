use crate::state::AppState;
use omnyra_core::{
    live::{self, LiveSession, LiveStats},
    AppError,
};
use serde::Serialize;
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveOverview {
    pub stats: LiveStats,
    pub sessions: Vec<LiveSession>,
}

/// Streams in the period (VODs plus what Omnyra observed live) for one account.
#[tauri::command]
#[specta::specta]
pub fn get_live_overview(
    state: State<'_, AppState>,
    account_id: String,
    from: Option<String>,
) -> Result<LiveOverview, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::Internal)?;
    let today = state.clock.today();
    let now = state.clock.now();
    Ok(LiveOverview {
        stats: live::stats(&conn, &account_id, from.as_deref(), &today, now)?,
        sessions: live::sessions(&conn, &account_id, from.as_deref(), &today, now)?,
    })
}
