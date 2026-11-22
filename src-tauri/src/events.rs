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

/// Bytes sent of the upload in progress (several times per second at most per chunk).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UploadProgress {
    pub publication_id: String,
    pub sent: f64,
    pub total: f64,
}

/// An upload changed status (`queued`, `uploading`, `published`, `failed`, `cancelled`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UploadChanged {
    pub publication_id: String,
    pub status: String,
}

/// New alerts were created (after a sync) or an alert was read. The frontend refreshes the
/// bell and the alerts list.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct AlertsChanged;

/// Device flow (Twitch): show this code; the user confirms it at `verification_uri`
/// (already opened in the browser).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeIssued {
    pub platform: String,
    pub user_code: String,
    pub verification_uri: String,
}

/// FFmpeg progress (0..1) of a publication's rendition, or of a "prepare and download"
/// export (`key` chosen by the frontend).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct RenderProgress {
    pub key: String,
    pub progress: f64,
}
