use crate::state::AppState;
use omnyra_core::{
    accounts as account_service,
    connectors::{UploadDraft, UploadOptions},
    db::repo::accounts,
    quota::{DbQuota, Purpose},
    uploads::{self, UploadRow, ValidationReport},
    AppError,
};
use std::sync::Arc;
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

fn connector_for(
    state: &State<'_, AppState>,
    account_id: &str,
) -> Result<
    (
        accounts::Account,
        Arc<dyn omnyra_core::connectors::Connector>,
    ),
    AppError,
> {
    let account = accounts::get(&*lock_db(state)?, account_id)?;
    let c = state
        .registry
        .get(&account.platform)
        .ok_or(AppError::NotSupported)?;
    Ok((account, c))
}

/// Checks the file (ffprobe) and the form against the platform's rules.
#[tauri::command]
#[specta::specta]
pub async fn validate_upload(
    state: State<'_, AppState>,
    account_id: String,
    draft: UploadDraft,
) -> Result<ValidationReport, AppError> {
    let (_, c) = connector_for(&state, &account_id)?;
    Ok(uploads::validate(c.as_ref(), &draft).await)
}

/// Queues the upload; the background worker sends it. Returns the publication ID.
#[tauri::command]
#[specta::specta]
pub async fn enqueue_upload(
    state: State<'_, AppState>,
    account_id: String,
    draft: UploadDraft,
) -> Result<String, AppError> {
    let (_, c) = connector_for(&state, &account_id)?;
    let id = uploads::enqueue(
        &state.db,
        state.clock.as_ref(),
        c.as_ref(),
        &account_id,
        &draft,
    )
    .await?;
    state.upload_wake.notify_one();
    Ok(id)
}

/// Categories and playlists for the form (2 quota units).
#[tauri::command]
#[specta::specta]
pub async fn get_upload_options(
    state: State<'_, AppState>,
    account_id: String,
    lang: String,
) -> Result<UploadOptions, AppError> {
    let (account, c) = connector_for(&state, &account_id)?;
    let quota = Arc::new(DbQuota::new(
        state.db.clone(),
        state.clock.clone(),
        &account.platform,
        Purpose::Upload,
    ));
    let ctx = account_service::account_ctx(&state.auth, c.as_ref(), &account, quota);
    c.upload_options(&ctx, &lang).await
}

/// OAuth scopes the upload needs for this account (to ask for them before queuing).
#[tauri::command]
#[specta::specta]
pub fn upload_scopes(
    state: State<'_, AppState>,
    account_id: String,
    with_playlist: bool,
) -> Result<Vec<String>, AppError> {
    let (_, c) = connector_for(&state, &account_id)?;
    Ok(c.upload_scopes(with_playlist))
}

#[tauri::command]
#[specta::specta]
pub fn list_uploads(state: State<'_, AppState>) -> Result<Vec<UploadRow>, AppError> {
    uploads::list(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_upload(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    uploads::cancel(&*lock_db(&state)?, &id, &state.clock.now_iso())?;
    // If it is the one being sent right now, stop the transfer.
    if let Some((running, tx)) = state.upload_cancel.lock().ok().and_then(|mut s| s.take()) {
        if running == id {
            let _ = tx.send(());
        } else if let Ok(mut s) = state.upload_cancel.lock() {
            *s = Some((running, tx));
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn retry_upload(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    uploads::retry(&*lock_db(&state)?, &id, &state.clock.now_iso())?;
    state.upload_wake.notify_one();
    Ok(())
}

/// Removes a finished, failed or cancelled upload from the list (the file is not touched).
#[tauri::command]
#[specta::specta]
pub fn remove_upload(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    uploads::remove(&*lock_db(&state)?, &id)?;
    Ok(())
}
