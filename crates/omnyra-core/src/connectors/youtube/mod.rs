//! YouTube connector: Data API v3 (profile, totals, uploads, per-video stats; metered in
//! the quota ledger), Analytics API (daily history) and Shorts detection.

mod analytics;
mod api;
mod duration;
mod shorts;

use super::*;
use crate::auth::ClientCredentials;

pub const SCOPE_READONLY: &str = "https://www.googleapis.com/auth/youtube.readonly";
pub const SCOPE_ANALYTICS: &str = "https://www.googleapis.com/auth/yt-analytics.readonly";
/// Requested incrementally the first time the user uploads (step 1d).
pub const SCOPE_UPLOAD: &str = "https://www.googleapis.com/auth/youtube.upload";
/// Requested incrementally for comments (step 4a).
pub const SCOPE_FORCE_SSL: &str = "https://www.googleapis.com/auth/youtube.force-ssl";

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const API_BASE: &str = "https://www.googleapis.com";
const ANALYTICS_BASE: &str = "https://youtubeanalytics.googleapis.com";
const WEB_BASE: &str = "https://www.youtube.com";

pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    pub revoke_url: String,
    pub api_base: String,
    pub analytics_base: String,
    /// youtube.com, for the Shorts check.
    pub web_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth_url: GOOGLE_AUTH_URL.into(),
            token_url: GOOGLE_TOKEN_URL.into(),
            revoke_url: GOOGLE_REVOKE_URL.into(),
            api_base: API_BASE.into(),
            analytics_base: ANALYTICS_BASE.into(),
            web_base: WEB_BASE.into(),
        }
    }
}

pub struct YouTubeConnector {
    info: PlatformInfo,
    oauth: Option<Arc<OAuthConfig>>,
    api_base: String,
    analytics_base: String,
    web_base: String,
    /// No redirects, no cookies, no token (see `shorts.rs`).
    web_http: reqwest::Client,
}

impl YouTubeConnector {
    pub fn new(credentials: Option<ClientCredentials>) -> Self {
        Self::with_endpoints(credentials, Endpoints::default())
    }

    pub fn with_endpoints(credentials: Option<ClientCredentials>, ep: Endpoints) -> Self {
        let oauth = credentials.map(|c| {
            Arc::new(OAuthConfig {
                auth_url: ep.auth_url,
                token_url: ep.token_url,
                revoke_url: Some(ep.revoke_url),
                client_id: c.client_id,
                client_secret: c.client_secret,
                base_scopes: vec![SCOPE_READONLY.into(), SCOPE_ANALYTICS.into()],
                extra_auth_params: vec![
                    // Get a refresh token, and always show the account/channel picker and the
                    // consent screen, so a refresh token is returned on every (re)connection.
                    ("access_type".into(), "offline".into()),
                    ("prompt".into(), "select_account consent".into()),
                    ("include_granted_scopes".into(), "true".into()),
                ],
            })
        });
        Self {
            info: PlatformInfo {
                id: "youtube".into(),
                display_name: "YouTube".into(),
                brand_color: "#ff0033".into(),
                capabilities: Capabilities {
                    analytics: true,
                    historic_analytics: true,
                    upload: UploadSupport::PrivateOnlyUnaudited,
                    resumable_upload: true,
                    schedule: ScheduleSupport::Native,
                    comments: CommentSupport::ReadReply,
                    live: true,
                    public_lookup: true,
                    limits: MediaLimits {
                        // 12 h for verified accounts (15 min otherwise; checked in 1d).
                        max_duration_s: Some(12 * 3600),
                        max_size_mb: Some(256 * 1024),
                        max_title_len: Some(100),
                        max_description_len: Some(5000),
                        // YouTube ignores every hashtag if there are more than 60.
                        max_hashtags: Some(60),
                    },
                },
                auth: AuthKind::OAuth,
                auth_configured: oauth.is_some(),
            },
            oauth,
            api_base: ep.api_base,
            analytics_base: ep.analytics_base,
            web_base: ep.web_base,
            web_http: shorts::client(),
        }
    }
}

