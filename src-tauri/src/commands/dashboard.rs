use crate::state::AppState;
use omnyra_core::{
    connectors::MetricPoint,
    dashboard::{self, Summary, TopContent},
    AppError,
};
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

/// Per-account and global figures for the period starting at `from` (`None` = all time).
#[tauri::command]
#[specta::specta]
pub fn get_summary(state: State<'_, AppState>, from: Option<String>) -> Result<Summary, AppError> {
    dashboard::summary(&*lock_db(&state)?, from.as_deref(), &state.clock.today())
}

/// `metric` summed across accounts, day by day.
#[tauri::command]
#[specta::specta]
pub fn get_combined_series(
    state: State<'_, AppState>,
    metric: String,
    from: Option<String>,
) -> Result<Vec<MetricPoint>, AppError> {
    dashboard::combined_series(
        &*lock_db(&state)?,
        &metric,
        from.as_deref(),
        &state.clock.today(),
    )
}

/// Most viewed contents published in the period, across accounts.
#[tauri::command]
#[specta::specta]
pub fn get_top_contents(
    state: State<'_, AppState>,
    from: Option<String>,
    limit: u32,
) -> Result<Vec<TopContent>, AppError> {
    dashboard::top_contents(
        &*lock_db(&state)?,
        from.as_deref(),
        &state.clock.today(),
        limit as usize,
    )
}
