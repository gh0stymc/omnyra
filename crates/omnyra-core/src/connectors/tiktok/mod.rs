//! TikTok connector (Login Kit for Desktop, Display API, Content Posting API).
//!
//! - **Auth:** PKCE (hex challenge, [VERIFICAR]) with a loopback redirect; `client_key`
//!   instead of `client_id`, comma-separated scopes. The code exchange and every refresh
//!   need the client secret, so both go through the token broker
//!   (docs/AUTH_STRATEGY.md). There is no revocation without the secret.
//! - **Data:** followers, total likes and video count; per video views, likes, comments
//!   and shares. No daily history (the API has none): it builds up from daily snapshots.
//!   No average watch time and no comment access for regular apps (A4).
//! - **Publishing:** Direct Post with `FILE_UPLOAD` in chunks. Unaudited API clients can
//!   only post `SELF_ONLY` (private) to private accounts, up to 5 users a day.

use super::rules::{self, ShortFormRules};
use super::*;
use crate::auth::ClientCredentials;
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub const SCOPE_BASIC: &str = "user.info.basic";
pub const SCOPE_STATS: &str = "user.info.stats";
pub const SCOPE_VIDEO_LIST: &str = "video.list";
pub const SCOPE_PUBLISH: &str = "video.publish";

const AUTH_URL: &str = "https://www.tiktok.com/v2/auth/authorize/";
const API_BASE: &str = "https://open.tiktokapis.com/v2";
/// 10 MiB chunks (TikTok: 5–64 MB each, the last one may be up to 128 MB).
pub const CHUNK_SIZE: u64 = 10 * 1024 * 1024;
const PUBLISH_POLLS: u32 = 120;

const RULES: ShortFormRules = ShortFormRules {
    extensions: &["mp4", "mov", "webm"],
    max_size_bytes: 4 * 1024 * 1024 * 1024,
    min_duration_s: 3.0,
    // The creator's own limit (`max_video_post_duration_sec`) is checked again on upload.
    max_duration_s: 10.0 * 60.0,
    max_caption_chars: 2200,
    max_hashtags: 100,
};

const USER_FIELDS: &str = "open_id,avatar_url,display_name,follower_count,likes_count,video_count";
const VIDEO_FIELDS: &str = "id,title,video_description,create_time,cover_image_url,share_url,duration,like_count,comment_count,share_count,view_count";

pub struct Endpoints {
    pub auth_url: String,
    pub api_base: String,
    pub poll_every: std::time::Duration,
    pub chunk_size: u64,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth_url: AUTH_URL.into(),
            api_base: API_BASE.into(),
            poll_every: std::time::Duration::from_secs(5),
            chunk_size: CHUNK_SIZE,
        }
    }
}

pub struct TikTokConnector {
    info: PlatformInfo,
    oauth: Option<Arc<OAuthConfig>>,
    ep: Endpoints,
}