#[async_trait]
impl Connector for YouTubeConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        self.oauth.as_ref()
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        api::profile(ctx, &self.api_base).await
    }

    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot> {
        api::snapshot(ctx, &self.api_base).await
    }

    async fn fetch_channel_metrics(
        &self,
        ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        analytics::daily_metrics(ctx, &self.analytics_base, &range.from, &range.to).await
    }

    async fn list_contents(
        &self,
        ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>> {
        api::list_uploads(ctx, &self.api_base, page).await
    }

    async fn fetch_content_details(
        &self,
        ctx: &AccountCtx,
        ids: &[String],
    ) -> Result<Vec<ContentDetails>> {
        api::videos(ctx, &self.api_base, ids).await
    }

    async fn classify(&self, _ctx: &AccountCtx, ids: &[String]) -> Vec<(String, String)> {
        shorts::classify(&self.web_http, &self.web_base, ids).await
    }
}

#[cfg(test)]
pub(crate) fn test_connector(server_url: &str) -> YouTubeConnector {
    YouTubeConnector::with_endpoints(
        ClientCredentials::from_parts(Some("cid"), Some("csecret")),
        Endpoints {
            auth_url: format!("{server_url}/auth"),
            token_url: format!("{server_url}/token"),
            revoke_url: format!("{server_url}/revoke"),
            api_base: server_url.into(),
            analytics_base: server_url.into(),
            web_base: server_url.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};

    fn ctx(token: &str) -> AccountCtx {
        AccountCtx::unmetered(
            "",
            "UCgh0st",
            ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken(token.into()))),
        )
    }

    #[test]
    fn unconfigured_without_client_id() {
        let yt = YouTubeConnector::new(None);
        assert!(yt.oauth().is_none());
        assert!(!yt.info().auth_configured);
    }

    #[test]
    fn requests_only_read_scopes_on_connect() {
        let yt = test_connector("http://x");
        let scopes = &yt.oauth().unwrap().base_scopes;
        assert!(scopes.iter().all(|s| s.ends_with("readonly")));
        assert!(!scopes
            .iter()
            .any(|s| s == SCOPE_UPLOAD || s == SCOPE_FORCE_SSL));
    }

    #[tokio::test]
    async fn fetches_profile() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("mine".into(), "true".into()),
                mockito::Matcher::UrlEncoded("part".into(), "snippet".into()),
            ]))
            .match_header("authorization", "Bearer tok")
            .with_body(
                r#"{"items":[{"id":"UC123","snippet":{"title":"gh0stymc","customUrl":"@gh0stymc",
                   "thumbnails":{"default":{"url":"https://yt3.example/a.jpg"}}}}]}"#,
            )
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let p = yt.fetch_profile(&ctx("tok")).await.unwrap();
        assert_eq!(p.external_id, "UC123");
        assert_eq!(p.display_name, "gh0stymc");
        assert_eq!(p.handle.as_deref(), Some("@gh0stymc"));
        assert_eq!(p.avatar_url.as_deref(), Some("https://yt3.example/a.jpg"));
    }

    #[tokio::test]
    async fn account_without_channel() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"kind":"youtube#channelListResponse","pageInfo":{"totalResults":0}}"#)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        assert!(matches!(
            yt.fetch_profile(&ctx("tok")).await,
            Err(AppError::NoChannel)
        ));
    }

    #[tokio::test]
    async fn snapshot_reads_totals_and_hides_hidden_subscribers() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(mockito::Matcher::UrlEncoded(
                "part".into(),
                "snippet,statistics".into(),
            ))
            .with_body(
                r#"{"items":[{"id":"UCgh0st","snippet":{"title":"g","publishedAt":"2020-05-01T00:00:00Z"},
                   "statistics":{"viewCount":"1000","subscriberCount":"120","videoCount":"12",
                   "hiddenSubscriberCount":false}}]}"#,
            )
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let s = yt.fetch_channel_snapshot(&ctx("tok")).await.unwrap();
        assert_eq!(s.created_at.as_deref(), Some("2020-05-01T00:00:00Z"));
        assert_eq!(
            s.totals,
            vec![
                MetricValue::new("followers", 120.0),
                MetricValue::new("views_total", 1000.0),
                MetricValue::new("contents_total", 12.0),
            ]
        );
    }

    #[tokio::test]
    async fn snapshot_of_another_channel_is_rejected() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"items":[{"id":"UCother","snippet":{"title":"x"}}]}"#)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        assert!(matches!(
            yt.fetch_channel_snapshot(&ctx("tok")).await,
            Err(AppError::NotConnected)
        ));
    }

    #[tokio::test]
    async fn lists_uploads_with_page_token() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/playlistItems")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("playlistId".into(), "UUgh0st".into()),
                mockito::Matcher::UrlEncoded("pageToken".into(), "p2".into()),
            ]))
            .with_body(r#"{"items":[{"contentDetails":{"videoId":"v3"}}]}"#)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let p = yt
            .list_contents(&ctx("tok"), Some("p2".into()))
            .await
            .unwrap();
        assert_eq!(
            p.items,
            vec![ContentRef {
                external_id: "v3".into()
            }]
        );
        assert!(p.next.is_none());
    }

    #[tokio::test]
    async fn channel_without_uploads_lists_nothing() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/playlistItems")
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let p = yt.list_contents(&ctx("tok"), None).await.unwrap();
        assert!(p.items.is_empty());
    }

    #[tokio::test]
    async fn video_details_are_batched_by_50_and_metered() {
        use crate::quota::QuotaGate;
        use std::sync::atomic::{AtomicU32, Ordering};
        struct Count(AtomicU32);
        impl QuotaGate for Count {
            fn spend(&self, units: u32) -> Result<()> {
                self.0.fetch_add(units, Ordering::SeqCst);
                Ok(())
            }
        }
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("GET", "/youtube/v3/videos")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"items":[{"id":"x"}]}"#)
            .expect(3)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let count = Arc::new(Count(AtomicU32::new(0)));
        let mut c = ctx("tok");
        c.quota = count.clone();
        let ids: Vec<String> = (0..120).map(|i| format!("v{i}")).collect();
        let out = yt.fetch_content_details(&c, &ids).await.unwrap();
        m.assert_async().await;
        assert_eq!(out.len(), 3);
        assert_eq!(count.0.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhausted_quota_makes_no_request() {
        struct Empty;
        impl crate::quota::QuotaGate for Empty {
            fn spend(&self, _: u32) -> Result<()> {
                Err(AppError::QuotaExceeded)
            }
        }
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("GET", "/youtube/v3/channels")
            .match_query(mockito::Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let mut c = ctx("tok");
        c.quota = Arc::new(Empty);
        assert!(matches!(
            yt.fetch_channel_snapshot(&c).await,
            Err(AppError::QuotaExceeded)
        ));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn analytics_history_is_fetched_per_year() {
        let mut srv = mockito::Server::new_async().await;
        let m = srv
            .mock("GET", "/v2/reports")
            .match_query(mockito::Matcher::UrlEncoded(
                "ids".into(),
                "channel==MINE".into(),
            ))
            .with_body(
                r#"{"columnHeaders":[{"name":"day"},{"name":"views"}],"rows":[["2025-01-01",5]]}"#,
            )
            .expect(2)
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let pts = yt
            .fetch_channel_metrics(
                &ctx("tok"),
                DateRange {
                    from: "2025-01-01".into(),
                    to: "2026-01-10".into(),
                },
            )
            .await
            .unwrap();
        m.assert_async().await;
        assert_eq!(pts.len(), 2);
    }
}
