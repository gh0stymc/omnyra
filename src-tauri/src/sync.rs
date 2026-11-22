//! Runs account syncs (button and startup) and reports them with the `SyncStatus` event.

use crate::events::{AccountChanged, SyncState, SyncStatus};
use crate::state::AppState;
use omnyra_core::{
    accounts as account_service,
    db::repo::accounts::{self, AccountStatus},
    sync::{self, SyncReport},
    AppError,
};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Accounts synced longer ago than this are synced automatically at startup.
const AUTO_SYNC_AFTER: time::Duration = time::Duration::hours(6);

fn emit(app: &AppHandle, account_id: &str, state: SyncState, error: Option<&AppError>) {
    let last_sync_at = app.state::<AppState>().db.lock().ok().and_then(|c| {
        accounts::get(&c, account_id)
            .ok()
            .and_then(|a| a.last_sync_at)
    });
    let error_code = error.and_then(|e| {
        serde_json::to_value(e)
            .ok()
            .and_then(|v| v["code"].as_str().map(str::to_owned))
    });
    let _ = SyncStatus {
        account_id: account_id.to_owned(),
        state,
        last_sync_at,
        error_code,
    }
    .emit(app);
}

pub async fn run(app: &AppHandle, account_id: &str) -> Result<SyncReport, AppError> {
    let state = app.state::<AppState>();
    let _guard = state.sync_locks.try_begin(account_id)?;
    let platform = accounts::get(
        &*state.db.lock().map_err(|_| AppError::Internal)?,
        account_id,
    )?
    .platform;
    let connector = state
        .registry
        .get(&platform)
        .ok_or(AppError::NotSupported)?;

    emit(app, account_id, SyncState::Running, None);
    let result = sync::sync_account(
        &state.db,
        &state.auth,
        &state.clock,
        connector.as_ref(),
        account_id,
    )
    .await;
    match &result {
        Ok(_) => {
            emit(app, account_id, SyncState::Done, None);
            // Compute performance/milestone alerts and notify (manual and background syncs).
            crate::background::after_sync(app, account_id);
            // Comments too, best effort (missing scope, quota...: the inbox can retry).
            if connector.info().capabilities.comments
                != omnyra_core::connectors::CommentSupport::None
            {
                match omnyra_core::community::sync(
                    &state.db,
                    &state.auth,
                    &state.clock,
                    connector.as_ref(),
                    account_id,
                )
                .await
                {
                    Ok(r) if r.new > 0 => {
                        let _ = crate::events::CommentsChanged {
                            account_id: account_id.to_owned(),
                        }
                        .emit(app);
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!(error = %e, "comment sync skipped"),
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "sync failed");
            if account_service::handle_auth_error(&state.db, account_id, e) {
                let _ = AccountChanged {
                    account_id: account_id.to_owned(),
                    status: AccountStatus::NeedsReauth,
                }
                .emit(app);
            }
            emit(app, account_id, SyncState::Error, Some(e));
        }
    }
    result
}

fn is_stale(last_sync_at: Option<&str>, now: OffsetDateTime) -> bool {
    match last_sync_at.and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
        Some(t) => now - t > AUTO_SYNC_AFTER,
        None => true,
    }
}

/// Active accounts whose last sync is older than [`AUTO_SYNC_AFTER`].
pub fn due_accounts(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let now = state.clock.now();
    let Ok(conn) = state.db.lock() else {
        return Vec::new();
    };
    accounts::list(&conn)
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.status == AccountStatus::Active)
        .filter(|a| is_stale(a.last_sync_at.as_deref(), now))
        .map(|a| a.id)
        .collect()
}

/// Syncs, one after another, the active accounts that have not been synced recently.
pub fn spawn_startup_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        for id in due_accounts(&app) {
            // Errors are already reported through the event.
            let _ = run(&app, &id).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn staleness() {
        let now = datetime!(2026-11-17 12:00 UTC);
        assert!(is_stale(None, now));
        assert!(is_stale(Some("2026-11-17T05:59:00Z"), now));
        assert!(!is_stale(Some("2026-11-17T06:01:00Z"), now));
        assert!(is_stale(Some("garbage"), now));
    }
}
