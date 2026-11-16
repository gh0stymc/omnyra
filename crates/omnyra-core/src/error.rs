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
