//! OAuth 2.0 authorization-code flow with PKCE for native apps (RFC 6749 / 7636 / 8252):
//! authorization URL, code exchange, refresh and revocation.
//!
//! Hand-rolled on top of `reqwest` instead of the `oauth2` crate: it is ~150 lines, keeps
//! a single HTTP stack and lets us control exactly what can end up in an error or a log.

use super::token_store::TokenSet;
use crate::{AppError, Result};
use serde::Deserialize;

/// Static OAuth description of a platform. `client_secret` is only for providers that
/// document it as non-confidential for installed apps (Google "Desktop app", R3).
#[derive(Clone)]
pub struct OAuthConfig {
    pub auth_url: String,
    pub token_url: String,
    pub revoke_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Minimum scopes requested on connect (read-only).
    pub base_scopes: Vec<String>,
    /// Extra query parameters for the authorization URL (`access_type=offline`...).
    pub extra_auth_params: Vec<(String, String)>,
}

impl std::fmt::Debug for OAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthConfig")
            .field("auth_url", &self.auth_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("base_scopes", &self.base_scopes)
            .finish_non_exhaustive()
    }
}

pub fn authorization_url(
    cfg: &OAuthConfig,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    code_challenge: &str,
) -> String {
    let mut url = url::Url::parse(&cfg.auth_url).expect("valid auth_url");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &cfg.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in &cfg.extra_auth_params {
            q.append_pair(k, v);
        }
    }
    url.into()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Default lifetime when the server omits `expires_in`.
const DEFAULT_EXPIRES_IN: i64 = 3600;

fn parse_scopes(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.split_whitespace().map(str::to_owned).collect();
    v.sort();
    v.dedup();
    v
}

async fn token_request(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    form: &[(&str, &str)],
) -> std::result::Result<TokenResponse, TokenError> {
    let mut body: Vec<(&str, &str)> = vec![("client_id", &cfg.client_id)];
    if let Some(secret) = &cfg.client_secret {
        body.push(("client_secret", secret));
    }
    body.extend_from_slice(form);
    let resp = http
        .post(&cfg.token_url)
        .header("Accept", "application/json")
        .form(&body)
        .send()
        .await
        .map_err(|_| TokenError::Other(AppError::Network))?;
    let status = resp.status();
    if status.is_success() {
        return resp
            .json::<TokenResponse>()
            .await
            .map_err(|_| TokenError::Other(AppError::AuthFailed));
    }
    if status.as_u16() == 429 {
        let retry = retry_after(&resp).unwrap_or(30);
        return Err(TokenError::Other(AppError::RateLimited {
            retry_after_s: retry,
        }));
    }
    if status.is_server_error() {
        return Err(TokenError::Other(AppError::Network));
    }
    match resp.json::<ErrorResponse>().await {
        Ok(e) if e.error == "invalid_grant" => Err(TokenError::InvalidGrant),
        _ => Err(TokenError::Other(AppError::AuthFailed)),
    }
}

pub(crate) fn retry_after(resp: &reqwest::Response) -> Option<u32> {
    resp.headers()
        .get("retry-after")?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

enum TokenError {
    /// Code or refresh token rejected (revoked, expired, already used...).
    InvalidGrant,
    Other(AppError),
}

/// Exchanges an authorization code. `now` is a Unix timestamp (seconds).
pub async fn exchange_code(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    requested_scopes: &[String],
    now: i64,
) -> Result<TokenSet> {
    let r = token_request(
        http,
        cfg,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", code_verifier),
            ("redirect_uri", redirect_uri),
        ],
    )
    .await
    .map_err(|e| match e {
        TokenError::InvalidGrant => AppError::AuthFailed,
        TokenError::Other(e) => e,
    })?;
    Ok(TokenSet {
        access_token: r.access_token,
        refresh_token: r.refresh_token,
        expires_at: now + r.expires_in.unwrap_or(DEFAULT_EXPIRES_IN),
        scopes: r
            .scope
            .as_deref()
            .map(parse_scopes)
            .unwrap_or_else(|| parse_scopes(&requested_scopes.join(" "))),
    })
}

