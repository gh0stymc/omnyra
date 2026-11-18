//! Background scheduling (step 2c): periodic snapshot syncs, the alert evaluation that runs
//! after every sync, and the reminder notifier.
//!
//! This is a lightweight scheduler, not the persistent `jobs` queue from the architecture.
//! The existing loops (startup sync, upload worker, live sampler) already cover today's
//! needs; the `jobs` table is reserved for work that must survive restarts (renders and
//! scheduled publishing in 3c).

use crate::events::AlertsChanged;
use crate::state::AppState;
use crate::{notify, sync};
use omnyra_core::{
    alerts,
    db::repo::settings::{self, keys},
    planning,
};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// How often the periodic syncer wakes up. It only syncs accounts that are actually stale
/// (`sync::AUTO_SYNC_AFTER`), so waking often is cheap.
const SYNC_TICK: Duration = Duration::from_secs(30 * 60);
/// How often due reminders are checked.
const REMINDER_TICK: Duration = Duration::from_secs(60);

fn locale(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let Ok(conn) = state.db.lock() else {
        return "es".into();
    };
    settings::get(&conn, keys::LOCALE)
        .ok()
        .flatten()
        .unwrap_or_else(|| "es".into())
}

fn setting_bool(app: &AppHandle, key: &str, default: bool) -> bool {
    let state = app.state::<AppState>();
    let Ok(conn) = state.db.lock() else {
        return default;
    };
    settings::get_bool(&conn, key, default).unwrap_or(default)
}

/// Runs after a successful sync: evaluates alerts, notifies about the new ones and tells the
/// UI to refresh. Never fails the sync.
pub fn after_sync(app: &AppHandle, account_id: &str) {
    let state = app.state::<AppState>();
    let now = state.clock.now();
    let created = {
        let Ok(conn) = state.db.lock() else { return };
        match alerts::evaluate_account(&conn, now, account_id) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "alert evaluation failed");
                return;
            }
        }
    };
    if created.is_empty() {
        return;
    }
    let _ = AlertsChanged.emit(app);
    if setting_bool(app, keys::NOTIFICATIONS, true) {
        let locale = locale(app);
        for a in &created {
            notify::alert(app, &locale, a);
        }
    }
}

/// Periodically syncs stale accounts in the background (snapshots), while the setting is on.
pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SYNC_TICK).await;
            if !setting_bool(&app, keys::BACKGROUND_SYNC, true) {
                continue;
            }
            for id in sync::due_accounts(&app) {
                let _ = sync::run(&app, &id).await;
            }
        }
    });
}

/// Periodically fires notifications for reminders that have come due.
pub fn spawn_reminder_notifier(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(REMINDER_TICK).await;
            if !setting_bool(&app, keys::NOTIFICATIONS, true) {
                continue;
            }
            let state = app.state::<AppState>();
            let now = state.clock.now_iso();
            let due = {
                let Ok(conn) = state.db.lock() else { continue };
                planning::due_reminders(&conn, &now).unwrap_or_default()
            };
            if due.is_empty() {
                continue;
            }
            let locale = locale(&app);
            for (id, title) in due {
                notify::reminder(&app, &locale, &title);
                if let Ok(conn) = state.db.lock() {
                    let _ = planning::mark_reminder_notified(&conn, &id, &now);
                }
            }
        }
    });
}
