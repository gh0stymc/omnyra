use crate::events::AccountChanged;
use crate::state::AppState;
use omnyra_core::{
    accounts::{self as account_service, DisconnectOutcome, CONNECT_TIMEOUT},
    connectors::{DateRange, MetricPoint},
    db::repo::{
        accounts::{self, Account, AccountStatus, NewAccount},
        snapshots,
    },
    AppError,
};
use tauri::{AppHandle, State};
use tauri_specta::Event;
use tokio::sync::oneshot;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

fn notify(app: &AppHandle, account_id: &str, status: AccountStatus) {
    let _ = AccountChanged {
        account_id: account_id.to_owned(),
        status,
    }
    .emit(app);
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
    let id = accounts::id_for(&conn, "mock", "mock-1")?;
    accounts::upsert(
        &conn,
        &id,
        &NewAccount {
            platform: "mock",
            external_id: "mock-1",
            handle: Some("@mock_creator"),
            display_name: "Mock Creator",
            granted_scopes: &[],
        },
        &state.clock.now_iso(),
    )
}

/// Opens the system browser on the platform's consent screen and waits (up to 5 min) for
/// the loopback redirect. Also used to reconnect an account that `needs_reauth`.
#[tauri::command]
#[specta::specta]
pub async fn connect_account(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: String,
) -> Result<Account, AppError> {
    let connector = state
        .registry
        .get(&platform)
        .ok_or(AppError::NotSupported)?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    {
        let mut slot = state
            .connect_cancel
            .lock()
            .map_err(|_| AppError::Internal)?;
        if slot.is_some() {
            return Err(AppError::AuthInProgress);
        }
        *slot = Some(cancel_tx);
    }

    let flow = account_service::connect(
        &state.db,
        &state.auth,
        state.clock.as_ref(),
        connector.as_ref(),
        &[],
        CONNECT_TIMEOUT,
        |url| {
            // Only ever hand provider HTTPS URLs to the OS.
            if !url.starts_with("https://") {
                return Err(AppError::Internal);
            }
            tauri_plugin_opener::open_url(url, None::<&str>).map_err(|_| AppError::Internal)
        },
    );
    let result = tokio::select! {
        r = flow => r,
        _ = cancel_rx => Err(AppError::AuthCancelled),
    };
    if let Ok(mut slot) = state.connect_cancel.lock() {
        *slot = None;
    }
    if let Ok(account) = &result {
        notify(&app, &account.id, account.status);
    }
    result
}

/// Aborts the flow started by `connect_account` (it then fails with `AUTH_CANCELLED`).
#[tauri::command]
#[specta::specta]
pub fn cancel_connect(state: State<'_, AppState>) {
    if let Some(tx) = state.connect_cancel.lock().ok().and_then(|mut s| s.take()) {
        let _ = tx.send(());
    }
}

/// Revokes the grant, deletes the tokens from the keyring and keeps the historical data.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<DisconnectOutcome, AppError> {
    let platform = accounts::get(&*lock_db(&state)?, &account_id)?.platform;
    let connector = state.registry.get(&platform);
    let out = account_service::disconnect(
        &state.db,
        &state.auth,
        state.clock.as_ref(),
        connector.as_deref(),
        &account_id,
    )
    .await?;
    notify(&app, &account_id, AccountStatus::Disconnected);
    Ok(out)
}

#[tauri::command]
#[specta::specta]
pub async fn sync_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), AppError> {
    let account = accounts::get(&*lock_db(&state)?, &account_id)?;
    if account.status != AccountStatus::Active {
        return Err(AppError::NotConnected);
    }
    let connector = state
        .registry
        .get(&account.platform)
        .ok_or(AppError::NotSupported)?;
    let ctx = account_service::account_ctx(&state.auth, connector.as_ref(), &account);
    let today = state.clock.today();
    let from = {
        // last 30 days
        let now = state.clock.now() - omnyra_core::time_days(30);
        omnyra_core::format_day(now)
    };
    let points = match connector
        .fetch_channel_metrics(&ctx, DateRange { from, to: today })
        .await
    {
        Ok(p) => p,
        Err(e) => {
            if account_service::handle_auth_error(&state.db, &account.id, &e) {
                notify(&app, &account.id, AccountStatus::NeedsReauth);
            }
            return Err(e);
        }
    };
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
