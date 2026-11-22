//! Instagram connector ("Instagram API with Instagram Login", professional accounts).
//!
//! - **Auth:** authorization code without PKCE; the `redirect_uri` must be HTTPS, so it is
//!   the token broker's callback, which forwards the code to our loopback; the exchange
//!   (short → long-lived token) and the refresh go through the broker, which holds the
//!   client secret (docs/AUTH_STRATEGY.md). The long-lived token doubles as refresh token.
//! - **Data:** followers and media count; daily `reach` and new followers from account
//!   insights (the API only serves the last 30 days of those, so the history builds up
//!   while Omnyra is used); media (Reels, posts) with likes, comments and views.
//! - **Publishing:** Reels with the resumable `rupload` protocol (no public URL needed).
//!   Instagram has no private or unlisted posts: everything published is public.
//! - No daily quota is metered (Instagram limits calls per hour per account).

use super::rules::{self, ShortFormRules};
use super::*;
use crate::auth::ClientCredentials;
use serde::Deserialize;
use serde_json::Value;

pub const SCOPE_BASIC: &str = "instagram_business_basic";
pub const SCOPE_INSIGHTS: &str = "instagram_business_manage_insights";
pub const SCOPE_PUBLISH: &str = "instagram_business_content_publish";
pub const SCOPE_COMMENTS: &str = "instagram_business_manage_comments";

/// Graph API version [VERIFICAR when the app is created].
pub const GRAPH_VERSION: &str = "v23.0";
const AUTH_URL: &str = "https://www.instagram.com/oauth/authorize";
const GRAPH: &str = "https://graph.instagram.com";
const RUPLOAD: &str = "https://rupload.facebook.com";
/// Account insights only cover the last 30 days.
const INSIGHTS_WINDOW_DAYS: i64 = 29;
/// How long we wait for Instagram to process an uploaded Reel.
const PROCESSING_POLLS: u32 = 120;

const RULES: ShortFormRules = ShortFormRules {
    extensions: &["mp4", "mov"],
    // [VERIFICAR] Reels via API: up to 300 MB, 3 s to 15 min.
    max_size_bytes: 300 * 1024 * 1024,
    min_duration_s: 3.0,
    max_duration_s: 15.0 * 60.0,
    max_caption_chars: 2200,
    max_hashtags: 30,
};

pub struct Endpoints {
    pub auth_url: String,
    pub graph: String,
    pub rupload: String,
    pub poll_every: std::time::Duration,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth_url: AUTH_URL.into(),
            graph: GRAPH.into(),
            rupload: RUPLOAD.into(),
            poll_every: std::time::Duration::from_secs(5),
        }
    }
}

pub struct InstagramConnector {
    info: PlatformInfo,
    oauth: Option<Arc<OAuthConfig>>,
    ep: Endpoints,
}

impl InstagramConnector {
    /// Configured only with both the app ID and the broker URL.
    pub fn new(credentials: Option<ClientCredentials>, broker: Option<&str>) -> Self {
        Self::with_endpoints(credentials, broker, Endpoints::default())
    }

    pub fn with_endpoints(
        credentials: Option<ClientCredentials>,
        broker: Option<&str>,
        ep: Endpoints,
    ) -> Self {
        let broker = broker.map(|b| b.trim_end_matches('/').to_owned());
        let oauth = credentials.zip(broker).map(|(c, b)| {
            Arc::new(OAuthConfig {
                auth_url: ep.auth_url.clone(),
                token_url: format!("{b}/instagram/token"),
                // Instagram has no revocation endpoint: removed from the app's settings.
                revoke_url: None,
                client_id: c.client_id,
                client_secret: None,
                base_scopes: vec![SCOPE_BASIC.into(), SCOPE_INSIGHTS.into()],
                scope_separator: Some(",".into()),
                redirect_broker: Some(format!("{b}/instagram/callback")),
                ..Default::default()
            })
        });
        Self {
            info: PlatformInfo {
                id: "instagram".into(),
                display_name: "Instagram".into(),
                brand_color: "#e1306c".into(),
                capabilities: Capabilities {
                    analytics: true,
                    historic_analytics: true,
                    upload: UploadSupport::Direct,
                    resumable_upload: true,
                    schedule: ScheduleSupport::LocalOnly,
                    comments: CommentSupport::ReadReply,
                    live: false,
                    public_lookup: false,
                    limits: MediaLimits {
                        max_duration_s: Some(RULES.max_duration_s as u32),
                        max_size_mb: Some((RULES.max_size_bytes / 1024 / 1024) as u32),
                        max_title_len: None,
                        max_description_len: Some(RULES.max_caption_chars as u32),
                        max_hashtags: Some(RULES.max_hashtags as u32),
                    },
                },
                auth: AuthKind::OAuth,
                auth_configured: oauth.is_some(),
            },
            oauth,
            ep,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{GRAPH_VERSION}/{path}", self.ep.graph)
    }
}

// ---- responses -------------------------------------------------------------------------

#[derive(Deserialize)]
struct Me {
    user_id: Option<String>,
    id: String,
    username: Option<String>,
    name: Option<String>,
    profile_picture_url: Option<String>,
    followers_count: Option<f64>,
    media_count: Option<f64>,
}

#[derive(Deserialize)]
struct Paged<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    paging: Option<Paging>,
}

