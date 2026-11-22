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

/// Retries for transient failures (network, 429, 5xx) on idempotent GETs.
const MAX_RETRIES: u32 = 3;
/// Longest wait we accept from `Retry-After` before giving up and reporting `RATE_LIMITED`.
const MAX_RETRY_AFTER_S: u32 = 60;

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    tokens: Arc<dyn TokenSource>,
    retry_base: std::time::Duration,
    /// Sent with every request (Twitch requires `Client-Id`).
    headers: Vec<(String, String)>,
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiClient { .. }")
    }
}

impl ApiClient {
    pub fn new(http: reqwest::Client, tokens: Arc<dyn TokenSource>) -> Self {
        Self {
            http,
            tokens,
            retry_base: std::time::Duration::from_millis(500),
            headers: Vec::new(),
        }
    }

    pub fn with_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    fn decorate(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        req
    }

    pub fn unauthenticated() -> Self {
        Self::new(reqwest::Client::new(), Arc::new(NoToken))
    }

    /// Base delay of the exponential backoff (tests use a tiny one).
    pub fn with_retry_base(mut self, base: std::time::Duration) -> Self {
        self.retry_base = base;
        self
    }

    /// Exponential backoff with full jitter: random in [base·2^n / 2, base·2^n].
    fn backoff(&self, retry: u32) -> std::time::Duration {
        let max = self.retry_base * 2u32.pow(retry);
        let jitter: f64 = rand::random();
        max.mul_f64(0.5 + jitter / 2.0)
    }

    /// GET with bearer token. A 401 refreshes the token once; network errors, 429 and 5xx
    /// are retried up to [`MAX_RETRIES`] times with backoff (honouring `Retry-After`).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let mut token = self.tokens.token().await?;
        let mut refreshed = false;
        let mut retries = 0;
        loop {
            let sent = self
                .decorate(self.http.get(url))
                .query(query)
                .bearer_auth(&token)
                .send()
                .await;
            let resp = match sent {
                Ok(r) => r,
                Err(e) if retries < MAX_RETRIES => {
                    tracing::debug!(error = %e, retries, "GET failed, retrying");
                    tokio::time::sleep(self.backoff(retries)).await;
                    retries += 1;
                    continue;
                }
                Err(_) => return Err(AppError::Network),
            };
            let status = resp.status().as_u16();
            if status == 401 && !refreshed {
                refreshed = true;
                token = self.tokens.refreshed(&token).await?;
                continue;
            }
            if resp.status().is_success() {
                return resp.json().await.map_err(|e| {
                    tracing::warn!(error = %e, "unexpected response body");
                    AppError::Internal
                });
            }
            if (status == 429 || status >= 500) && retries < MAX_RETRIES {
                let wait = match crate::auth::oauth::retry_after(&resp) {
                    Some(s) if s > MAX_RETRY_AFTER_S => return Err(map_status(resp).await),
                    Some(s) => std::time::Duration::from_secs(s.into()),
                    None => self.backoff(retries),
                };
                tracing::debug!(status, retries, ?wait, "transient API error, retrying");
                tokio::time::sleep(wait).await;
                retries += 1;
                continue;
            }
            return Err(map_status(resp).await);
        }
    }
}

impl ApiClient {
    /// Sends a request built by `build` with the bearer token, refreshing it once on 401.
    /// No other retries (uploads handle their own resume logic); any other status is
    /// returned to the caller, who can map errors with [`error_from`].
    pub async fn send(
        &self,
        build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let mut token = self.tokens.token().await?;
        let mut refreshed = false;
        loop {
            let resp = self
                .decorate(build(&self.http))
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| {
                    tracing::debug!(error = %e, "request failed");
                    AppError::Network
                })?;
            if resp.status().as_u16() == 401 && !refreshed {
                refreshed = true;
                token = self.tokens.refreshed(&token).await?;
                continue;
            }
            return Ok(resp);
        }
    }
}

