//! Authenticated HTTP client handed to connectors through [`super::AccountCtx`].
//! It injects the bearer token and, on a 401, refreshes once and retries.
//! Rate limiting, backoff, quota ledger and ETag cache arrive in step 1b.

use crate::auth::{AuthService, OAuthConfig};
use crate::{AppError, Result};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::sync::Arc;

/// Shared HTTP client: rustls with the OS trust store, sane timeouts, no secrets in the UA.
pub fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Omnyra/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("HTTP client")
}

#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn token(&self) -> Result<String>;
    /// New token after the API rejected `stale` with 401.
    async fn refreshed(&self, stale: &str) -> Result<String>;
}

/// Fixed token (used right after the code exchange, before the account exists).
pub struct StaticToken(pub String);

#[async_trait]
impl TokenSource for StaticToken {
    async fn token(&self) -> Result<String> {
        Ok(self.0.clone())
    }
    async fn refreshed(&self, _stale: &str) -> Result<String> {
        Err(AppError::TokenExpired)
    }
}

/// No credentials (mock connector, public endpoints).
pub struct NoToken;

#[async_trait]
impl TokenSource for NoToken {
    async fn token(&self) -> Result<String> {
        Err(AppError::NotConnected)
    }
    async fn refreshed(&self, _stale: &str) -> Result<String> {
        Err(AppError::NotConnected)
    }
}

/// Tokens of a stored account, refreshed through [`AuthService`].
pub struct AccountTokens {
    pub auth: Arc<AuthService>,
    pub account_id: String,
    pub config: Arc<OAuthConfig>,
}

#[async_trait]
impl TokenSource for AccountTokens {
    async fn token(&self) -> Result<String> {
        self.auth.access_token(&self.account_id, &self.config).await
    }
    async fn refreshed(&self, stale: &str) -> Result<String> {
        self.auth
            .refreshed_after(&self.account_id, &self.config, stale)
            .await
    }
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    tokens: Arc<dyn TokenSource>,
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiClient { .. }")
    }
}

impl ApiClient {
    pub fn new(http: reqwest::Client, tokens: Arc<dyn TokenSource>) -> Self {
        Self { http, tokens }
    }

    pub fn unauthenticated() -> Self {
        Self::new(reqwest::Client::new(), Arc::new(NoToken))
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let mut token = self.tokens.token().await?;
        for attempt in 0..2 {
            let resp = self
                .http
                .get(url)
                .query(query)
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|_| AppError::Network)?;
            let status = resp.status().as_u16();
            if status == 401 && attempt == 0 {
                token = self.tokens.refreshed(&token).await?;
                continue;
            }
            if resp.status().is_success() {
                return resp.json().await.map_err(|_| AppError::Internal);
            }
            return Err(map_status(resp).await);
        }
        Err(AppError::TokenExpired)
    }
}

/// Maps an error response to an [`AppError`] without leaking its body.
async fn map_status(resp: reqwest::Response) -> AppError {
    let status = resp.status().as_u16();
    let retry_after = crate::auth::oauth::retry_after(&resp);
    let body = resp.text().await.unwrap_or_default();
    match status {
        401 => AppError::TokenExpired,
        403 if body.contains("quotaExceeded") || body.contains("dailyLimitExceeded") => {
            AppError::QuotaExceeded
        }
        403 if body.contains("insufficientPermissions")
            || body.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") =>
        {
            AppError::ScopesDenied
        }
        403 if body.contains("rateLimitExceeded") => AppError::RateLimited {
            retry_after_s: retry_after.unwrap_or(30),
        },
        404 => AppError::NotFound,
        429 => AppError::RateLimited {
            retry_after_s: retry_after.unwrap_or(30),
        },
        s if s >= 500 => AppError::Network,
        _ => AppError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Rotating(AtomicU32);

    #[async_trait]
    impl TokenSource for Rotating {
        async fn token(&self) -> Result<String> {
            Ok("old".into())
        }
        async fn refreshed(&self, stale: &str) -> Result<String> {
            assert_eq!(stale, "old");
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("new".into())
        }
    }

    #[tokio::test]
    async fn retries_once_with_refreshed_token_on_401() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/r")
            .match_header("authorization", "Bearer old")
            .with_status(401)
            .create_async()
            .await;
        srv.mock("GET", "/r")
            .match_header("authorization", "Bearer new")
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;
        let src = Arc::new(Rotating(AtomicU32::new(0)));
        let api = ApiClient::new(reqwest::Client::new(), src.clone());
        let v: serde_json::Value = api
            .get_json(&format!("{}/r", srv.url()), &[])
            .await
            .unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(src.0.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn maps_quota_and_scope_errors() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/q")
            .with_status(403)
            .with_body(r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#)
            .create_async()
            .await;
        srv.mock("GET", "/s")
            .with_status(403)
            .with_body(r#"{"error":{"errors":[{"reason":"insufficientPermissions"}]}}"#)
            .create_async()
            .await;
        let api = ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into())));
        let q: Result<serde_json::Value> = api.get_json(&format!("{}/q", srv.url()), &[]).await;
        assert!(matches!(q, Err(AppError::QuotaExceeded)));
        let s: Result<serde_json::Value> = api.get_json(&format!("{}/s", srv.url()), &[]).await;
        assert!(matches!(s, Err(AppError::ScopesDenied)));
    }

    #[tokio::test]
    async fn static_token_gives_up_after_401() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/r").with_status(401).create_async().await;
        let api = ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into())));
        let r: Result<serde_json::Value> = api.get_json(&format!("{}/r", srv.url()), &[]).await;
        assert!(matches!(r, Err(AppError::TokenExpired)));
    }
}
