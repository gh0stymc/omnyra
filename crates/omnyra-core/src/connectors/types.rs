use serde::{Deserialize, Serialize};
use specta::Type;

pub type PlatformId = String;

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UploadSupport {
    None,
    Direct,
    DraftOnly,
    PrivateOnlyUnaudited,
}

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleSupport {
    None,
    Native,
    LocalOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CommentSupport {
    None,
    Read,
    ReadReply,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaLimits {
    pub max_duration_s: Option<u32>,
    pub max_size_mb: Option<u32>,
    pub max_title_len: Option<u32>,
    pub max_description_len: Option<u32>,
    pub max_hashtags: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub analytics: bool,
    pub historic_analytics: bool,
    pub upload: UploadSupport,
    pub resumable_upload: bool,
    pub schedule: ScheduleSupport,
    pub comments: CommentSupport,
    pub live: bool,
    pub public_lookup: bool,
    pub limits: MediaLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    /// No login (mock).
    None,
    /// OAuth 2.0 + PKCE with a loopback redirect.
    OAuth,
    /// OAuth 2.0 device code flow: the user types a code on the provider's site.
    DeviceCode,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub id: PlatformId,
    pub display_name: String,
    pub brand_color: String,
    pub capabilities: Capabilities,
    pub auth: AuthKind,
    /// False when the platform needs a client ID that was not provided at build time.
    pub auth_configured: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub external_id: String,
    pub handle: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// `day` is `YYYY-MM-DD` (UTC). `metric` mixes common keys (`followers`, `views`...)
/// with platform keys (`yt.avg_view_pct`...), so the DB schema never changes per network.
#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricPoint {
    pub day: String,
    pub metric: String,
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

/// A metric value without a day (current totals, per-content stats).
#[derive(Debug, Clone, PartialEq)]
pub struct MetricValue {
    pub metric: String,
    pub value: f64,
}

impl MetricValue {
    pub fn new(metric: &str, value: f64) -> Self {
        Self {
            metric: metric.into(),
            value,
        }
    }
}

/// Current state of the channel: profile plus cumulative totals (`followers`,
/// `views_total`, `contents_total`). One cheap call; taken on every sync.
#[derive(Debug, Clone)]
pub struct ChannelSnapshot {
    pub profile: Profile,
    pub totals: Vec<MetricValue>,
    /// When the channel was created (ISO-8601): where the history backfill starts.
    pub created_at: Option<String>,
}

/// One entry of the content listing (newest first). Details come from
/// [`super::Connector::fetch_content_details`].
#[derive(Debug, Clone, PartialEq)]
pub struct ContentRef {
    pub external_id: String,
}

/// Content kinds stored in `contents.kind`.
pub mod kind {
    pub const VIDEO: &str = "video";
    pub const SHORT: &str = "short";
    pub const STREAM: &str = "stream";
    /// Recording of a past live stream (Twitch archive or highlight).
    pub const VOD: &str = "vod";
    pub const CLIP: &str = "clip";
    /// Instagram Reel.
    pub const REEL: &str = "reel";
    /// Instagram feed post (image, carousel or non-Reel video).
    pub const POST: &str = "post";
    pub const STORY: &str = "story";
}

/// A live stream in progress, as the platform reports it right now.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveStatus {
    pub stream_id: String,
    pub started_at: String,
    pub viewers: u32,
    pub title: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Content {
    pub external_id: String,
    /// See [`kind`]. Provisional unless `kind_final` is set (see [`ContentDetails`]).
    pub kind: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub published_at: Option<String>,
    pub duration_s: Option<u32>,
    pub visibility: Option<String>,
    /// Platform fields kept in `contents.extra` (Twitch: the VOD's `stream_id`).
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ContentDetails {
    pub content: Content,
    /// True when the API alone settles the kind (e.g. a live stream). Otherwise the
    /// connector's `classify` may refine it once (YouTube: Short or regular video).
    pub kind_final: bool,
    /// Current cumulative stats (`views`, `likes`, `comments`).
    pub stats: Vec<MetricValue>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub external_id: String,
    /// Set on replies: the top-level comment they answer.
    pub parent_external_id: Option<String>,
    pub author_name: Option<String>,
    pub author_external_id: Option<String>,
    pub text: String,
    /// RFC 3339.
    pub published_at: Option<String>,
    pub like_count: Option<u32>,
    /// Written by the account owner.
    pub is_own: bool,
}

/// What the user wants to publish, as entered in the upload form.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadDraft {
    pub file_path: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `private` | `unlisted` | `public`.
    pub visibility: String,
    pub category_id: Option<String>,
    pub playlist_id: Option<String>,
    pub thumbnail_path: Option<String>,
    /// Required by YouTube (COPPA): is the video made for kids?
    pub made_for_kids: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// A validation finding. `code` is stable and translated by the frontend
/// (`upload.issue.<code>`); `limit` carries the number it refers to, if any.
#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub field: String,
    pub code: String,
    pub severity: Severity,
    pub limit: Option<f64>,
}

impl Issue {
    pub fn error(field: &str, code: &str) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            severity: Severity::Error,
            limit: None,
        }
    }
    pub fn warning(field: &str, code: &str) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(field, code)
        }
    }
    pub fn with_limit(mut self, limit: f64) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Facts about the local files, gathered before the connector checks its own rules.
#[derive(Debug, Clone, PartialEq)]
pub struct FileFacts {
    pub size_bytes: u64,
    /// Lower-case, without the dot.
    pub extension: String,
    /// `None` when ffprobe is not available.
    pub media: Option<crate::media::MediaInfo>,
    pub thumbnail: Option<ThumbnailFacts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailFacts {
    pub size_bytes: u64,
    pub extension: String,
}

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub draft: UploadDraft,
    pub size_bytes: u64,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadOutcome {
    pub external_id: String,
    /// Non-fatal problems (`thumbnail_failed`, `playlist_failed`...).
    pub warnings: Vec<String>,
}

/// Receives upload progress, and the resumable session URI as soon as it exists so the
/// caller can keep it (in the keyring, H6) and resume after a restart.
#[async_trait::async_trait]
pub trait UploadSink: Send + Sync {
    async fn session_started(&self, session_uri: &str) -> crate::Result<()>;
    fn progress(&self, sent: u64, total: u64);
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamedOption {
    pub id: String,
    pub title: String,
}

/// Choices for the upload form that come from the platform.
#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadOptions {
    pub categories: Vec<NamedOption>,
    pub playlists: Vec<NamedOption>,
}
