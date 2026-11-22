use crate::state::AppState;
use omnyra_core::{
    accounts::account_ctx,
    competitors::{self, DayValue, TrackedSummary, TrendKeyword},
    connectors::{AccountCtx, Connector, PublicVideo},
    db::repo::accounts::{self, AccountStatus},
    quota::{DbQuota, Purpose},
    AppError,
};
use std::sync::Arc;
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

/// The first active YouTube account: public lookups go through its token and quota.
pub(crate) fn youtube_ctx(state: &AppState) -> Result<(Arc<dyn Connector>, AccountCtx), AppError> {
    let account = {
        let c = state.db.lock().map_err(|_| AppError::Internal)?;
        accounts::list(&c)?
            .into_iter()
            .find(|a| a.platform == "youtube" && a.status == AccountStatus::Active)
            .ok_or(AppError::NotConnected)?
    };
    let connector = state
        .registry
        .get("youtube")
        .ok_or(AppError::NotSupported)?;
    let quota = Arc::new(DbQuota::new(
        state.db.clone(),
        state.clock.clone(),
        "youtube",
        Purpose::Sync,
    ));
    let ctx = account_ctx(&state.auth, connector.as_ref(), &account, quota);
    Ok((connector, ctx))
}

#[tauri::command]
#[specta::specta]
pub fn list_tracked(state: State<'_, AppState>) -> Result<Vec<TrackedSummary>, AppError> {
    competitors::list(&*lock_db(&state)?, &state.clock.now_iso())
}

/// Adds a YouTube channel by handle, ID or URL.
#[tauri::command]
#[specta::specta]
pub async fn add_tracked(state: State<'_, AppState>, query: String) -> Result<String, AppError> {
    let (c, ctx) = youtube_ctx(&state)?;
    competitors::add(&state.db, c.as_ref(), &ctx, &query, &state.clock.now_iso()).await
}

#[tauri::command]
#[specta::specta]
pub fn remove_tracked(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    competitors::remove(&*lock_db(&state)?, &id)
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_tracked(state: State<'_, AppState>) -> Result<u32, AppError> {
    let (c, ctx) = youtube_ctx(&state)?;
    competitors::refresh_all(&state.db, c.as_ref(), &ctx, &state.clock.now_iso(), true).await
}

#[tauri::command]
#[specta::specta]
pub fn tracked_followers(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<DayValue>, AppError> {
    competitors::followers_series(&*lock_db(&state)?, &id)
}

#[tauri::command]
#[specta::specta]
pub fn tracked_top(state: State<'_, AppState>, id: String) -> Result<Vec<PublicVideo>, AppError> {
    competitors::top_videos(&*lock_db(&state)?, &id, 10)
}

#[tauri::command]
#[specta::specta]
pub fn list_trends(state: State<'_, AppState>) -> Result<Vec<TrendKeyword>, AppError> {
    competitors::list_trends(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn add_trend_keyword(state: State<'_, AppState>, keyword: String) -> Result<String, AppError> {
    competitors::add_keyword(&*lock_db(&state)?, &keyword, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn remove_trend_keyword(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    competitors::remove_keyword(&*lock_db(&state)?, &id)
}

/// Searches the keywords not searched today (about 101 quota units each).
#[tauri::command]
#[specta::specta]
pub async fn refresh_trends(state: State<'_, AppState>) -> Result<u32, AppError> {
    let (c, ctx) = youtube_ctx(&state)?;
    competitors::refresh_trends(&state.db, c.as_ref(), &ctx, &state.clock.now_iso()).await
}
