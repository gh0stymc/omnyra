//! Twitch connector (Helix API).
//!
//! - **Auth:** the app is registered as a *public* client, so the only allowed flow is the
//!   device code grant (no client secret; single-use refresh tokens that expire after 30
//!   days without use). Every API call also needs the `Client-Id` header.
//! - **Data:** followers (total, plus the follow date of each current follower, which
//!   gives a follower history), VODs and clips with their views, and the live stream
//!   right now. Helix has no viewer history: average and peak viewers only exist for the
//!   streams Omnyra saw live (`live.rs`, A2). Twitch has no daily request quota, only a
//!   per-minute rate limit, so nothing is charged to the quota ledger.

use super::*;
use crate::auth::ClientCredentials;
use serde::Deserialize;
use std::collections::BTreeMap;

pub const SCOPE_READ_FOLLOWERS: &str = "moderator:read:followers";

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
const REVOKE_URL: &str = "https://id.twitch.tv/oauth2/revoke";
const API_BASE: &str = "https://api.twitch.tv";
/// Helix page size limit.
const PAGE: &str = "100";
/// Follower list pages read for the history (100 each): enough for 20,000 followers.
const MAX_FOLLOWER_PAGES: usize = 200;

pub struct Endpoints {
    pub token_url: String,
    pub device_url: String,
    pub revoke_url: String,
    pub api_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            token_url: TOKEN_URL.into(),
            device_url: DEVICE_URL.into(),
            revoke_url: REVOKE_URL.into(),
            api_base: API_BASE.into(),
        }
    }
}

pub struct TwitchConnector {
    info: PlatformInfo,
    oauth: Option<Arc<OAuthConfig>>,
    api_base: String,
}

impl TwitchConnector {
    pub fn new(credentials: Option<ClientCredentials>) -> Self {
        Self::with_endpoints(credentials, Endpoints::default())
    }

    pub fn with_endpoints(credentials: Option<ClientCredentials>, ep: Endpoints) -> Self {
        let oauth = credentials.map(|c| {
            Arc::new(OAuthConfig {
                auth_url: String::new(),
                token_url: ep.token_url,
                revoke_url: Some(ep.revoke_url),
                client_id: c.client_id,
                // Public client: never a secret.
                client_secret: None,
                base_scopes: vec![SCOPE_READ_FOLLOWERS.into()],
                extra_auth_params: Vec::new(),
                device_url: Some(ep.device_url),
                revoke_with_client_id: true,
                ..Default::default()
            })
        });
        Self {
            info: PlatformInfo {
                id: "twitch".into(),
                display_name: "Twitch".into(),
                brand_color: "#9146ff".into(),
                capabilities: Capabilities {
                    analytics: true,
                    // Follower history comes from follow dates; no other history exists.
                    historic_analytics: true,
                    upload: UploadSupport::None,
                    resumable_upload: false,
                    schedule: ScheduleSupport::None,
                    comments: CommentSupport::None,
                    live: true,
                    public_lookup: false,
                    limits: MediaLimits {
                        max_duration_s: None,
                        max_size_mb: None,
                        max_title_len: Some(140),
                        max_description_len: None,
                        max_hashtags: None,
                    },
                },
                auth: AuthKind::DeviceCode,
                auth_configured: oauth.is_some(),
            },
            oauth,
            api_base: ep.api_base,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/helix/{path}", self.api_base)
    }
}

// ---- Helix responses -----------------------------------------------------------------

#[derive(Deserialize)]
struct Data<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    #[serde(default)]
    pagination: Pagination,
    total: Option<u64>,
}

