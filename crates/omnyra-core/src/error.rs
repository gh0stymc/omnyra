use serde::Serialize;
use specta::Type;

pub type Result<T> = std::result::Result<T, AppError>;

/// Single serializable error with a stable `code` the frontend maps to an i18n message.
/// Technical messages must never include tokens or secrets.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "code", content = "detail", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppError {
    #[error("token expired")]
    TokenExpired,
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("rate limited, retry after {retry_after_s}s")]
    RateLimited { retry_after_s: u32 },
    #[error("network error")]
    Network,
    #[error("not supported by this platform")]
    NotSupported,
    #[error("invalid field: {field}")]
    Validation { field: String },
    #[error("not found")]
    NotFound,
    #[error("database error")]
    Database,
    #[error("internal error")]
    Internal,
    /// The platform's OAuth client ID was not provided at build time (`.env`).
    #[error("platform not configured")]
    NotConfigured,
    /// Another connection flow is already waiting for the browser.
    #[error("authorization already in progress")]
    AuthInProgress,
    /// The user denied consent or cancelled the flow.
    #[error("authorization cancelled")]
    AuthCancelled,
    #[error("authorization timed out")]
    AuthTimeout,
    /// The authorization server rejected the request (bad client, bad code...).
    #[error("authorization failed")]
    AuthFailed,
    /// The user unticked scopes that the feature needs (Google granular consent).
    #[error("required permissions were not granted")]
    ScopesDenied,
    /// No OS keyring (Secret Service / Credential Manager) is available. Tokens are never
    /// stored anywhere else (decision D4).
    #[error("system keyring unavailable")]
    KeyringUnavailable,
    /// The Google account has no YouTube channel.
    #[error("no channel on this account")]
    NoChannel,
    /// The account is disconnected (or needs to be reconnected) and has no usable token.
    #[error("account not connected")]
    NotConnected,
    /// This account is already being synchronised.
    #[error("sync already in progress")]
    SyncInProgress,
    /// The resumable upload session expired or is unknown: start a new one.
    #[error("upload session expired")]
    UploadSessionExpired,
    /// The file changed or disappeared since it was queued.
    #[error("file not available")]
    FileUnavailable,
    /// No LLM API key saved (text assistant, 5b).
    #[error("no LLM API key")]
    LlmKeyMissing,
    /// The LLM provider rejected the API key.
    #[error("LLM API key rejected")]
    LlmKeyInvalid,
    /// The model declined the request.
    #[error("LLM declined the request")]
    LlmRefused,
    /// The app is locked with a PIN (6c).
    #[error("app locked")]
    Locked,
}

impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        AppError::Database
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(_: rusqlite_migration::Error) -> Self {
        AppError::Database
    }
}