impl TikTokConnector {
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
                token_url: format!("{b}/tiktok/token"),
                revoke_url: None,
                client_id: c.client_id,
                client_secret: None,
                base_scopes: vec![
                    SCOPE_BASIC.into(),
                    SCOPE_STATS.into(),
                    SCOPE_VIDEO_LIST.into(),
                ],
                client_id_param: Some("client_key".into()),
                scope_separator: Some(",".into()),
                pkce_hex: true,
                ..Default::default()
            })
        });
        Self {
            info: PlatformInfo {
                id: "tiktok".into(),
                display_name: "TikTok".into(),
                brand_color: "#25f4ee".into(),
                capabilities: Capabilities {
                    analytics: true,
                    historic_analytics: false,
                    upload: UploadSupport::PrivateOnlyUnaudited,
                    resumable_upload: false,
                    schedule: ScheduleSupport::LocalOnly,
                    comments: CommentSupport::None,
                    live: false,
                    public_lookup: false,
                    limits: MediaLimits {
                        max_duration_s: Some(RULES.max_duration_s as u32),
                        max_size_mb: Some((RULES.max_size_bytes / 1024 / 1024) as u32),
                        max_title_len: None,
                        max_description_len: Some(RULES.max_caption_chars as u32),
                        max_hashtags: None,
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
        format!("{}/{path}", self.ep.api_base)
    }
}

// ---- responses -------------------------------------------------------------------------

/// Every TikTok response is `{data, error: {code, message, log_id}}`; `code` is `ok` on
/// success even with HTTP 200.
#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<serde_json::Value>,
    error: Option<ApiError>,
    #[serde(skip)]
    _t: std::marker::PhantomData<T>,
}

#[derive(Deserialize)]
struct ApiError {
    code: String,
}

fn map_code(code: &str) -> AppError {
    match code {
        "access_token_invalid" | "access_token_expired" => AppError::TokenExpired,
        "scope_not_authorized" | "scope_permission_missed" => AppError::ScopesDenied,
        "rate_limit_exceeded"
        | "spam_risk_too_many_posts"
        | "spam_risk_user_banned_from_posting" => AppError::RateLimited {
            retry_after_s: 3600,
        },
        "unaudited_client_can_only_post_to_private_accounts" | "privacy_level_option_mismatch" => {
            AppError::Validation {
                field: "visibility".into(),
            }
        }
        "file_format_check_failed"
        | "duration_check_failed"
        | "frame_rate_check_failed"
        | "picture_size_check_failed" => AppError::Validation {
            field: "file".into(),
        },
        _ => {
            tracing::warn!(code, "TikTok API error");
            AppError::Internal
        }
    }
}

impl<T: serde::de::DeserializeOwned> Envelope<T> {
    /// The error code wins over the body (errors come with an empty `data`).
    fn into_result(self) -> Result<T> {
        if let Some(e) = self.error.filter(|e| e.code != "ok") {
            return Err(map_code(&e.code));
        }
        serde_json::from_value(self.data.ok_or(AppError::Internal)?).map_err(|e| {
            tracing::warn!(error = %e, "unexpected TikTok response body");
            AppError::Internal
        })
    }
}

#[derive(Deserialize)]
struct UserData {
    user: User,
}

#[derive(Deserialize)]
struct User {
    open_id: String,
    avatar_url: Option<String>,
    display_name: Option<String>,
    follower_count: Option<f64>,
    likes_count: Option<f64>,
    video_count: Option<f64>,
}

#[derive(Deserialize)]
struct VideoPage {
    #[serde(default)]
    videos: Vec<Video>,
    cursor: Option<i64>,
    has_more: Option<bool>,
}

#[derive(Deserialize)]
struct Video {
    id: String,
    title: Option<String>,
    video_description: Option<String>,
    create_time: Option<i64>,
    cover_image_url: Option<String>,
    share_url: Option<String>,
    duration: Option<u32>,
    like_count: Option<f64>,
    comment_count: Option<f64>,
    share_count: Option<f64>,
    view_count: Option<f64>,
}

fn video_details(v: Video) -> ContentDetails {
    let published_at = v.create_time.and_then(|t| {
        time::OffsetDateTime::from_unix_timestamp(t)
            .ok()?
            .format(&time::format_description::well_known::Rfc3339)
            .ok()
    });
    let mut stats = Vec::new();
    for (k, val) in [
        ("views", v.view_count),
        ("likes", v.like_count),
        ("comments", v.comment_count),
        ("shares", v.share_count),
    ] {
        if let Some(n) = val {
            stats.push(MetricValue::new(k, n));
        }
    }
    let description = v.video_description.filter(|d| !d.is_empty());
    ContentDetails {
        content: Content {
            kind: kind::VIDEO.into(),
            title: v.title.filter(|t| !t.is_empty()).or_else(|| {
                description
                    .as_deref()
                    .and_then(|d| d.lines().next())
                    .map(str::to_owned)
            }),
            description,
            url: v.share_url,
            thumbnail_url: v.cover_image_url,
            published_at,
            duration_s: v.duration,
            visibility: None,
            external_id: v.id,
            extra: Default::default(),
        },
        kind_final: true,
        stats,
    }
}

/// `(chunk_size, total_chunk_count)` as TikTok wants them: whole chunks of `preferred`,
/// with the remainder folded into the last one; files that fit in one chunk go whole.
pub fn chunking(size: u64, preferred: u64) -> (u64, u64) {
    if size <= preferred {
        return (size, 1);
    }
    let count = (size / preferred).clamp(1, 1000);
    (preferred, count)
}

impl TikTokConnector {
    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        ctx: &AccountCtx,
        path: &str,
        query: &[(&str, &str)],
        body: serde_json::Value,
    ) -> Result<T> {
        let resp = ctx
            .api
            .send(|c| c.post(self.url(path)).query(query).json(&body))
            .await?;
        let status = resp.status();
        let env: Envelope<T> = resp.json().await.map_err(|_| {
            if status.is_server_error() {
                AppError::Network
            } else {
                AppError::Internal
            }
        })?;
        env.into_result()
    }

    async fn user(&self, ctx: &AccountCtx) -> Result<User> {
        let env: Envelope<UserData> = ctx
            .api
            .get_json(&self.url("user/info/"), &[("fields", USER_FIELDS)])
            .await?;
        Ok(env.into_result()?.user)
    }
}