#[derive(Deserialize)]
struct Paging {
    cursors: Option<Cursors>,
    next: Option<String>,
}

#[derive(Deserialize)]
struct Cursors {
    after: Option<String>,
}

#[derive(Deserialize)]
struct Media {
    id: String,
    caption: Option<String>,
    media_type: Option<String>,
    media_product_type: Option<String>,
    permalink: Option<String>,
    thumbnail_url: Option<String>,
    media_url: Option<String>,
    timestamp: Option<String>,
    like_count: Option<f64>,
    comments_count: Option<f64>,
}

#[derive(Deserialize)]
struct Insight {
    name: String,
    #[serde(default)]
    values: Vec<InsightValue>,
    total_value: Option<TotalValue>,
}

#[derive(Deserialize)]
struct InsightValue {
    value: Value,
    end_time: Option<String>,
}

#[derive(Deserialize)]
struct TotalValue {
    value: f64,
}

#[derive(Deserialize)]
struct IgComment {
    id: String,
    text: Option<String>,
    username: Option<String>,
    timestamp: Option<String>,
    like_count: Option<f64>,
    from: Option<From>,
    replies: Option<Paged<IgComment>>,
}

#[derive(Deserialize)]
struct From {
    id: Option<String>,
}

const MEDIA_FIELDS: &str = "id,caption,media_type,media_product_type,permalink,thumbnail_url,media_url,timestamp,like_count,comments_count";
const COMMENT_FIELDS: &str = "id,text,username,timestamp,like_count,from";

fn media_kind(m: &Media) -> &'static str {
    match (m.media_product_type.as_deref(), m.media_type.as_deref()) {
        (Some("REELS"), _) => kind::REEL,
        (Some("STORY"), _) => kind::STORY,
        _ => kind::POST,
    }
}

/// Instagram timestamps are `2026-11-20T18:00:00+0000`: normalise to RFC 3339 UTC.
pub fn normalize_ts(ts: &str) -> String {
    match ts.strip_suffix("+0000") {
        Some(base) => format!("{base}Z"),
        None => ts.to_owned(),
    }
}

/// First line of the caption, as the content's title.
fn title_of(caption: Option<&str>) -> Option<String> {
    let line = caption?.lines().find(|l| !l.trim().is_empty())?.trim();
    Some(line.chars().take(100).collect())
}

fn to_comment(c: IgComment, own_id: &str) -> Comment {
    Comment {
        is_own: c.from.and_then(|f| f.id).as_deref() == Some(own_id),
        external_id: c.id,
        parent_external_id: None,
        content_external_id: None,
        author_name: c.username,
        author_external_id: None,
        text: c.text.unwrap_or_default(),
        published_at: c.timestamp.as_deref().map(normalize_ts),
        like_count: c.like_count.map(|n| n as u32),
    }
}

impl InstagramConnector {
    async fn me(&self, ctx: &AccountCtx) -> Result<Me> {
        ctx.api
            .get_json(
                &self.url("me"),
                &[(
                    "fields",
                    "user_id,id,username,name,profile_picture_url,followers_count,media_count",
                )],
            )
            .await
    }

