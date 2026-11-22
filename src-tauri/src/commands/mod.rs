pub mod accounts;
pub mod alerts;
pub mod community;
pub mod competitors;
pub mod creation;
pub mod dashboard;
pub mod live;
pub mod moments;
pub mod planning;
pub mod platforms;
pub mod publish;
pub mod security;
pub mod settings;
pub mod sponsors;
pub mod uploads;

use crate::state::AppState;
use omnyra_core::AppError;
use tauri::State;

/// The database connection (a poisoned mutex is reported as `INTERNAL`).
pub(crate) fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}
