use crate::state::AppState;
use omnyra_core::{
    clock::Clock,
    connectors::{AccountCtx, DateRange, MetricPoint},
    db::repo::{
        accounts::{self, Account, NewAccount},
        snapshots,
    },
    AppError,
};
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

#[tauri::command]
#[specta::specta]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, AppError> {
    let conn = lock_db(&state)?;
    accounts::list(&conn)
}

/// Dev helper: connects the fake `mock` account (no OAuth involved).
#[tauri::command]
#[specta::specta]
pub fn add_mock_account(state: State<'_, AppState>) -> Result<Account, AppError> {
    let conn = lock_db(&state)?;
    accounts::upsert(
        &conn,
        &NewAccount {
            platform: "mock",
            external_id: "mock-1",
            handle: Some("@mock_creator"),
            display_name: "Mock Creator",
        },
        &state.clock.now_iso(),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn sync_account(state: State<'_, AppState>, account_id: String) -> Result<(), AppError> {
    let account = {
        let conn = lock_db(&state)?;
        accounts::list(&conn)?
            .into_iter()
            .find(|a| a.id == account_id)
            .ok_or(AppError::NotFound)?
    };
    let connector = state
        .registry
        .get(&account.platform)
        .ok_or(AppError::NotSupported)?;
    let ctx = AccountCtx {
        account_id: account.id.clone(),
        external_id: account.external_id.clone(),
    };
    let today = state.clock.today();
    let from = {
        // last 30 days
        let now = state.clock.now() - omnyra_core::time_days(30);
        omnyra_core::format_day(now)
    };
    let points = connector
        .fetch_channel_metrics(&ctx, DateRange { from, to: today })
        .await?;
    let now_iso = state.clock.now_iso();
    let conn = lock_db(&state)?;
    for p in points {
        snapshots::upsert_account_snapshot(
            &conn,
            &account.id,
            &p.day,
            &p.metric,
            p.value,
            "current",
            &now_iso,
        )?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_account_series(
    state: State<'_, AppState>,
    account_id: String,
    metric: String,
) -> Result<Vec<MetricPoint>, AppError> {
    let conn = lock_db(&state)?;
    snapshots::account_series(&conn, &account_id, &metric)
}
