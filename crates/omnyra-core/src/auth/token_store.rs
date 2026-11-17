//! Token storage. Tokens live ONLY in the OS keyring (Windows Credential Manager / Secret
//! Service on Linux): never in SQLite, plain files or logs. If no keyring is available we
//! fail with [`AppError::KeyringUnavailable`] instead of falling back to plain text (D4).

use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// Keyring service name. Entries are `omnyra/<account_uuid>`.
pub const KEYRING_SERVICE: &str = "omnyra";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds, UTC) when the access token expires.
    pub expires_at: i64,
    pub scopes: Vec<String>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Synchronous on purpose: OS keyring APIs are blocking. Async callers must go through
/// `tokio::task::spawn_blocking` (see `auth::service`).
pub trait TokenStore: Send + Sync {
    fn get(&self, account_id: &str) -> Result<Option<TokenSet>>;
    fn set(&self, account_id: &str, tokens: &TokenSet) -> Result<()>;
    /// Idempotent: deleting a missing entry is not an error.
    fn delete(&self, account_id: &str) -> Result<()>;

    /// Other secrets that must never reach SQLite, such as resumable upload session URIs
    /// (H6). `key` is namespaced by the caller (`upload/<publication_id>`).
    fn get_secret(&self, key: &str) -> Result<Option<String>>;
    fn set_secret(&self, key: &str, value: &str) -> Result<()>;
    /// Idempotent.
    fn delete_secret(&self, key: &str) -> Result<()>;
}

/// The real store, backed by the OS keyring.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self::with_service(KEYRING_SERVICE)
    }

    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entry(&self, account_id: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, account_id).map_err(map_keyring_err)
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Never forwards the platform error text (it could, in theory, contain data).
fn map_keyring_err(e: keyring::Error) -> AppError {
    use keyring::Error as E;
    match e {
        E::NoDefaultStore | E::NoStorageAccess(_) | E::PlatformFailure(_) => {
            AppError::KeyringUnavailable
        }
        _ => AppError::Internal,
    }
}

impl TokenStore for KeyringStore {
    fn get(&self, account_id: &str) -> Result<Option<TokenSet>> {
        match self.entry(account_id)?.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|_| AppError::Internal),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn set(&self, account_id: &str, tokens: &TokenSet) -> Result<()> {
        let json = serde_json::to_string(tokens).map_err(|_| AppError::Internal)?;
        self.entry(account_id)?
            .set_password(&json)
            .map_err(map_keyring_err)
    }

    fn delete(&self, account_id: &str) -> Result<()> {
        match self.entry(account_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .map_err(map_keyring_err)
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        self.delete(key)
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<HashMap<String, TokenSet>>,
    secrets: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryStore {
    fn get(&self, account_id: &str) -> Result<Option<TokenSet>> {
        Ok(self.inner.lock().unwrap().get(account_id).cloned())
    }

    fn set(&self, account_id: &str, tokens: &TokenSet) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert(account_id.to_owned(), tokens.clone());
        Ok(())
    }

    fn delete(&self, account_id: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account_id);
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        Ok(self.secrets.lock().unwrap().get(key).cloned())
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        self.secrets
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        self.secrets.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn sample_tokens() -> TokenSet {
    TokenSet {
        access_token: format!("ya29.{}", "a".repeat(250)),
        refresh_token: Some(format!("1//{}", "r".repeat(100))),
        expires_at: 1_800_000_000,
        scopes: vec![
            "https://www.googleapis.com/auth/youtube.readonly".into(),
            "https://www.googleapis.com/auth/yt-analytics.readonly".into(),
            "https://www.googleapis.com/auth/youtube.upload".into(),
            "https://www.googleapis.com/auth/youtube.force-ssl".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip() {
        let s = MemoryStore::new();
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", &sample_tokens()).unwrap();
        assert_eq!(s.get("a").unwrap(), Some(sample_tokens()));
        s.delete("a").unwrap();
        s.delete("a").unwrap();
        assert_eq!(s.get("a").unwrap(), None);
    }

    #[test]
    fn debug_redacts_secrets() {
        let t = sample_tokens();
        let dbg = format!("{t:?}");
        assert!(!dbg.contains(&t.access_token));
        assert!(!dbg.contains(t.refresh_token.as_deref().unwrap()));
    }

    /// Windows Credential Manager caps a credential blob at 2560 bytes (UTF-16 → 1280 chars).
    #[test]
    fn serialized_tokens_fit_windows_credential_limit() {
        let json = serde_json::to_string(&sample_tokens()).unwrap();
        assert!(json.len() < 1280, "token JSON is {} chars", json.len());
    }

    /// Real keyring roundtrip. Needs a running Secret Service / Credential Manager, so it
    /// is opt-in: `cargo test -p omnyra-core -- --ignored keyring`.
    #[test]
    #[ignore]
    fn keyring_roundtrip() {
        let s = KeyringStore::with_service("omnyra-test");
        let id = format!("test-{}", uuid::Uuid::now_v7());
        s.set(&id, &sample_tokens()).unwrap();
        assert_eq!(s.get(&id).unwrap(), Some(sample_tokens()));
        s.delete(&id).unwrap();
        assert_eq!(s.get(&id).unwrap(), None);
    }
}
