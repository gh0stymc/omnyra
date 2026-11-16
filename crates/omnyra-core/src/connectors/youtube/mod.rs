//! YouTube connector. Step 1a only needs the OAuth description and `fetch_profile` (to
//! identify the channel when connecting). Metrics, contents and snapshots: step 1b.

use super::*;
use crate::auth::ClientCredentials;
use serde::Deserialize;

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

pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    pub revoke_url: String,
    pub api_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth_url: GOOGLE_AUTH_URL.into(),
            token_url: GOOGLE_TOKEN_URL.into(),
            revoke_url: GOOGLE_REVOKE_URL.into(),
            api_base: API_BASE.into(),
        }
    }
}

pub struct YouTubeConnector {
    info: PlatformInfo,
    oauth: Option<Arc<OAuthConfig>>,
    api_base: String,
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
        }
    }
}

#[derive(Deserialize)]
struct ChannelList {
    #[serde(default)]
    items: Vec<Channel>,
}

#[derive(Deserialize)]
struct Channel {
    id: String,
    snippet: Snippet,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snippet {
    title: String,
    custom_url: Option<String>,
    thumbnails: Option<Thumbnails>,
}

#[derive(Deserialize)]
struct Thumbnails {
    default: Option<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

#[async_trait]
impl Connector for YouTubeConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        self.oauth.as_ref()
    }

    /// `channels.list?mine=true` (1 quota unit). The channel is the one picked on Google's
    /// consent screen (personal or brand account).
    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        let list: ChannelList = ctx
            .api
            .get_json(
                &format!("{}/youtube/v3/channels", self.api_base),
                &[("part", "snippet"), ("mine", "true"), ("maxResults", "1")],
            )
            .await?;
        let ch = list.items.into_iter().next().ok_or(AppError::NoChannel)?;
        Ok(Profile {
            external_id: ch.id,
            handle: ch.snippet.custom_url.map(|h| {
                if h.starts_with('@') {
                    h
                } else {
                    format!("@{h}")
                }
            }),
            display_name: ch.snippet.title,
            avatar_url: ch.snippet.thumbnails.and_then(|t| t.default).map(|t| t.url),
        })
    }

    async fn fetch_channel_metrics(
        &self,
        _ctx: &AccountCtx,
        _range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        // Step 1b.
        Err(AppError::NotSupported)
    }

    async fn list_contents(
        &self,
        _ctx: &AccountCtx,
        _page: Option<String>,
    ) -> Result<Page<Content>> {
        // Step 1b.
        Err(AppError::NotSupported)
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
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};

    fn ctx(token: &str) -> AccountCtx {
        AccountCtx {
            account_id: String::new(),
            external_id: String::new(),
            api: ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken(token.into()))),
        }
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
}
