//! Background upload worker: one upload at a time, woken when something is queued and
//! when a retry is due. Uploads can be cancelled while running.

use crate::events::{UploadChanged, UploadProgress};
use crate::state::AppState;
use omnyra_core::{db::repo::accounts, uploads, uploads::UploadEvents, AppError};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::oneshot;

/// Longest sleep between queue checks (retries are usually much sooner).
const IDLE_CHECK: Duration = Duration::from_secs(60);

struct Emitter(AppHandle);

impl uploads::UploadEvents for Emitter {
    fn progress(&self, publication_id: &str, sent: u64, total: u64) {
        let _ = UploadProgress {
            publication_id: publication_id.to_owned(),
            sent: sent as f64,
            total: total as f64,
        }
        .emit(&self.0);
    }
    fn changed(&self, publication_id: &str, status: &str) {
        let _ = UploadChanged {
            publication_id: publication_id.to_owned(),
            status: status.to_owned(),
        }
        .emit(&self.0);
    }
}

async fn run_one(app: &AppHandle, id: &str) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let platform = {
        let conn = state.db.lock().map_err(|_| AppError::Internal)?;
        let row = uploads::get(&conn, id)?;
        accounts::get(&conn, &row.account_id)?.platform
    };
    let connector = state
        .registry
        .get(&platform)
        .ok_or(AppError::NotSupported)?;
    let (tx, rx) = oneshot::channel();
    *state.upload_cancel.lock().map_err(|_| AppError::Internal)? = Some((id.to_owned(), tx));
    let events = Emitter(app.clone());
    let result = tokio::select! {
        r = uploads::run(&state.db, &state.auth, &state.clock, connector.as_ref(), id, &events) => r,
        // Dropping the upload future aborts the transfer.
        _ = rx => {
            let _ = state.auth.delete_secret(&format!("upload/{id}")).await;
            events.changed(id, "cancelled");
            Ok(())
        }
    };
    if let Ok(mut slot) = state.upload_cancel.lock() {
        *slot = None;
    }
    result
}

pub fn spawn_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Ok(conn) = state.db.lock() {
            let _ = uploads::recover(&conn, &state.clock.now_iso());
        }
        loop {
            let now = state.clock.now();
            let (due, next_retry) = match state.db.lock() {
                Ok(conn) => (
                    uploads::next_due(&conn, &state.clock.now_iso())
                        .ok()
                        .flatten(),
                    uploads::next_retry_at(&conn).ok().flatten(),
                ),
                Err(_) => (None, None),
            };
            if let Some(id) = due {
                if let Err(e) = run_one(&app, &id).await {
                    tracing::warn!(error = %e, "upload attempt ended with an error");
                }
                continue;
            }
            let wait = next_retry
                .and_then(|at| {
                    time::OffsetDateTime::parse(&at, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .map(|at| (at - now).unsigned_abs().min(IDLE_CHECK))
                .unwrap_or(IDLE_CHECK);
            tokio::select! {
                _ = state.upload_wake.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    });
}