    async fn media_views(&self, ctx: &AccountCtx, id: &str) -> Option<f64> {
        // `views` exists for Reels and feed videos; other media types reject the metric.
        let r: Paged<Insight> = ctx
            .api
            .get_json(&self.url(&format!("{id}/insights")), &[("metric", "views")])
            .await
            .ok()?;
        let i = r.data.into_iter().find(|i| i.name == "views")?;
        i.total_value
            .map(|t| t.value)
            .or_else(|| i.values.first().and_then(|v| v.value.as_f64()))
    }

    async fn wait_until_ready(&self, ctx: &AccountCtx, container: &str) -> Result<()> {
        #[derive(Deserialize)]
        struct Status {
            status_code: Option<String>,
        }
        for _ in 0..PROCESSING_POLLS {
            let s: Status = ctx
                .api
                .get_json(&self.url(container), &[("fields", "status_code")])
                .await?;
            match s.status_code.as_deref() {
                Some("FINISHED") | Some("PUBLISHED") => return Ok(()),
                Some("ERROR") | Some("EXPIRED") => {
                    tracing::warn!(status = ?s.status_code, "Instagram rejected the video");
                    return Err(AppError::Validation {
                        field: "file".into(),
                    });
                }
                _ => tokio::time::sleep(self.ep.poll_every).await,
            }
        }
        Err(AppError::Network)
    }
}

