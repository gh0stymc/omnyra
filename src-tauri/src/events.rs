use omnyra_core::db::repo::accounts::AccountStatus;
use serde::{Deserialize, Serialize};

/// An account was connected, disconnected or needs to be reconnected.
/// The frontend invalidates its account queries when it receives it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AccountChanged {
    pub account_id: String,
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    Running,
    Done,
    Error,
}

/// Progress of an account sync (manual or automatic at startup).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub account_id: String,
    pub state: SyncState,
    pub last_sync_at: Option<String>,
    /// `AppError` code when `state` is `error`.
    pub error_code: Option<String>,
}