/// Refreshes `current`. Keeps the old refresh token and scopes when the server omits them.
/// A rejected refresh token becomes [`AppError::TokenExpired`] (the user must reconnect).
pub async fn refresh(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    current: &TokenSet,
    now: i64,
) -> Result<TokenSet> {
    let Some(rt) = current.refresh_token.as_deref() else {
        return Err(AppError::TokenExpired);
    };
    let r = token_request(
        http,
        cfg,
        &[("grant_type", "refresh_token"), ("refresh_token", rt)],
    )
    .await
    .map_err(|e| match e {
        TokenError::InvalidGrant => AppError::TokenExpired,
        TokenError::Other(e) => e,
    })?;
    Ok(TokenSet {
        access_token: r.access_token,
        refresh_token: r.refresh_token.or_else(|| current.refresh_token.clone()),
        expires_at: now + r.expires_in.unwrap_or(DEFAULT_EXPIRES_IN),
        scopes: r
            .scope
            .as_deref()
            .map(parse_scopes)
            .unwrap_or_else(|| current.scopes.clone()),
    })
}

/// Revokes a token (Google: revoking the refresh token revokes the whole grant).
/// A token the server no longer knows counts as revoked.
pub async fn revoke(http: &reqwest::Client, cfg: &OAuthConfig, token: &str) -> Result<()> {
    let Some(url) = &cfg.revoke_url else {
        return Err(AppError::NotSupported);
    };
    let resp = http
        .post(url)
        .form(&[("token", token)])
        .send()
        .await
        .map_err(|_| AppError::Network)?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    if status.as_u16() == 400 {
        if let Ok(e) = resp.json::<ErrorResponse>().await {
            if e.error == "invalid_token" {
                return Ok(());
            }
        }
        return Err(AppError::AuthFailed);
    }
    Err(AppError::Network)
}

