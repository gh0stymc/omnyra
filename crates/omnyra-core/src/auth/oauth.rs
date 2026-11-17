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
    /// Device authorization endpoint (RFC 8628). When set, connecting uses the device
    /// flow (the user types a code on the provider's site) instead of the loopback
    /// redirect: Twitch public clients only allow that flow.
    pub device_url: Option<String>,
    /// Twitch wants `client_id` in revocation requests; Google does not.
    pub revoke_with_client_id: bool,
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

/// Google sends scopes as one space-separated string, Twitch as an array.
#[derive(Deserialize)]
#[serde(untagged)]
enum Scope {
    Joined(String),
    List(Vec<String>),
}

impl Scope {
    fn into_vec(self) -> Vec<String> {
        match self {
            Scope::Joined(s) => parse_scopes(&s),
            Scope::List(v) => parse_scopes(&v.join(" ")),
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<Scope>,
}

/// RFC 6749 errors (`{"error": "invalid_grant"}`) and Twitch's
/// (`{"status": 400, "message": "Invalid refresh token"}`).
#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
    message: Option<String>,
}

impl ErrorResponse {
    /// Normalised error code (`invalid_grant`, `authorization_pending`...).
    fn code(&self) -> String {
        let raw = self
            .error
            .as_deref()
            .or(self.message.as_deref())
            .unwrap_or_default()
            .to_lowercase();
        match raw.as_str() {
            "invalid refresh token" => "invalid_grant".into(),
            "invalid token" => "invalid_token".into(),
            "invalid device code" => "expired_token".into(),
            _ => raw.replace(' ', "_"),
        }
    }
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
    let code = resp
        .json::<ErrorResponse>()
        .await
        .map(|e| e.code())
        .unwrap_or_default();
    match code.as_str() {
        "invalid_grant" => Err(TokenError::InvalidGrant),
        "authorization_pending" => Err(TokenError::Pending),
        "slow_down" => Err(TokenError::SlowDown),
        "access_denied" => Err(TokenError::Other(AppError::AuthCancelled)),
        "expired_token" => Err(TokenError::Other(AppError::AuthTimeout)),
        _ => {
            tracing::warn!(%status, code, "token request rejected");
            Err(TokenError::Other(AppError::AuthFailed))
        }
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
    /// Device flow: the user has not finished yet.
    Pending,
    /// Device flow: poll less often.
    SlowDown,
    Other(AppError),
}

impl TokenError {
    fn into_app(self, invalid_grant: AppError) -> AppError {
        match self {
            TokenError::InvalidGrant => invalid_grant,
            TokenError::Pending | TokenError::SlowDown => AppError::AuthFailed,
            TokenError::Other(e) => e,
        }
    }
}

fn token_set(
    r: TokenResponse,
    now: i64,
    fallback_scopes: Vec<String>,
    old_refresh: Option<String>,
) -> TokenSet {
    TokenSet {
        access_token: r.access_token,
        refresh_token: r.refresh_token.or(old_refresh),
        expires_at: now + r.expires_in.unwrap_or(DEFAULT_EXPIRES_IN),
        scopes: r.scope.map(Scope::into_vec).unwrap_or(fallback_scopes),
    }
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
    .map_err(|e| e.into_app(AppError::AuthFailed))?;
    Ok(token_set(
        r,
        now,
        parse_scopes(&requested_scopes.join(" ")),
        None,
    ))
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
    .map_err(|e| e.into_app(AppError::TokenExpired))?;
    // Twitch public clients get a new, single-use refresh token every time.
    Ok(token_set(
        r,
        now,
        current.scopes.clone(),
        current.refresh_token.clone(),
    ))
}

/// Device authorization (RFC 8628) as returned to the UI: the user opens
/// `verification_uri` and confirms `user_code`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

pub async fn start_device(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    scopes: &[String],
) -> Result<DeviceCode> {
    let url = cfg.device_url.as_deref().ok_or(AppError::NotSupported)?;
    let joined = scopes.join(" ");
    let resp = http
        .post(url)
        .header("Accept", "application/json")
        // Twitch names it `scopes`; RFC 8628 says `scope`. Send both.
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("scopes", &joined),
            ("scope", &joined),
        ])
        .send()
        .await
        .map_err(|_| AppError::Network)?;
    if resp.status().is_server_error() {
        return Err(AppError::Network);
    }
    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "device authorization rejected");
        return Err(AppError::AuthFailed);
    }
    resp.json().await.map_err(|_| AppError::AuthFailed)
}

/// Polls the token endpoint until the user approves, denies or the code expires.
/// `now` gives the Unix time for `expires_at`; `sleep` is injectable for tests.
pub async fn poll_device(
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    dc: &DeviceCode,
    scopes: &[String],
    now: impl Fn() -> i64,
    poll_unit: std::time::Duration,
) -> Result<TokenSet> {
    let joined = scopes.join(" ");
    let mut interval = dc.interval.max(1);
    let deadline = now() + dc.expires_in as i64;
    loop {
        tokio::time::sleep(poll_unit * interval as u32).await;
        if now() > deadline {
            return Err(AppError::AuthTimeout);
        }
        let r = token_request(
            http,
            cfg,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &dc.device_code),
                ("scopes", &joined),
            ],
        )
        .await;
        match r {
            Ok(r) => return Ok(token_set(r, now(), parse_scopes(&joined), None)),
            Err(TokenError::Pending) => {}
            Err(TokenError::SlowDown) => interval += 5,
            Err(e) => return Err(e.into_app(AppError::AuthFailed)),
        }
    }
}