impl ApiClient {
    /// Like [`ApiClient::send`], then parses a successful JSON body or maps the error.
    pub async fn send_json<T: DeserializeOwned>(
        &self,
        build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    ) -> Result<T> {
        let resp = self.send(build).await?;
        if !resp.status().is_success() {
            return Err(map_status(resp).await);
        }
        resp.json().await.map_err(|e| {
            tracing::warn!(error = %e, "unexpected response body");
            AppError::Internal
        })
    }

    /// The current access token, for upload hosts that want it in another header
    /// (Instagram's `rupload` uses `Authorization: OAuth <token>`). Never log it.
    pub async fn access_token(&self) -> Result<String> {
        self.tokens.token().await
    }

    /// The underlying client, for requests that must not carry the bearer token
    /// (pre-signed upload URLs).
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

/// Maps an error response to an [`AppError`] without leaking its body.
pub async fn error_from(resp: reqwest::Response) -> AppError {
    map_status(resp).await
}

/// Maps an error response to an [`AppError`] without leaking its body.
async fn map_status(resp: reqwest::Response) -> AppError {
    let status = resp.status().as_u16();
    let retry_after = crate::auth::oauth::retry_after(&resp);
    let body = resp.text().await.unwrap_or_default();
    // Google's machine-readable reason (`quotaExceeded`, `invalidTitle`...): safe to log.
    let reason = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v["error"]["errors"][0]["reason"]
                .as_str()
                .map(str::to_owned)
        });
    tracing::debug!(status, ?reason, "API error");
    match status {
        401 => AppError::TokenExpired,
        400 | 403
            if body.contains("quotaExceeded")
                || body.contains("dailyLimitExceeded")
                || body.contains("uploadLimitExceeded") =>
        {
            AppError::QuotaExceeded
        }
        400 => match reason.as_deref() {
            Some("invalidTitle") => AppError::Validation {
                field: "title".into(),
            },
            Some("invalidDescription") => AppError::Validation {
                field: "description".into(),
            },
            Some("invalidTags") => AppError::Validation {
                field: "tags".into(),
            },
            Some("invalidCategoryId") => AppError::Validation {
                field: "category".into(),
            },
            _ => {
                tracing::warn!(?reason, "bad request");
                AppError::Internal
            }
        },
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
    async fn retries_transient_errors() {
        let mut srv = mockito::Server::new_async().await;
        let failing = srv
            .mock("GET", "/r")
            .with_status(503)
            .expect(2)
            .create_async()
            .await;
        // Once the failing mock has had its 2 hits, mockito moves on to this one.
        let ok = srv
            .mock("GET", "/r")
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;
        let api = ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into())))
            .with_retry_base(std::time::Duration::from_millis(1));
        let v: serde_json::Value = api
            .get_json(&format!("{}/r", srv.url()), &[])
            .await
            .unwrap();
        failing.assert_async().await;
        ok.assert_async().await;
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn gives_up_after_max_retries_and_honours_retry_after() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("GET", "/r")
            .with_status(429)
            .with_header("retry-after", "0")
            .expect(1 + MAX_RETRIES as usize)
            .create_async()
            .await;
        let api = ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into())))
            .with_retry_base(std::time::Duration::from_millis(1));
        let r: Result<serde_json::Value> = api.get_json(&format!("{}/r", srv.url()), &[]).await;
        m.assert_async().await;
        assert!(matches!(r, Err(AppError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn long_retry_after_is_not_waited() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("GET", "/r")
            .with_status(429)
            .with_header("retry-after", "3600")
            .expect(1)
            .create_async()
            .await;
        let api = ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into())));
        let r: Result<serde_json::Value> = api.get_json(&format!("{}/r", srv.url()), &[]).await;
        m.assert_async().await;
        assert!(matches!(
            r,
            Err(AppError::RateLimited {
                retry_after_s: 3600
            })
        ));
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