#[async_trait]
impl Connector for InstagramConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        self.oauth.as_ref()
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        let me = self.me(ctx).await?;
        Ok(Profile {
            external_id: me.user_id.unwrap_or(me.id),
            display_name: me
                .name
                .filter(|n| !n.is_empty())
                .or_else(|| me.username.clone())
                .unwrap_or_default(),
            handle: me.username,
            avatar_url: me.profile_picture_url,
        })
    }

    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot> {
        let me = self.me(ctx).await?;
        let mut totals = Vec::new();
        if let Some(f) = me.followers_count {
            totals.push(MetricValue::new("followers", f));
        }
        if let Some(m) = me.media_count {
            totals.push(MetricValue::new("contents_total", m));
        }
        Ok(ChannelSnapshot {
            created_at: None,
            totals,
            profile: Profile {
                external_id: me.user_id.unwrap_or(me.id),
                display_name: me
                    .name
                    .filter(|n| !n.is_empty())
                    .or_else(|| me.username.clone())
                    .unwrap_or_default(),
                handle: me.username,
                avatar_url: me.profile_picture_url,
            },
        })
    }

    /// Daily `reach` and new followers (`subscribers_gained`) for the last 30 days at most.
    async fn fetch_channel_metrics(
        &self,
        ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        let to = crate::sync::parse_day(&range.to)?;
        let from = crate::sync::parse_day(&range.from)?
            .max(to - time::Duration::days(INSIGHTS_WINDOW_DAYS));
        if from > to {
            return Ok(Vec::new());
        }
        let ts = |d: time::Date| d.midnight().assume_utc().unix_timestamp().to_string();
        let since = ts(from);
        // `until` is exclusive: the end of the last day.
        let until = ts(to + time::Duration::days(1));
        let r: Paged<Insight> = ctx
            .api
            .get_json(
                &self.url(&format!("{}/insights", ctx.external_id)),
                &[
                    ("metric", "reach,follower_count"),
                    ("period", "day"),
                    ("since", &since),
                    ("until", &until),
                ],
            )
            .await?;
        let mut out = Vec::new();
        for i in r.data {
            let metric = match i.name.as_str() {
                "reach" => "reach",
                "follower_count" => "subscribers_gained",
                _ => continue,
            };
            for v in i.values {
                let (Some(end), Some(value)) = (v.end_time, v.value.as_f64()) else {
                    continue;
                };
                // `end_time` is the end of the day (next day 07:00/08:00 UTC, Pacific):
                // the measured day is the one before.
                let Ok(end) = crate::sync::parse_day(end.get(..10).unwrap_or_default()) else {
                    continue;
                };
                out.push(MetricPoint {
                    day: crate::sync::fmt_day(end - time::Duration::days(1)),
                    metric: metric.into(),
                    value,
                });
            }
        }
        Ok(out)
    }

    async fn list_contents(
        &self,
        ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>> {
        let mut q = vec![("fields", "id"), ("limit", "50")];
        if let Some(p) = page.as_deref() {
            q.push(("after", p));
        }
        #[derive(Deserialize)]
        struct Id {
            id: String,
        }
        let r: Paged<Id> = ctx
            .api
            .get_json(&self.url(&format!("{}/media", ctx.external_id)), &q)
            .await?;
        let next = r
            .paging
            .filter(|p| p.next.is_some())
            .and_then(|p| p.cursors)
            .and_then(|c| c.after);
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
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let m: Media = match ctx
                .api
                .get_json(&self.url(id), &[("fields", MEDIA_FIELDS)])
                .await
            {
                Ok(m) => m,
                // Deleted since it was listed.
                Err(AppError::NotFound) => continue,
                Err(e) => return Err(e),
            };
            let k = media_kind(&m);
            let mut stats = Vec::new();
            if let Some(n) = m.like_count {
                stats.push(MetricValue::new("likes", n));
            }
            if let Some(n) = m.comments_count {
                stats.push(MetricValue::new("comments", n));
            }
            if m.media_type.as_deref() == Some("VIDEO") {
                if let Some(v) = self.media_views(ctx, &m.id).await {
                    stats.push(MetricValue::new("views", v));
                }
            }
            let mut extra = serde_json::Map::new();
            if let Some(t) = &m.media_type {
                extra.insert("ig_media_type".into(), t.clone().into());
            }
            out.push(ContentDetails {
                content: Content {
                    kind: k.into(),
                    title: title_of(m.caption.as_deref()),
                    description: m.caption,
                    url: m.permalink,
                    thumbnail_url: m.thumbnail_url.or(m.media_url),
                    published_at: m.timestamp.as_deref().map(normalize_ts),
                    duration_s: None,
                    visibility: Some("public".into()),
                    external_id: m.id,
                    extra,
                },
                kind_final: true,
                stats,
            });
        }
        Ok(out)
    }

    fn upload_scopes(&self, _with_playlist: bool) -> Vec<String> {
        vec![SCOPE_PUBLISH.into()]
    }

    fn comment_scopes(&self) -> Vec<String> {
        vec![SCOPE_COMMENTS.into()]
    }

    fn check_upload(&self, draft: &UploadDraft, facts: &FileFacts) -> Vec<Issue> {
        let mut out = rules::check(&RULES, draft, facts);
        // No private or unlisted posts on Instagram.
        out.push(Issue::warning("visibility", "always_public"));
        out
    }

    async fn upload_options(&self, _ctx: &AccountCtx, _lang: &str) -> Result<UploadOptions> {
        Ok(UploadOptions {
            categories: Vec::new(),
            playlists: Vec::new(),
        })
    }

    /// Reel: container (resumable) → binary to `rupload` → wait for processing → publish.
    /// A restart creates a new container (Instagram containers expire after 24 h anyway).
    async fn upload(
        &self,
        ctx: &AccountCtx,
        req: &UploadRequest,
        _resume: Option<String>,
        sink: &dyn UploadSink,
    ) -> Result<UploadOutcome> {
        #[derive(Deserialize)]
        struct Created {
            id: String,
            uri: Option<String>,
        }
        let caption = rules::caption(&req.draft);
        let created: Created = ctx
            .api
            .send_json(|c| {
                c.post(self.url(&format!("{}/media", ctx.external_id)))
                    .form(&[
                        ("media_type", "REELS"),
                        ("upload_type", "resumable"),
                        ("caption", caption.as_str()),
                    ])
            })
            .await?;
        let upload_uri = created.uri.unwrap_or_else(|| {
            format!(
                "{}/ig-api-upload/{GRAPH_VERSION}/{}",
                self.ep.rupload, created.id
            )
        });
        sink.session_started(&upload_uri).await?;

        let bytes = tokio::fs::read(&req.draft.file_path)
            .await
            .map_err(|_| AppError::FileUnavailable)?;
        let total = bytes.len() as u64;
        if total != req.size_bytes {
            return Err(AppError::FileUnavailable);
        }
        sink.progress(0, total);
        let token = ctx.api.access_token().await?;
        let resp = ctx
            .api
            .http()
            .post(&upload_uri)
            .header("Authorization", format!("OAuth {token}"))
            .header("offset", "0")
            .header("file_size", total.to_string())
            .timeout(std::time::Duration::from_secs(60 * 60))
            .body(bytes)
            .send()
            .await
            .map_err(|_| AppError::Network)?;
        if !resp.status().is_success() {
            return Err(http::error_from(resp).await);
        }
        sink.progress(total, total);

        self.wait_until_ready(ctx, &created.id).await?;
        #[derive(Deserialize)]
        struct Published {
            id: String,
        }
        let published: Published = ctx
            .api
            .send_json(|c| {
                c.post(self.url(&format!("{}/media_publish", ctx.external_id)))
                    .form(&[("creation_id", created.id.as_str())])
            })
            .await?;
        let mut warnings = Vec::new();
        if req.draft.thumbnail_path.is_some() {
            warnings.push("thumbnail_ignored".into());
        }
        Ok(UploadOutcome {
            external_id: published.id,
            warnings,
        })
    }

    async fn list_comments(
        &self,
        ctx: &AccountCtx,
        content: &str,
        page: Option<String>,
    ) -> Result<Page<Comment>> {
        let fields = format!("{COMMENT_FIELDS},replies{{{COMMENT_FIELDS}}}");
        let mut q = vec![("fields", fields.as_str()), ("limit", "50")];
        if let Some(p) = page.as_deref() {
            q.push(("after", p));
        }
        let r: Paged<IgComment> = ctx
            .api
            .get_json(&self.url(&format!("{content}/comments")), &q)
            .await?;
        let next = r
            .paging
            .filter(|p| p.next.is_some())
            .and_then(|p| p.cursors)
            .and_then(|c| c.after);
        let mut items = Vec::new();
        for mut c in r.data {
            let replies = c.replies.take().map(|r| r.data).unwrap_or_default();
            let parent = c.id.clone();
            let mut top = to_comment(c, &ctx.external_id);
            top.content_external_id = Some(content.to_owned());
            items.push(top);
            for r in replies {
                let mut reply = to_comment(r, &ctx.external_id);
                reply.parent_external_id = Some(parent.clone());
                reply.content_external_id = Some(content.to_owned());
                items.push(reply);
            }
        }
        Ok(Page { items, next })
    }

    async fn reply_comment(&self, ctx: &AccountCtx, parent: &str, text: &str) -> Result<Comment> {
        #[derive(Deserialize)]
        struct Id {
            id: String,
        }
        let r: Id = ctx
            .api
            .send_json(|c| {
                c.post(self.url(&format!("{parent}/replies")))
                    .form(&[("message", text)])
            })
            .await?;
        Ok(Comment {
            external_id: r.id,
            parent_external_id: Some(parent.into()),
            content_external_id: None,
            author_name: None,
            author_external_id: None,
            text: text.into(),
            published_at: None,
            like_count: None,
            is_own: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};

    fn connector(srv: &str) -> InstagramConnector {
        InstagramConnector::with_endpoints(
            ClientCredentials::from_parts(Some("igid"), None),
            Some("https://broker.example/"),
            Endpoints {
                auth_url: format!("{srv}/auth"),
                graph: srv.into(),
                rupload: srv.into(),
                poll_every: std::time::Duration::from_millis(1),
            },
        )
    }

    fn ctx() -> AccountCtx {
        AccountCtx::unmetered(
            "acc",
            "1784",
            ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("tok".into())))
                .with_retry_base(std::time::Duration::from_millis(1)),
        )
    }

    #[test]
    fn oauth_goes_through_the_broker_and_needs_both_settings() {
        let c = connector("http://x");
        let cfg = c.oauth().unwrap();
        assert_eq!(cfg.token_url, "https://broker.example/instagram/token");
        assert_eq!(
            cfg.redirect_broker.as_deref(),
            Some("https://broker.example/instagram/callback")
        );
        assert!(cfg.client_secret.is_none());
        assert!(
            !InstagramConnector::new(ClientCredentials::from_parts(Some("id"), None), None)
                .info()
                .auth_configured
        );
    }

    #[test]
    fn timestamps_and_titles() {
        assert_eq!(
            normalize_ts("2026-11-20T18:00:00+0000"),
            "2026-11-20T18:00:00Z"
        );
        assert_eq!(
            title_of(Some("\n  Hola mundo  \nsegunda")),
            Some("Hola mundo".into())
        );
        assert_eq!(title_of(None), None);
    }

    #[tokio::test]
    async fn reads_profile_media_and_insights() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/v23.0/me")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"user_id":"1784","id":"x","username":"gh0sty","name":"","followers_count":120,"media_count":3}"#)
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/1784/media")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"data":[{"id":"m1"},{"id":"m2"}],"paging":{"cursors":{"after":"A"}}}"#)
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/m1")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"id":"m1","caption":"Reel!\n#mc","media_type":"VIDEO","media_product_type":"REELS","timestamp":"2026-11-20T18:00:00+0000","like_count":7,"comments_count":2}"#)
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/m1/insights")
            .match_query(mockito::Matcher::Any)
            .with_body(
                r#"{"data":[{"name":"views","period":"lifetime","values":[{"value":900}]}]}"#,
            )
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/m2")
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_body("{}")
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/1784/insights")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"data":[{"name":"follower_count","values":[{"value":3,"end_time":"2026-11-21T08:00:00+0000"}]},{"name":"reach","values":[{"value":50,"end_time":"2026-11-21T08:00:00+0000"}]}]}"#)
            .create_async()
            .await;
        let c = connector(&srv.url());
        let ctx = ctx();

        let snap = c.fetch_channel_snapshot(&ctx).await.unwrap();
        assert_eq!(snap.profile.external_id, "1784");
        assert_eq!(snap.profile.display_name, "gh0sty");
        assert_eq!(snap.totals[0], MetricValue::new("followers", 120.0));

        let page = c.list_contents(&ctx, None).await.unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.next, None, "no `next` link means last page");

        let d = c
            .fetch_content_details(&ctx, &["m1".into(), "m2".into()])
            .await
            .unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].content.kind, "reel");
        assert_eq!(d[0].content.title.as_deref(), Some("Reel!"));
        assert_eq!(
            d[0].content.published_at.as_deref(),
            Some("2026-11-20T18:00:00Z")
        );
        assert!(d[0].stats.contains(&MetricValue::new("views", 900.0)));

        let m = c
            .fetch_channel_metrics(
                &ctx,
                DateRange {
                    from: "2020-01-01".into(),
                    to: "2026-11-21".into(),
                },
            )
            .await
            .unwrap();
        assert!(m.contains(&MetricPoint {
            day: "2026-11-20".into(),
            metric: "subscribers_gained".into(),
            value: 3.0
        }));
        assert!(m.iter().any(|p| p.metric == "reach" && p.value == 50.0));
    }

    struct Sink;
    #[async_trait]
    impl UploadSink for Sink {
        async fn session_started(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn progress(&self, _: u64, _: u64) {}
    }

    #[tokio::test]
    async fn publishes_a_reel_with_resumable_upload() {
        let dir = std::env::temp_dir().join(format!("omnyra-ig-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("v.mp4");
        std::fs::write(&file, b"videodata").unwrap();
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/v23.0/1784/media")
            .match_body(mockito::Matcher::UrlEncoded(
                "media_type".into(),
                "REELS".into(),
            ))
            .with_body(r#"{"id":"C1"}"#)
            .create_async()
            .await;
        let up = srv
            .mock("POST", "/ig-api-upload/v23.0/C1")
            .match_header("authorization", "OAuth tok")
            .match_header("file_size", "9")
            .match_body("videodata")
            .with_body(r#"{"success":true}"#)
            .create_async()
            .await;
        srv.mock("GET", "/v23.0/C1")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"status_code":"FINISHED"}"#)
            .create_async()
            .await;
        srv.mock("POST", "/v23.0/1784/media_publish")
            .match_body(mockito::Matcher::UrlEncoded(
                "creation_id".into(),
                "C1".into(),
            ))
            .with_body(r#"{"id":"M9"}"#)
            .create_async()
            .await;
        let c = connector(&srv.url());
        let req = UploadRequest {
            draft: UploadDraft {
                file_path: file.to_string_lossy().into(),
                title: "Hola".into(),
                description: String::new(),
                tags: vec![],
                visibility: "public".into(),
                category_id: None,
                playlist_id: None,
                thumbnail_path: None,
                made_for_kids: false,
            },
            size_bytes: 9,
            mime: "video/mp4".into(),
        };
        let out = c.upload(&ctx(), &req, None, &Sink).await.unwrap();
        assert_eq!(out.external_id, "M9");
        up.assert_async().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
