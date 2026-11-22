use crate::events::{AccountChanged, DeviceCodeIssued};
use crate::state::AppState;
use omnyra_core::{
    accounts::{self as account_service, AuthPrompt, DisconnectOutcome, CONNECT_TIMEOUT},
    best_time::{self, BestTime, BestTimeQuery},
    connectors::MetricPoint,
    db::repo::{
        accounts::{self, Account, AccountStatus, NewAccount},
        contents::{self, ContentRow},
        snapshots,
    },
    quota::{self, QuotaUsage},
    sync::SyncReport,
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
    run_connect(&app, &state, &platform, &[]).await
}

/// Asks for the extra permissions uploads need (incremental authorization): the user goes
/// through Google's consent screen again and must pick the same channel.
#[tauri::command]
#[specta::specta]
pub async fn authorize_upload(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    with_playlist: bool,
) -> Result<Account, AppError> {
    let account = accounts::get(&*lock_db(&state)?, &account_id)?;
    let connector = state
        .registry
        .get(&account.platform)
        .ok_or(AppError::NotSupported)?;
    let scopes = connector.upload_scopes(with_playlist);
    let connected = run_connect(&app, &state, &account.platform, &scopes).await?;
    if connected.id != account.id {
        // Another channel was picked: it is now connected too, but this one still lacks
        // the permission.
        return Err(AppError::ScopesDenied);
    }
    Ok(connected)
}

pub(crate) async fn run_connect(
    app: &AppHandle,
    state: &State<'_, AppState>,
    platform: &str,
    extra_scopes: &[String],
) -> Result<Account, AppError> {
    let connector = state.registry.get(platform).ok_or(AppError::NotSupported)?;
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
        extra_scopes,
        CONNECT_TIMEOUT,
        |prompt| {
            let url = match prompt {
                AuthPrompt::OpenUrl(url) => url,
                AuthPrompt::DeviceCode {
                    user_code,
                    verification_uri,
                } => {
                    // The UI shows the code while the user confirms it on the provider's site.
                    let _ = DeviceCodeIssued {
                        platform: platform.to_owned(),
                        user_code,
                        verification_uri: verification_uri.clone(),
                    }
                    .emit(app);
                    verification_uri
                }
            };
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
        notify(app, &account.id, account.status);
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
    super::security::ensure_unlocked(&state)?;
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

/// Syncs the account now. Progress is also reported through the `SyncStatus` event.
#[tauri::command]
#[specta::specta]
pub async fn sync_account(app: AppHandle, account_id: String) -> Result<SyncReport, AppError> {
    crate::sync::run(&app, &account_id).await
}

/// Contents of an account (newest first) with their latest stats.
#[tauri::command]
#[specta::specta]
pub fn list_contents(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<ContentRow>, AppError> {
    contents::list_with_stats(&*lock_db(&state)?, &account_id)
}

/// Best weekday and hour to publish, from the first-week views of past contents.
#[tauri::command]
#[specta::specta]
pub fn get_best_time(
    state: State<'_, AppState>,
    account_id: String,
    query: BestTimeQuery,
) -> Result<BestTime, AppError> {
    best_time::best_time(&*lock_db(&state)?, &account_id, &query, state.clock.now())
}

/// Today's API quota usage.
#[tauri::command]
#[specta::specta]
pub fn get_quota_usage(state: State<'_, AppState>) -> Result<Vec<QuotaUsage>, AppError> {
    quota::usage(&*lock_db(&state)?, state.clock.now())
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