#[derive(Deserialize, Default)]
struct Pagination {
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct User {
    id: String,
    login: String,
    display_name: String,
    profile_image_url: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct Follower {
    followed_at: String,
}

#[derive(Deserialize)]
struct Stream {
    id: String,
    started_at: String,
    viewer_count: u32,
    title: Option<String>,
    game_name: Option<String>,
}

#[derive(Deserialize)]
struct Video {
    id: String,
    stream_id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    created_at: Option<String>,
    url: Option<String>,
    thumbnail_url: Option<String>,
    viewable: Option<String>,
    view_count: Option<f64>,
    #[serde(rename = "type")]
    kind: Option<String>,
    duration: Option<String>,
}

#[derive(Deserialize)]
struct Clip {
    id: String,
    url: Option<String>,
    title: Option<String>,
    view_count: Option<f64>,
    created_at: Option<String>,
    thumbnail_url: Option<String>,
    duration: Option<f64>,
}

/// Twitch VOD durations: `3h2m1s`, `45m10s`, `30s`.
pub fn parse_duration(s: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u32 = num.parse().ok()?;
        total = total.checked_add(match c {
            'h' => n.checked_mul(3600)?,
            'm' => n.checked_mul(60)?,
            's' => n,
            _ => return None,
        })?;
        num.clear();
    }
    (num.is_empty() && !s.is_empty()).then_some(total)
}

/// VOD thumbnails are templates (`%{width}x%{height}`).
fn thumb(url: Option<String>) -> Option<String> {
    url.filter(|u| !u.is_empty())
        .map(|u| u.replace("%{width}", "320").replace("%{height}", "180"))
}

fn video_details(v: Video) -> ContentDetails {
    let mut extra = serde_json::Map::new();
    if let Some(sid) = &v.stream_id {
        extra.insert("stream_id".into(), sid.clone().into());
    }
    if let Some(t) = &v.kind {
        extra.insert("twitch_type".into(), t.clone().into());
    }
    ContentDetails {
        content: Content {
            kind: match v.kind.as_deref() {
                Some("upload") => kind::VIDEO,
                _ => kind::VOD,
            }
            .into(),
            title: v.title,
            description: v.description.filter(|d| !d.is_empty()),
            url: v.url,
            thumbnail_url: thumb(v.thumbnail_url),
            published_at: v.created_at,
            duration_s: v.duration.as_deref().and_then(parse_duration),
            visibility: v.viewable,
            external_id: v.id,
            extra,
        },
        kind_final: true,
        stats: v
            .view_count
            .map(|n| vec![MetricValue::new("views", n)])
            .unwrap_or_default(),
    }
}

fn clip_details(c: Clip) -> ContentDetails {
    ContentDetails {
        content: Content {
            external_id: c.id,
            kind: kind::CLIP.into(),
            title: c.title,
            description: None,
            url: c.url,
            thumbnail_url: thumb(c.thumbnail_url),
            published_at: c.created_at,
            duration_s: c.duration.map(|d| d.round() as u32),
            visibility: Some("public".into()),
            extra: Default::default(),
        },
        kind_final: true,
        stats: c
            .view_count
            .map(|n| vec![MetricValue::new("views", n)])
            .unwrap_or_default(),
    }
}

/// Video IDs are numeric; clip IDs are slugs.
fn is_video_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

impl TwitchConnector {
    async fn me(&self, ctx: &AccountCtx) -> Result<User> {
        let r: Data<User> = ctx.api.get_json(&self.url("users"), &[]).await?;
        r.data.into_iter().next().ok_or(AppError::NotFound)
    }

