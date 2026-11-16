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

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub external_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub duration_s: Option<u32>,
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
    pub author_name: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    pub file_path: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UploadOutcome {
    pub external_id: String,
}
