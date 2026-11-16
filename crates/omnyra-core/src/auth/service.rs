//! Token lifecycle on top of a [`TokenStore`]: load/save off the async runtime, and
//! automatic refresh shortly before expiry, serialized per account.

use super::oauth::{self, OAuthConfig};
use super::token_store::{TokenSet, TokenStore};
use crate::clock::Clock;
use crate::{AppError, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Refresh this many seconds before the access token actually expires.
const EXPIRY_MARGIN_S: i64 = 60;

pub struct AuthService {
    store: Arc<dyn TokenStore>,
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    refresh_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl AuthService {
    pub fn new(store: Arc<dyn TokenStore>, http: reqwest::Client, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            http,
            clock,
            refresh_locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn now(&self) -> i64 {
        self.clock.now().unix_timestamp()
    }

    pub async fn load(&self, account_id: &str) -> Result<Option<TokenSet>> {
        let (store, id) = (self.store.clone(), account_id.to_owned());
        blocking(move || store.get(&id)).await
    }

    pub async fn save(&self, account_id: &str, tokens: &TokenSet) -> Result<()> {
        let (store, id, t) = (self.store.clone(), account_id.to_owned(), tokens.clone());
        blocking(move || store.set(&id, &t)).await
    }

    pub async fn delete(&self, account_id: &str) -> Result<()> {
        let (store, id) = (self.store.clone(), account_id.to_owned());
        blocking(move || store.delete(&id)).await
    }

    fn lock_for(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.refresh_locks
            .lock()
            .unwrap()
            .entry(account_id.to_owned())
            .or_default()
            .clone()
    }

    /// A valid access token, refreshing it first if it is about to expire.
    /// `NotConnected` if there is no token; `TokenExpired` if the refresh token was rejected.
    pub async fn access_token(&self, account_id: &str, cfg: &OAuthConfig) -> Result<String> {
        let lock = self.lock_for(account_id);
        let _guard = lock.lock().await;
        let current = self.load(account_id).await?.ok_or(AppError::NotConnected)?;
        if current.expires_at - EXPIRY_MARGIN_S > self.now() {
            return Ok(current.access_token);
        }
        self.refresh_locked(account_id, cfg, &current).await
    }

    /// Called after the API answered 401 with `stale`. Refreshes only if nobody else has
    /// already replaced that token in the meantime.
    pub async fn refreshed_after(
        &self,
        account_id: &str,
        cfg: &OAuthConfig,
        stale: &str,
    ) -> Result<String> {
        let lock = self.lock_for(account_id);
        let _guard = lock.lock().await;
        let current = self.load(account_id).await?.ok_or(AppError::NotConnected)?;
        if current.access_token != stale {
            return Ok(current.access_token);
        }
        self.refresh_locked(account_id, cfg, &current).await
    }

    async fn refresh_locked(
        &self,
        account_id: &str,
        cfg: &OAuthConfig,
        current: &TokenSet,
    ) -> Result<String> {
        let fresh = oauth::refresh(&self.http, cfg, current, self.now()).await?;
        self.save(account_id, &fresh).await?;
        Ok(fresh.access_token)
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| AppError::Internal)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::oauth::test_config;
    use crate::auth::token_store::{sample_tokens, MemoryStore};
    use crate::clock::FixedClock;
    use time::OffsetDateTime;

    fn service(now: i64) -> (AuthService, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore::new());
        let clock = Arc::new(FixedClock(
            OffsetDateTime::from_unix_timestamp(now).unwrap(),
        ));
        (
            AuthService::new(store.clone(), reqwest::Client::new(), clock),
            store,
        )
    }

    #[tokio::test]
    async fn returns_cached_token_when_valid() {
        let (svc, store) = service(1_000);
        let mut t = sample_tokens();
        t.expires_at = 5_000;
        store.set("acc", &t).unwrap();
        // Unreachable token endpoint: any refresh attempt would fail.
        let cfg = test_config("http://127.0.0.1:9");
        assert_eq!(svc.access_token("acc", &cfg).await.unwrap(), t.access_token);
    }

    #[tokio::test]
    async fn refreshes_when_about_to_expire_and_persists() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/token")
            .with_body(r#"{"access_token":"fresh","expires_in":3600}"#)
            .expect(1)
            .create_async()
            .await;
        let (svc, store) = service(1_000);
        let mut t = sample_tokens();
        t.expires_at = 1_030; // inside the 60 s margin
        store.set("acc", &t).unwrap();
        let cfg = test_config(&srv.url());
        assert_eq!(svc.access_token("acc", &cfg).await.unwrap(), "fresh");
        let saved = store.get("acc").unwrap().unwrap();
        assert_eq!(saved.access_token, "fresh");
        assert_eq!(saved.expires_at, 4_600);
        assert_eq!(saved.refresh_token, t.refresh_token);
        // Second call uses the stored token, no new request.
        assert_eq!(svc.access_token("acc", &cfg).await.unwrap(), "fresh");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn concurrent_refreshes_hit_the_server_once() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/token")
            .with_body(r#"{"access_token":"fresh","expires_in":3600}"#)
            .expect(1)
            .create_async()
            .await;
        let (svc, store) = service(1_000);
        let mut t = sample_tokens();
        t.expires_at = 0;
        store.set("acc", &t).unwrap();
        let cfg = test_config(&srv.url());
        let (a, b) = tokio::join!(svc.access_token("acc", &cfg), svc.access_token("acc", &cfg));
        assert_eq!(a.unwrap(), "fresh");
        assert_eq!(b.unwrap(), "fresh");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn refreshed_after_skips_if_token_already_replaced() {
        let (svc, store) = service(1_000);
        let mut t = sample_tokens();
        t.access_token = "newer".into();
        t.expires_at = 5_000;
        store.set("acc", &t).unwrap();
        let cfg = test_config("http://127.0.0.1:9");
        assert_eq!(
            svc.refreshed_after("acc", &cfg, "older").await.unwrap(),
            "newer"
        );
    }

    #[tokio::test]
    async fn missing_token_is_not_connected() {
        let (svc, _) = service(0);
        let cfg = test_config("http://127.0.0.1:9");
        assert!(matches!(
            svc.access_token("nobody", &cfg).await,
            Err(AppError::NotConnected)
        ));
    }

    #[tokio::test]
    async fn revoked_refresh_token_is_token_expired() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;
        let (svc, store) = service(1_000);
        let mut t = sample_tokens();
        t.expires_at = 0;
        store.set("acc", &t).unwrap();
        let cfg = test_config(&srv.url());
        assert!(matches!(
            svc.access_token("acc", &cfg).await,
            Err(AppError::TokenExpired)
        ));
    }
}
