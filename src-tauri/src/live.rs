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