/// Revokes a token (Google: revoking the refresh token revokes the whole grant).
/// A token the server no longer knows counts as revoked.
pub async fn revoke(http: &reqwest::Client, cfg: &OAuthConfig, token: &str) -> Result<()> {
    let Some(url) = &cfg.revoke_url else {
        return Err(AppError::NotSupported);
    };
    let mut form = vec![("token", token)];
    if cfg.revoke_with_client_id {
        form.push(("client_id", &cfg.client_id));
    }
    let resp = http.post(url).form(&form).send().await.map_err(|e| {
        // reqwest errors carry the URL but never the form body, so no token leaks here.
        tracing::warn!(error = %e, "token revocation request failed");
        AppError::Network
    })?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    if status.as_u16() == 400 {
        let code = resp.json::<ErrorResponse>().await.map(|e| e.code()).ok();
        if code.as_deref() == Some("invalid_token") {
            return Ok(());
        }
        tracing::warn!(%status, error = ?code, "token revocation rejected");
        return Err(AppError::AuthFailed);
    }
    tracing::warn!(%status, "token revocation returned an unexpected status");
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
        device_url: None,
        revoke_with_client_id: false,
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

    fn device_config(server_url: &str) -> OAuthConfig {
        OAuthConfig {
            client_secret: None,
            device_url: Some(format!("{server_url}/device")),
            revoke_with_client_id: true,
            ..test_config(server_url)
        }
    }

    #[tokio::test]
    async fn device_flow_polls_until_approved_and_reads_array_scopes() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/device")
            .match_body(Matcher::UrlEncoded("scopes".into(), "a:b".into()))
            .with_body(r#"{"device_code":"dc","user_code":"ABCD1234","verification_uri":"https://www.twitch.tv/activate?device-code=ABCD1234","expires_in":1800,"interval":1}"#)
            .create_async()
            .await;
        let pending = srv
            .mock("POST", "/token")
            .match_body(Matcher::UrlEncoded("device_code".into(), "dc".into()))
            .with_status(400)
            .with_body(r#"{"status":400,"message":"authorization_pending"}"#)
            .expect(2)
            .create_async()
            .await;
        srv.mock("POST", "/token")
            .with_body(r#"{"access_token":"at","refresh_token":"rt","expires_in":14400,"scope":["a:b"],"token_type":"bearer"}"#)
            .create_async()
            .await;
        let cfg = device_config(&srv.url());
        let http = reqwest::Client::new();
        let scopes = vec!["a:b".to_string()];
        let dc = start_device(&http, &cfg, &scopes).await.unwrap();
        assert_eq!(dc.user_code, "ABCD1234");
        let t = poll_device(
            &http,
            &cfg,
            &dc,
            &scopes,
            || 100,
            std::time::Duration::from_millis(1),
        )
        .await
        .unwrap();
        pending.assert_async().await;
        assert_eq!(t.refresh_token.as_deref(), Some("rt"));
        assert_eq!(t.expires_at, 100 + 14400);
        assert_eq!(t.scopes, vec!["a:b"]);
    }

    #[tokio::test]
    async fn device_flow_denied_and_expired() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"status":400,"message":"access_denied"}"#)
            .create_async()
            .await;
        let cfg = device_config(&srv.url());
        let dc = DeviceCode {
            device_code: "dc".into(),
            user_code: "u".into(),
            verification_uri: "v".into(),
            expires_in: 60,
            interval: 1,
        };
        let http = reqwest::Client::new();
        let r = poll_device(
            &http,
            &cfg,
            &dc,
            &[],
            || 0,
            std::time::Duration::from_millis(1),
        )
        .await;
        assert!(matches!(r, Err(AppError::AuthCancelled)));
        // Past the deadline before the first poll.
        let t = std::sync::atomic::AtomicI64::new(0);
        let r = poll_device(
            &http,
            &cfg,
            &dc,
            &[],
            || t.fetch_add(100, std::sync::atomic::Ordering::SeqCst) + 100,
            std::time::Duration::from_millis(1),
        )
        .await;
        assert!(matches!(r, Err(AppError::AuthTimeout)));
    }

    #[tokio::test]
    async fn twitch_style_refresh_rejection_means_reconnect() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"status":400,"message":"Invalid refresh token"}"#)
            .create_async()
            .await;
        let cfg = device_config(&srv.url());
        let cur = TokenSet {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: 0,
            scopes: vec![],
        };
        let r = refresh(&reqwest::Client::new(), &cfg, &cur, 0).await;
        assert!(matches!(r, Err(AppError::TokenExpired)));
    }

    #[tokio::test]
    async fn revoke_sends_client_id_when_required() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("POST", "/revoke")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("client_id".into(), "cid.apps.googleusercontent.com".into()),
                Matcher::UrlEncoded("token".into(), "t".into()),
            ]))
            .create_async()
            .await;
        revoke(&reqwest::Client::new(), &device_config(&srv.url()), "t")
            .await
            .unwrap();
        m.assert_async().await;
    }
}