    async fn by_ids<T: serde::de::DeserializeOwned>(
        &self,
        ctx: &AccountCtx,
        path: &str,
        ids: &[&String],
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(100) {
            let mut q: Vec<(&str, &str)> = chunk.iter().map(|id| ("id", id.as_str())).collect();
            q.push(("first", PAGE));
            let r: Data<T> = ctx.api.get_json(&self.url(path), &q).await?;
            out.extend(r.data);
        }
        Ok(out)
    }
}

#[async_trait]
impl Connector for TwitchConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        self.oauth.as_ref()
    }

    fn api_headers(&self) -> Vec<(String, String)> {
        self.oauth
            .as_ref()
            .map(|c| vec![("Client-Id".into(), c.client_id.clone())])
            .unwrap_or_default()
    }

    /// VODs first, then clips (sorted by views): not newest first.
    fn lists_newest_first(&self) -> bool {
        false
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        let u = self.me(ctx).await?;
        Ok(Profile {
            external_id: u.id,
            handle: Some(u.login),
            display_name: u.display_name,
            avatar_url: u.profile_image_url,
        })
    }

    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot> {
        let u = self.me(ctx).await?;
        if !ctx.external_id.is_empty() && u.id != ctx.external_id {
            tracing::warn!("token belongs to a different Twitch user than the stored account");
            return Err(AppError::NotConnected);
        }
        let f: Data<Follower> = ctx
            .api
            .get_json(
                &self.url("channels/followers"),
                &[("broadcaster_id", u.id.as_str()), ("first", "1")],
            )
            .await?;
        Ok(ChannelSnapshot {
            created_at: u.created_at.clone(),
            totals: f
                .total
                .map(|t| vec![MetricValue::new("followers", t as f64)])
                .unwrap_or_default(),
            profile: Profile {
                external_id: u.id,
                handle: Some(u.login),
                display_name: u.display_name,
                avatar_url: u.profile_image_url,
            },
        })
    }

    /// New follows per day, from the follow date of each *current* follower (people who
    /// unfollowed are gone from the list, so the past is slightly underestimated).
    async fn fetch_channel_metrics(
        &self,
        ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        let mut per_day: BTreeMap<String, f64> = BTreeMap::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_FOLLOWER_PAGES {
            let mut q = vec![
                ("broadcaster_id", ctx.external_id.as_str()),
                ("first", PAGE),
            ];
            if let Some(c) = cursor.as_deref() {
                q.push(("after", c));
            }
            let r: Result<Data<Follower>> =
                ctx.api.get_json(&self.url("channels/followers"), &q).await;
            let page = match r {
                Ok(p) => p,
                // Without the scope only the total is visible: no history, not an error.
                Err(AppError::ScopesDenied) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            for f in page.data {
                let day = f.followed_at.get(..10).unwrap_or_default().to_owned();
                if day >= range.from && day <= range.to {
                    *per_day.entry(day).or_default() += 1.0;
                }
            }
            match page.pagination.cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }
        Ok(per_day
            .into_iter()
            .map(|(day, value)| MetricPoint {
                day,
                metric: "subscribers_gained".into(),
                value,
            })
            .collect())
    }

    /// Page tokens: `v:<cursor>` for videos, `c:<cursor>` (or `c:`) for clips.
    async fn list_contents(
        &self,
        ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>> {
        let (clips, cursor) = match page.as_deref() {
            None => (false, None),
            Some(p) => match p.split_once(':') {
                Some(("c", c)) => (true, Some(c.to_owned()).filter(|c| !c.is_empty())),
                Some((_, c)) => (false, Some(c.to_owned())),
                None => (false, None),
            },
        };
        let mut q = vec![("first", PAGE)];
        let id = ctx.external_id.as_str();
        q.push(if clips {
            ("broadcaster_id", id)
        } else {
            ("user_id", id)
        });
        if let Some(c) = cursor.as_deref() {
            q.push(("after", c));
        }
        #[derive(Deserialize)]
        struct Id {
            id: String,
        }
        let path = if clips { "clips" } else { "videos" };
        let r: Data<Id> = ctx.api.get_json(&self.url(path), &q).await?;
        let next = match (clips, r.pagination.cursor.filter(|c| !c.is_empty())) {
            (false, Some(c)) => Some(format!("v:{c}")),
            (false, None) => Some("c:".into()),
            (true, Some(c)) => Some(format!("c:{c}")),
            (true, None) => None,
        };
        Ok(Page {
            items: r
                .data
                .into_iter()
                .map(|i| ContentRef { external_id: i.id })
                .collect(),
            next,
        })
    }

    async fn fetch_content_details(
        &self,
        ctx: &AccountCtx,
        ids: &[String],
    ) -> Result<Vec<ContentDetails>> {
        let (videos, clips): (Vec<&String>, Vec<&String>) =
            ids.iter().partition(|id| is_video_id(id));
        let mut out: Vec<ContentDetails> = self
            .by_ids::<Video>(ctx, "videos", &videos)
            .await?
            .into_iter()
            .map(video_details)
            .collect();
        out.extend(
            self.by_ids::<Clip>(ctx, "clips", &clips)
                .await?
                .into_iter()
                .map(clip_details),
        );
        Ok(out)
    }

    async fn live_status(&self, ctx: &AccountCtx) -> Result<Option<LiveStatus>> {
        let r: Data<Stream> = ctx
            .api
            .get_json(
                &self.url("streams"),
                &[("user_id", ctx.external_id.as_str()), ("type", "live")],
            )
            .await?;
        Ok(r.data.into_iter().next().map(|s| LiveStatus {
            stream_id: s.id,
            started_at: s.started_at,
            viewers: s.viewer_count,
            title: s.title,
            category: s.game_name.filter(|g| !g.is_empty()),
        }))
    }
}

#[cfg(test)]
pub(crate) fn test_connector(server_url: &str) -> TwitchConnector {
    TwitchConnector::with_endpoints(
        ClientCredentials::from_parts(Some("tw-cid"), None),
        Endpoints {
            token_url: format!("{server_url}/oauth2/token"),
            device_url: format!("{server_url}/oauth2/device"),
            revoke_url: format!("{server_url}/oauth2/revoke"),
            api_base: server_url.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};
    use mockito::Matcher;

    fn ctx(c: &TwitchConnector) -> AccountCtx {
        AccountCtx::unmetered(
            "acc",
            "1234",
            ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("tok".into())))
                .with_headers(c.api_headers()),
        )
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("3h2m1s"), Some(10_921));
        assert_eq!(parse_duration("45m10s"), Some(2_710));
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("5x"), None);
    }

    #[test]
    fn public_client_with_device_flow_and_no_upload() {
        let c = TwitchConnector::new(ClientCredentials::from_parts(Some("x"), None));
        let o = c.oauth().unwrap();
        assert!(o.client_secret.is_none());
        assert!(o.device_url.is_some());
        assert_eq!(c.info().capabilities.upload, UploadSupport::None);
        assert_eq!(c.info().auth, AuthKind::DeviceCode);
        assert!(!TwitchConnector::new(None).info().auth_configured);
    }

    #[tokio::test]
    async fn snapshot_sends_client_id_and_reads_follower_total() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/helix/users")
            .match_header("client-id", "tw-cid")
            .match_header("authorization", "Bearer tok")
            .with_body(r#"{"data":[{"id":"1234","login":"gh0sty","display_name":"Gh0sty","created_at":"2019-01-01T00:00:00Z"}]}"#)
            .create_async()
            .await;
        srv.mock("GET", "/helix/channels/followers")
            .match_query(Matcher::UrlEncoded("broadcaster_id".into(), "1234".into()))
            .with_body(r#"{"total":321,"data":[],"pagination":{}}"#)
            .create_async()
            .await;
        let c = test_connector(&srv.url());
        let s = c.fetch_channel_snapshot(&ctx(&c)).await.unwrap();
        assert_eq!(s.profile.handle.as_deref(), Some("gh0sty"));
        assert_eq!(s.totals, vec![MetricValue::new("followers", 321.0)]);
    }

    #[tokio::test]
    async fn follower_history_counts_follows_per_day_across_pages() {
        let mut srv = mockito::Server::new_async().await;
        // First page (mockito tries mocks in creation order until each is used up).
        srv.mock("GET", "/helix/channels/followers")
            .match_query(Matcher::UrlEncoded("first".into(), "100".into()))
            .with_body(r#"{"total":3,"data":[{"followed_at":"2026-11-02T08:00:00Z"},{"followed_at":"2026-11-01T09:00:00Z"},{"followed_at":"2020-01-01T00:00:00Z"}],"pagination":{"cursor":"p2"}}"#)
            .expect(1)
            .create_async()
            .await;
        srv.mock("GET", "/helix/channels/followers")
            .match_query(Matcher::UrlEncoded("after".into(), "p2".into()))
            .with_body(
                r#"{"total":3,"data":[{"followed_at":"2026-11-01T08:00:00Z"}],"pagination":{}}"#,
            )
            .create_async()
            .await;
        let c = test_connector(&srv.url());
        let pts = c
            .fetch_channel_metrics(
                &ctx(&c),
                DateRange {
                    from: "2026-01-01".into(),
                    to: "2026-11-17".into(),
                },
            )
            .await
            .unwrap();
        let got: Vec<(&str, f64)> = pts.iter().map(|p| (p.day.as_str(), p.value)).collect();
        assert_eq!(got, vec![("2026-11-01", 2.0), ("2026-11-02", 1.0)]);
    }

    #[tokio::test]
    async fn lists_videos_then_clips() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/helix/videos")
            .match_query(Matcher::UrlEncoded("user_id".into(), "1234".into()))
            .with_body(r#"{"data":[{"id":"111"}],"pagination":{}}"#)
            .create_async()
            .await;
        srv.mock("GET", "/helix/clips")
            .match_query(Matcher::UrlEncoded("broadcaster_id".into(), "1234".into()))
            .with_body(r#"{"data":[{"id":"FunnyClipSlug"}],"pagination":{}}"#)
            .create_async()
            .await;
        let c = test_connector(&srv.url());
        let p1 = c.list_contents(&ctx(&c), None).await.unwrap();
        assert_eq!(p1.items[0].external_id, "111");
        assert_eq!(p1.next.as_deref(), Some("c:"));
        let p2 = c.list_contents(&ctx(&c), p1.next).await.unwrap();
        assert_eq!(p2.items[0].external_id, "FunnyClipSlug");
        assert!(p2.next.is_none());
    }

    #[tokio::test]
    async fn details_split_videos_and_clips() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/helix/videos")
            .match_query(Matcher::UrlEncoded("id".into(), "111".into()))
            .with_body(r#"{"data":[{"id":"111","stream_id":"s9","type":"archive","title":"Directo","created_at":"2026-11-10T18:00:00Z","duration":"2h30m0s","view_count":40,"viewable":"public","thumbnail_url":"https://x/%{width}x%{height}.jpg"}]}"#)
            .create_async()
            .await;
        srv.mock("GET", "/helix/clips")
            .match_query(Matcher::UrlEncoded("id".into(), "Slug".into()))
            .with_body(r#"{"data":[{"id":"Slug","title":"clip","view_count":7,"duration":29.6,"created_at":"2026-11-10T19:00:00Z"}]}"#)
            .create_async()
            .await;
        let c = test_connector(&srv.url());
        let d = c
            .fetch_content_details(&ctx(&c), &["111".into(), "Slug".into()])
            .await
            .unwrap();
        assert_eq!(d.len(), 2);
        let vod = &d[0].content;
        assert_eq!((vod.kind.as_str(), vod.duration_s), ("vod", Some(9000)));
        assert_eq!(vod.extra["stream_id"], "s9");
        assert_eq!(vod.thumbnail_url.as_deref(), Some("https://x/320x180.jpg"));
        assert_eq!(d[1].content.kind, "clip");
        assert_eq!(d[1].content.duration_s, Some(30));
        assert_eq!(d[1].stats, vec![MetricValue::new("views", 7.0)]);
    }

    #[tokio::test]
    async fn live_status_now() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/helix/streams")
            .match_query(Matcher::Any)
            .with_body(r#"{"data":[{"id":"s9","started_at":"2026-11-17T18:00:00Z","viewer_count":12,"title":"t","game_name":"Minecraft"}]}"#)
            .create_async()
            .await;
        let c = test_connector(&srv.url());
        let s = c.live_status(&ctx(&c)).await.unwrap().unwrap();
        assert_eq!((s.stream_id.as_str(), s.viewers), ("s9", 12));
        assert_eq!(s.category.as_deref(), Some("Minecraft"));
    }
}