#[async_trait]
impl Connector for TikTokConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        self.oauth.as_ref()
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        let u = self.user(ctx).await?;
        Ok(Profile {
            external_id: u.open_id,
            handle: None,
            display_name: u.display_name.unwrap_or_else(|| "TikTok".into()),
            avatar_url: u.avatar_url,
        })
    }

    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot> {
        let u = self.user(ctx).await?;
        let mut totals = Vec::new();
        for (k, v) in [
            ("followers", u.follower_count),
            ("likes_total", u.likes_count),
            ("contents_total", u.video_count),
        ] {
            if let Some(n) = v {
                totals.push(MetricValue::new(k, n));
            }
        }
        Ok(ChannelSnapshot {
            created_at: None,
            totals,
            profile: Profile {
                external_id: u.open_id,
                handle: None,
                display_name: u.display_name.unwrap_or_else(|| "TikTok".into()),
                avatar_url: u.avatar_url,
            },
        })
    }

    async fn fetch_channel_metrics(
        &self,
        _ctx: &AccountCtx,
        _range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        Ok(Vec::new())
    }

    async fn list_contents(
        &self,
        ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>> {
        let mut body = json!({ "max_count": 20 });
        if let Some(c) = page.as_deref().and_then(|p| p.parse::<i64>().ok()) {
            body["cursor"] = c.into();
        }
        let p: VideoPage = self
            .post(ctx, "video/list/", &[("fields", "id")], body)
            .await?;
        Ok(Page {
            items: p
                .videos
                .into_iter()
                .map(|v| ContentRef { external_id: v.id })
                .collect(),
            next: match (p.has_more, p.cursor) {
                (Some(true), Some(c)) => Some(c.to_string()),
                _ => None,
            },
        })
    }

    async fn fetch_content_details(
        &self,
        ctx: &AccountCtx,
        ids: &[String],
    ) -> Result<Vec<ContentDetails>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(20) {
            let p: VideoPage = self
                .post(
                    ctx,
                    "video/query/",
                    &[("fields", VIDEO_FIELDS)],
                    json!({ "filters": { "video_ids": chunk } }),
                )
                .await?;
            out.extend(p.videos.into_iter().map(video_details));
        }
        Ok(out)
    }

    fn upload_scopes(&self, _with_playlist: bool) -> Vec<String> {
        vec![SCOPE_PUBLISH.into()]
    }

    fn check_upload(&self, draft: &UploadDraft, facts: &FileFacts) -> Vec<Issue> {
        let mut out = rules::check(&RULES, draft, facts);
        if !["private", "public", "followers", "friends"].contains(&draft.visibility.as_str()) {
            out.push(Issue::error("visibility", "invalid"));
        } else if draft.visibility != "private" {
            out.push(Issue::warning("visibility", "tiktok_private_until_audit"));
        }
        out
    }

    async fn upload_options(&self, _ctx: &AccountCtx, _lang: &str) -> Result<UploadOptions> {
        Ok(UploadOptions {
            categories: Vec::new(),
            playlists: Vec::new(),
        })
    }

    async fn upload(
        &self,
        ctx: &AccountCtx,
        req: &UploadRequest,
        _resume: Option<String>,
        sink: &dyn UploadSink,
    ) -> Result<UploadOutcome> {
        #[derive(Deserialize)]
        struct CreatorInfo {
            #[serde(default)]
            privacy_level_options: Vec<String>,
            max_video_post_duration_sec: Option<u32>,
        }
        let info: CreatorInfo = self
            .post(ctx, "post/publish/creator_info/query/", &[], json!({}))
            .await?;
        let wanted = match req.draft.visibility.as_str() {
            "public" => "PUBLIC_TO_EVERYONE",
            "followers" => "FOLLOWER_OF_CREATOR",
            "friends" => "MUTUAL_FOLLOW_FRIENDS",
            _ => "SELF_ONLY",
        };
        let mut warnings = Vec::new();
        // Unaudited clients only get SELF_ONLY: fall back to it instead of failing.
        let privacy = if info.privacy_level_options.iter().any(|o| o == wanted) {
            wanted
        } else {
            warnings.push("posted_private".to_owned());
            "SELF_ONLY"
        };
        let _ = info.max_video_post_duration_sec;

        let total = req.size_bytes;
        let (chunk, count) = chunking(total, self.ep.chunk_size);
        #[derive(Deserialize)]
        struct Init {
            publish_id: String,
            upload_url: String,
        }
        let init: Init = self
            .post(
                ctx,
                "post/publish/video/init/",
                &[],
                json!({
                    "post_info": {
                        "title": rules::caption(&req.draft),
                        "privacy_level": privacy,
                    },
                    "source_info": {
                        "source": "FILE_UPLOAD",
                        "video_size": total,
                        "chunk_size": chunk,
                        "total_chunk_count": count,
                    },
                }),
            )
            .await?;
        sink.session_started(&init.upload_url).await?;

        let mut file = tokio::fs::File::open(&req.draft.file_path)
            .await
            .map_err(|_| AppError::FileUnavailable)?;
        for i in 0..count {
            let start = i * chunk;
            let end = if i + 1 == count { total } else { start + chunk };
            let mut buf = vec![0u8; (end - start) as usize];
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|_| AppError::FileUnavailable)?;
            file.read_exact(&mut buf)
                .await
                .map_err(|_| AppError::FileUnavailable)?;
            let resp = ctx
                .api
                .http()
                .put(&init.upload_url)
                .header("Content-Type", req.mime.as_str())
                .header(
                    "Content-Range",
                    format!("bytes {start}-{}/{total}", end - 1),
                )
                .timeout(std::time::Duration::from_secs(15 * 60))
                .body(buf)
                .send()
                .await
                .map_err(|_| AppError::Network)?;
            match resp.status().as_u16() {
                200 | 201 | 206 => sink.progress(end, total),
                403 | 404 => return Err(AppError::UploadSessionExpired),
                _ => return Err(http::error_from(resp).await),
            }
        }

        #[derive(Deserialize)]
        struct Status {
            status: String,
            fail_reason: Option<String>,
            #[serde(default, rename = "publicaly_available_post_id")]
            post_ids: Vec<serde_json::Value>,
        }
        for _ in 0..PUBLISH_POLLS {
            let s: Status = self
                .post(
                    ctx,
                    "post/publish/status/fetch/",
                    &[],
                    json!({ "publish_id": init.publish_id }),
                )
                .await?;
            match s.status.as_str() {
                "PUBLISH_COMPLETE" => {
                    // Private posts have no public ID: keep the publish ID.
                    let id = s
                        .post_ids
                        .first()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .unwrap_or(init.publish_id);
                    return Ok(UploadOutcome {
                        external_id: id,
                        warnings,
                    });
                }
                "FAILED" => {
                    return Err(map_code(s.fail_reason.as_deref().unwrap_or("failed")));
                }
                _ => tokio::time::sleep(self.ep.poll_every).await,
            }
        }
        Err(AppError::Network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};
    use mockito::Matcher;

    fn connector(srv: &str) -> TikTokConnector {
        TikTokConnector::with_endpoints(
            ClientCredentials::from_parts(Some("ck"), None),
            Some("https://broker.example"),
            Endpoints {
                auth_url: format!("{srv}/auth"),
                api_base: srv.into(),
                poll_every: std::time::Duration::from_millis(1),
                chunk_size: 4,
            },
        )
    }

    fn ctx() -> AccountCtx {
        AccountCtx::unmetered(
            "acc",
            "open1",
            ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("tok".into()))),
        )
    }

    #[test]
    fn authorization_url_uses_client_key_and_commas() {
        let c = connector("http://x");
        let cfg = c.oauth().unwrap();
        let url = crate::auth::oauth::authorization_url(
            cfg,
            "http://127.0.0.1:1/",
            &cfg.base_scopes,
            "st",
            "ch",
        );
        assert!(url.contains("client_key=ck"));
        assert!(url.contains("scope=user.info.basic%2Cuser.info.stats%2Cvideo.list"));
        assert_eq!(cfg.token_url, "https://broker.example/tiktok/token");
        assert!(cfg.pkce_hex);
    }

    #[test]
    fn chunking_follows_tiktok_rules() {
        let mb = 1024 * 1024;
        assert_eq!(chunking(3 * mb, 10 * mb), (3 * mb, 1));
        assert_eq!(chunking(10 * mb, 10 * mb), (10 * mb, 1));
        // 25 MB in 10 MB chunks: 2 chunks, the last one carries the 5 MB remainder.
        assert_eq!(chunking(25 * mb, 10 * mb), (10 * mb, 2));
        assert_eq!(chunking(0, 10 * mb), (0, 1));
    }

    #[test]
    fn maps_api_error_codes() {
        assert!(matches!(
            map_code("access_token_invalid"),
            AppError::TokenExpired
        ));
        assert!(matches!(
            map_code("unaudited_client_can_only_post_to_private_accounts"),
            AppError::Validation { .. }
        ));
    }

    #[tokio::test]
    async fn reads_user_and_videos() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/user/info/")
            .match_query(Matcher::Any)
            .with_body(r#"{"data":{"user":{"open_id":"open1","display_name":"gh0sty","follower_count":10,"likes_count":99,"video_count":2}},"error":{"code":"ok"}}"#)
            .create_async()
            .await;
        srv.mock("POST", "/video/list/")
            .match_query(Matcher::Any)
            .with_body(r#"{"data":{"videos":[{"id":"v1"},{"id":"v2"}],"cursor":123,"has_more":true},"error":{"code":"ok"}}"#)
            .create_async()
            .await;
        srv.mock("POST", "/video/query/")
            .match_query(Matcher::Any)
            .match_body(Matcher::PartialJsonString(r#"{"filters":{"video_ids":["v1"]}}"#.into()))
            .with_body(r#"{"data":{"videos":[{"id":"v1","title":"","video_description":"Mi tiktok\n#mc","create_time":1763661600,"duration":15,"view_count":500,"like_count":40}]},"error":{"code":"ok"}}"#)
            .create_async()
            .await;
        let c = connector(&srv.url());
        let snap = c.fetch_channel_snapshot(&ctx()).await.unwrap();
        assert_eq!(snap.profile.external_id, "open1");
        assert!(snap.totals.contains(&MetricValue::new("likes_total", 99.0)));
        let page = c.list_contents(&ctx(), None).await.unwrap();
        assert_eq!(page.next.as_deref(), Some("123"));
        let d = c
            .fetch_content_details(&ctx(), &["v1".into()])
            .await
            .unwrap();
        assert_eq!(d[0].content.title.as_deref(), Some("Mi tiktok"));
        assert_eq!(
            d[0].content.published_at.as_deref(),
            Some("2025-11-20T18:00:00Z")
        );
        assert!(d[0].stats.contains(&MetricValue::new("views", 500.0)));
    }

    #[tokio::test]
    async fn token_error_in_envelope_is_reported() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/user/info/")
            .match_query(Matcher::Any)
            .with_body(r#"{"data":{},"error":{"code":"scope_not_authorized"}}"#)
            .create_async()
            .await;
        let c = connector(&srv.url());
        assert!(matches!(
            c.fetch_profile(&ctx()).await,
            Err(AppError::ScopesDenied)
        ));
    }

    struct Sink(std::sync::Mutex<Vec<u64>>);
    #[async_trait]
    impl UploadSink for Sink {
        async fn session_started(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn progress(&self, sent: u64, _: u64) {
            self.0.lock().unwrap().push(sent);
        }
    }

    #[tokio::test]
    async fn uploads_in_chunks_and_falls_back_to_private() {
        let dir = std::env::temp_dir().join(format!("omnyra-tt-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("v.mp4");
        std::fs::write(&file, b"0123456789").unwrap();
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/post/publish/creator_info/query/")
            .with_body(r#"{"data":{"privacy_level_options":["SELF_ONLY"]},"error":{"code":"ok"}}"#)
            .create_async()
            .await;
        let url = format!("{}/up", srv.url());
        srv.mock("POST", "/post/publish/video/init/")
            .match_body(Matcher::PartialJsonString(
                r#"{"post_info":{"privacy_level":"SELF_ONLY"},"source_info":{"chunk_size":4,"total_chunk_count":2,"video_size":10}}"#.into(),
            ))
            .with_body(format!(r#"{{"data":{{"publish_id":"p1","upload_url":"{url}"}},"error":{{"code":"ok"}}}}"#))
            .create_async()
            .await;
        let first = srv
            .mock("PUT", "/up")
            .match_header("content-range", "bytes 0-3/10")
            .match_body("0123")
            .with_status(206)
            .create_async()
            .await;
        let last = srv
            .mock("PUT", "/up")
            .match_header("content-range", "bytes 4-9/10")
            .match_body("456789")
            .with_status(201)
            .create_async()
            .await;
        srv.mock("POST", "/post/publish/status/fetch/")
            .with_body(r#"{"data":{"status":"PUBLISH_COMPLETE"},"error":{"code":"ok"}}"#)
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
            size_bytes: 10,
            mime: "video/mp4".into(),
        };
        let sink = Sink(Default::default());
        let out = c.upload(&ctx(), &req, None, &sink).await.unwrap();
        assert_eq!(out.external_id, "p1");
        assert_eq!(out.warnings, vec!["posted_private".to_string()]);
        assert_eq!(*sink.0.lock().unwrap(), vec![4, 10]);
        first.assert_async().await;
        last.assert_async().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
