use crate::events::CommentsChanged;
use crate::state::AppState;
use omnyra_core::{
    community::{self, CommentFilter, CommentRow, CommentSyncReport, InboxCounts},
    db::repo::accounts::{self, Account},
    AppError,
};
use tauri::{AppHandle, State};
use tauri_specta::Event;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

fn connector_of(
    state: &State<'_, AppState>,
    account_id: &str,
) -> Result<std::sync::Arc<dyn omnyra_core::connectors::Connector>, AppError> {
    let platform = accounts::get(&*lock_db(state)?, account_id)?.platform;
    state.registry.get(&platform).ok_or(AppError::NotSupported)
}

#[tauri::command]
#[specta::specta]
pub async fn sync_comments(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<CommentSyncReport, AppError> {
    let c = connector_of(&state, &account_id)?;
    let r = community::sync(
        &state.db,
        &state.auth,
        &state.clock,
        c.as_ref(),
        &account_id,
    )
    .await?;
    let _ = CommentsChanged { account_id }.emit(&app);
    Ok(r)
}

#[tauri::command]
#[specta::specta]
pub fn list_inbox(
    state: State<'_, AppState>,
    filter: CommentFilter,
) -> Result<Vec<CommentRow>, AppError> {
    community::list(&*lock_db(&state)?, &filter)
}

#[tauri::command]
#[specta::specta]
pub fn inbox_counts(state: State<'_, AppState>) -> Result<InboxCounts, AppError> {
    community::counts(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_comment_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<(), AppError> {
    community::set_status(&*lock_db(&state)?, &id, &status)
}

#[tauri::command]
#[specta::specta]
pub async fn reply_to_comment(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> Result<(), AppError> {
    let account_id: String = lock_db(&state)?
        .query_row(
            "SELECT account_id FROM comments WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::NotFound)?;
    let c = connector_of(&state, &account_id)?;
    community::reply(&state.db, &state.auth, &state.clock, c.as_ref(), &id, &text).await?;
    let _ = CommentsChanged { account_id }.emit(&app);
    Ok(())
}

/// Scopes needed to answer comments on `account_id`'s network.
#[tauri::command]
#[specta::specta]
pub fn comment_scopes(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<String>, AppError> {
    Ok(connector_of(&state, &account_id)?.comment_scopes())
}

/// Incremental authorization for comments (YouTube `force-ssl`, Instagram comments).
#[tauri::command]
#[specta::specta]
pub async fn authorize_comments(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Account, AppError> {
    let c = connector_of(&state, &account_id)?;
    let platform = c.info().id.clone();
    let connected =
        super::accounts::run_connect(&app, &state, &platform, &c.comment_scopes()).await?;
    if connected.id != account_id {
        return Err(AppError::ScopesDenied);
    }
    Ok(connected)
}