#[cfg(test)]
pub(crate) fn test_config(server_url: &str) -> OAuthConfig {
    OAuthConfig {
        auth_url: format!("{server_url}/auth"),
        token_url: format!("{server_url}/token"),
        revoke_url: Some(format!("{server_url}/revoke")),
        client_id: "cid.apps.googleusercontent.com".into(),
        client_secret: Some("csecret".into()),
        base_scopes: vec!["scope.read".into(), "scope.analytics".into()],
        extra_auth_params: vec![("access_type".into(), "offline".into())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token_store::sample_tokens;
    use mockito::Matcher;

    #[test]
    fn builds_authorization_url() {
        let cfg = test_config("https://accounts.example");
        let url = authorization_url(
            &cfg,
            "http://127.0.0.1:5555/",
            &cfg.base_scopes,
            "st",
            "chal",
        );
        let parsed = url::Url::parse(&url).unwrap();
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], cfg.client_id);
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:5555/");
        assert_eq!(q["scope"], "scope.read scope.analytics");
        assert_eq!(q["state"], "st");
        assert_eq!(q["code_challenge"], "chal");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["access_type"], "offline");
        // The secret never goes in the browser URL.
        assert!(!url.contains("csecret"));
    }

    #[test]
    fn debug_redacts_client_secret() {
        let cfg = test_config("https://x");
        assert!(!format!("{cfg:?}").contains("csecret"));
    }

    #[tokio::test]
    async fn exchanges_code_with_verifier() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
                Matcher::UrlEncoded("code".into(), "the-code".into()),
                Matcher::UrlEncoded("code_verifier".into(), "the-verifier".into()),
                Matcher::UrlEncoded("redirect_uri".into(), "http://127.0.0.1:1/".into()),
                Matcher::UrlEncoded("client_id".into(), "cid.apps.googleusercontent.com".into()),
                Matcher::UrlEncoded("client_secret".into(), "csecret".into()),
            ]))
            .with_body(
                r#"{"access_token":"at","expires_in":3599,"refresh_token":"rt",
                    "scope":"scope.read scope.analytics","token_type":"Bearer"}"#,
            )
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let t = exchange_code(
            &reqwest::Client::new(),
            &cfg,
            "the-code",
            "the-verifier",
            "http://127.0.0.1:1/",
            &cfg.base_scopes,
            1000,
        )
        .await
        .unwrap();
        m.assert_async().await;
        assert_eq!(t.access_token, "at");
        assert_eq!(t.refresh_token.as_deref(), Some("rt"));
        assert_eq!(t.expires_at, 4599);
        assert_eq!(t.scopes, vec!["scope.analytics", "scope.read"]);
    }

    #[tokio::test]
    async fn rejected_code_is_auth_failed() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let r = exchange_code(&reqwest::Client::new(), &cfg, "c", "v", "r", &[], 0).await;
        assert!(matches!(r, Err(AppError::AuthFailed)));
    }

    #[tokio::test]
    async fn refresh_keeps_refresh_token_when_omitted() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("grant_type".into(), "refresh_token".into()),
                Matcher::UrlEncoded(
                    "refresh_token".into(),
                    sample_tokens().refresh_token.unwrap(),
                ),
            ]))
            .with_body(r#"{"access_token":"new-at","expires_in":3600}"#)
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let cur = sample_tokens();
        let t = refresh(&reqwest::Client::new(), &cfg, &cur, 10)
            .await
            .unwrap();
        assert_eq!(t.access_token, "new-at");
        assert_eq!(t.refresh_token, cur.refresh_token);
        assert_eq!(t.scopes, cur.scopes);
        assert_eq!(t.expires_at, 3610);
    }

    #[tokio::test]
    async fn invalid_grant_on_refresh_is_token_expired() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#)
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let r = refresh(&reqwest::Client::new(), &cfg, &sample_tokens(), 0).await;
        assert!(matches!(r, Err(AppError::TokenExpired)));
    }

    #[tokio::test]
    async fn refresh_without_refresh_token_is_token_expired() {
        let cfg = test_config("http://127.0.0.1:9");
        let mut t = sample_tokens();
        t.refresh_token = None;
        let r = refresh(&reqwest::Client::new(), &cfg, &t, 0).await;
        assert!(matches!(r, Err(AppError::TokenExpired)));
    }

    #[tokio::test]
    async fn server_errors_are_network_and_429_is_rate_limited() {
        let mut srv = mockito::Server::new_async().await;
        let m500 = srv
            .mock("POST", "/token")
            .with_status(503)
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let http = reqwest::Client::new();
        assert!(matches!(
            refresh(&http, &cfg, &sample_tokens(), 0).await,
            Err(AppError::Network)
        ));
        m500.remove_async().await;
        srv.mock("POST", "/token")
            .with_status(429)
            .with_header("retry-after", "12")
            .create_async()
            .await;
        assert!(matches!(
            refresh(&http, &cfg, &sample_tokens(), 0).await,
            Err(AppError::RateLimited { retry_after_s: 12 })
        ));
    }

    #[tokio::test]
    async fn revoke_treats_unknown_token_as_revoked() {
        let mut srv = mockito::Server::new_async().await;
        let ok = srv
            .mock("POST", "/revoke")
            .match_body(Matcher::UrlEncoded("token".into(), "good".into()))
            .create_async()
            .await;
        srv.mock("POST", "/revoke")
            .match_body(Matcher::UrlEncoded("token".into(), "gone".into()))
            .with_status(400)
            .with_body(r#"{"error":"invalid_token"}"#)
            .create_async()
            .await;
        let cfg = test_config(&srv.url());
        let http = reqwest::Client::new();
        revoke(&http, &cfg, "good").await.unwrap();
        ok.assert_async().await;
        revoke(&http, &cfg, "gone").await.unwrap();
    }

    #[tokio::test]
    async fn revoke_network_failure_is_reported() {
        let cfg = test_config("http://127.0.0.1:9");
        let r = revoke(&reqwest::Client::new(), &cfg, "t").await;
        assert!(matches!(r, Err(AppError::Network)));
    }
}
