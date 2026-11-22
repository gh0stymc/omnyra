//! Samples live streams (Twitch) every two minutes while the app runs (A2).

use crate::state::AppState;
use omnyra_core::{
    accounts as account_service,
    db::repo::accounts::{self, AccountStatus},
    live,
    quota::Unmetered,
    AppError,
};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const EVERY: Duration = Duration::from_secs(120);

/// Chat readers running, by account (5c, A3): one per live account.
static CHAT: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, tauri::async_runtime::JoinHandle<()>>>,
> = std::sync::LazyLock::new(Default::default);

/// Starts reading the account's public chat (anonymous IRC) and flushing message counts
/// into 10 s buckets of its open session; restarts on disconnect while it stays live.
fn ensure_chat(app: &AppHandle, account_id: &str, login: &str) {
    let Ok(mut map) = CHAT.lock() else { return };
    if map
        .get(account_id)
        .is_some_and(|h| !h.inner().is_finished())
    {
        return;
    }
    let (app, account, login) = (app.clone(), account_id.to_owned(), login.to_owned());
    let handle = tauri::async_runtime::spawn(async move {
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        /// Stops the flusher when the reader task is aborted.
        struct AbortOnDrop(tokio::task::JoinHandle<()>);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _flusher = AbortOnDrop({
            let (app, account, count) = (app.clone(), account.clone(), count.clone());
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(
                        omnyra_core::moments::CHAT_BUCKET_S as u64,
                    ))
                    .await;
                    let n = count.swap(0, std::sync::atomic::Ordering::SeqCst);
                    flush_chat(&app, &account, n);
                }
            })
        });
        loop {
            let c = count.clone();
            let r = omnyra_core::moments::read_chat(&login, move || {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .await;
            tracing::debug!(?r, "chat connection ended, reconnecting");
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
    map.insert(account_id.to_owned(), handle);
}

fn flush_chat(app: &AppHandle, account_id: &str, n: u32) {
    let state = app.state::<AppState>();
    // The bucket the 10 s that just elapsed belong to.
    let ts = state.clock.now() - time::Duration::seconds(omnyra_core::moments::CHAT_BUCKET_S);
    let Ok(c) = state.db.lock() else { return };
    if let Ok(Some(session)) = omnyra_core::moments::open_session(&c, account_id) {
        let _ =
            omnyra_core::moments::add_chat(&c, &session, &omnyra_core::moments::bucket_of(ts), n);
    }
}

fn stop_chat(account_id: &str) {
    if let Ok(mut map) = CHAT.lock() {
        if let Some(h) = map.remove(account_id) {
            h.abort();
        }
    }
}

async fn sample_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let accounts = match state.db.lock() {
        Ok(c) => accounts::list(&c).unwrap_or_default(),
        Err(_) => return,
    };
    for a in accounts
        .into_iter()
        .filter(|a| a.status == AccountStatus::Active)
    {
        let Some(connector) = state.registry.get(&a.platform) else {
            continue;
        };
        if !connector.info().capabilities.live {
            continue;
        }
        let ctx =
            account_service::account_ctx(&state.auth, connector.as_ref(), &a, Arc::new(Unmetered));
        match connector.live_status(&ctx).await {
            Ok(status) => {
                match (&status, a.handle.as_deref()) {
                    (Some(_), Some(login)) => ensure_chat(app, &a.id, login),
                    _ => stop_chat(&a.id),
                }
                let now = state.clock.now_iso();
                if let Ok(mut c) = state.db.lock() {
                    if let Err(e) = live::record(&mut c, &a.id, status.as_ref(), &now) {
                        tracing::warn!(error = %e, "live sample not stored");
                    }
                }
            }
            Err(AppError::NotSupported) => {}
            Err(e) => {
                tracing::debug!(error = %e, platform = %a.platform, "live status failed");
                account_service::handle_auth_error(&state.db, &a.id, &e);
            }
        }
    }
}

pub fn spawn_sampler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            sample_all(&app).await;
            tokio::time::sleep(EVERY).await;
        }
    });
}
