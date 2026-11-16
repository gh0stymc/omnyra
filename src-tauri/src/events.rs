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
