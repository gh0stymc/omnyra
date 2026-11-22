use crate::state::AppState;
use omnyra_core::{
    db::repo::{audit_log, settings},
    security::{
        checklist::{self, AccountChecklist},
        impersonation::{self, Suspect},
        lock,
    },
    AppError,
};
use serde::Serialize;
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

/// Refuses sensitive actions while the app is locked.
pub(crate) fn ensure_unlocked(state: &AppState) -> Result<(), AppError> {
    if state.locked.load(Ordering::SeqCst) {
        Err(AppError::Locked)
    } else {
        Ok(())
    }
}

fn audit(state: &State<'_, AppState>, action: &str, detail: serde_json::Value) {
    if let Ok(c) = state.db.lock() {
        let _ = audit_log::append(&c, &state.clock.now_iso(), action, None, &detail);
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LockStatus {
    pub enabled: bool,
    pub locked: bool,
    /// Seconds to wait before another try (after repeated wrong PINs).
    pub retry_after_s: u32,
    pub idle_minutes: u32,
}

fn retry_after(state: &AppState) -> u32 {
    let Ok(f) = state.lock_failures.lock() else {
        return 0;
    };
    let (failures, last) = *f;
    let wait = lock::backoff_s(failures);
    match last {
        Some(t) if wait > 0 => wait.saturating_sub(t.elapsed().as_secs()) as u32,
        _ => 0,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn lock_status(state: State<'_, AppState>) -> Result<LockStatus, AppError> {
    let enabled = state.auth.get_secret(lock::SECRET_KEY).await?.is_some();
    let idle = settings::get(&*lock_db(&state)?, settings::keys::LOCK_IDLE_MINUTES)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    Ok(LockStatus {
        enabled,
        locked: enabled && state.locked.load(Ordering::SeqCst),
        retry_after_s: retry_after(&state),
        idle_minutes: idle,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn unlock(state: State<'_, AppState>, pin: String) -> Result<bool, AppError> {
    if retry_after(&state) > 0 {
        return Err(AppError::RateLimited {
            retry_after_s: retry_after(&state),
        });
    }
    let Some(stored) = state.auth.get_secret(lock::SECRET_KEY).await? else {
        state.locked.store(false, Ordering::SeqCst);
        return Ok(true);
    };
    let ok = tokio::task::spawn_blocking(move || lock::verify_pin(&pin, &stored))
        .await
        .map_err(|_| AppError::Internal)?;
    let mut f = state.lock_failures.lock().map_err(|_| AppError::Internal)?;
    if ok {
        *f = (0, None);
        state.locked.store(false, Ordering::SeqCst);
    } else {
        *f = (f.0 + 1, Some(std::time::Instant::now()));
        drop(f);
        audit(&state, "lock.failed", json!({}));
    }
    Ok(ok)
}

#[tauri::command]
#[specta::specta]
pub async fn lock_now(state: State<'_, AppState>) -> Result<(), AppError> {
    if state.auth.get_secret(lock::SECRET_KEY).await?.is_some() {
        state.locked.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Sets or changes the PIN (`current` is required when one is already set).
#[tauri::command]
#[specta::specta]
pub async fn set_pin(
    state: State<'_, AppState>,
    current: Option<String>,
    new_pin: String,
) -> Result<(), AppError> {
    ensure_unlocked(&state)?;
    if let Some(stored) = state.auth.get_secret(lock::SECRET_KEY).await? {
        let cur = current.unwrap_or_default();
        let ok = tokio::task::spawn_blocking(move || lock::verify_pin(&cur, &stored))
            .await
            .map_err(|_| AppError::Internal)?;
        if !ok {
            return Err(AppError::Validation {
                field: "currentPin".into(),
            });
        }
    }
    let hash = tokio::task::spawn_blocking(move || lock::hash_pin(&new_pin, lock::ITERATIONS))
        .await
        .map_err(|_| AppError::Internal)??;
    state.auth.set_secret(lock::SECRET_KEY, &hash).await?;
    audit(&state, "lock.change", json!({ "enabled": true }));
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn remove_pin(state: State<'_, AppState>, pin: String) -> Result<(), AppError> {
    ensure_unlocked(&state)?;
    let Some(stored) = state.auth.get_secret(lock::SECRET_KEY).await? else {
        return Ok(());
    };
    let ok = tokio::task::spawn_blocking(move || lock::verify_pin(&pin, &stored))
        .await
        .map_err(|_| AppError::Internal)?;
    if !ok {
        return Err(AppError::Validation {
            field: "currentPin".into(),
        });
    }
    state.auth.delete_secret(lock::SECRET_KEY).await?;
    audit(&state, "lock.change", json!({ "enabled": false }));
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_lock_idle(state: State<'_, AppState>, minutes: u32) -> Result<(), AppError> {
    settings::set(
        &*lock_db(&state)?,
        settings::keys::LOCK_IDLE_MINUTES,
        &minutes.min(240).to_string(),
    )
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    pub entries: Vec<audit_log::AuditEntry>,
    /// False if a row was edited or deleted outside the app.
    pub chain_ok: bool,
}

#[tauri::command]
#[specta::specta]
pub fn audit_log_view(state: State<'_, AppState>, limit: u32) -> Result<AuditView, AppError> {
    let c = lock_db(&state)?;
    Ok(AuditView {
        entries: audit_log::recent(&c, limit.min(500))?,
        chain_ok: audit_log::verify_chain(&c)?,
    })
}

#[tauri::command]
#[specta::specta]
pub fn security_checklist(state: State<'_, AppState>) -> Result<Vec<AccountChecklist>, AppError> {
    checklist::list(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_checklist_item(
    state: State<'_, AppState>,
    account_id: String,
    item: String,
    value: String,
) -> Result<(), AppError> {
    checklist::set(
        &*lock_db(&state)?,
        &account_id,
        &item,
        &value,
        &state.clock.now_iso(),
    )
}

#[tauri::command]
#[specta::specta]
pub fn list_suspects(state: State<'_, AppState>) -> Result<Vec<Suspect>, AppError> {
    impersonation::list(&*lock_db(&state)?)
}

/// Checks comment authors of every account (free) and returns how many new suspects.
#[tauri::command]
#[specta::specta]
pub fn scan_impersonation(state: State<'_, AppState>) -> Result<u32, AppError> {
    let c = lock_db(&state)?;
    let now = state.clock.now_iso();
    let ids: Vec<String> = omnyra_core::db::repo::accounts::list(&c)?
        .into_iter()
        .filter(|a| a.platform != "mock")
        .map(|a| a.id)
        .collect();
    let mut n = 0;
    for id in ids {
        n += impersonation::scan_comments(&c, &id, &now)?;
    }
    Ok(n)
}

/// YouTube channel search for lookalikes of the first YouTube account (100+ units).
#[tauri::command]
#[specta::specta]
pub async fn search_impersonators(state: State<'_, AppState>) -> Result<u32, AppError> {
    let (c, ctx) = super::competitors::youtube_ctx(&state)?;
    impersonation::scan_search(
        &state.db,
        c.as_ref(),
        &ctx,
        &ctx.account_id,
        &state.clock.now_iso(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub fn set_suspect_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<(), AppError> {
    impersonation::set_status(&*lock_db(&state)?, &id, &status)
}
