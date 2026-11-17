//! YouTube Data API v3 calls. Every call is charged to the quota ledger *before* it is made.
//! Costs: https://developers.google.com/youtube/v3/determine_quota_cost

use super::super::{
    kind, AccountCtx, ChannelSnapshot, Content, ContentDetails, ContentRef, MetricValue, Page,
    Profile,
};
use super::duration::parse_iso8601;
use crate::{AppError, Result};
use serde::Deserialize;

pub const COST_CHANNELS_LIST: u32 = 1;
pub const COST_PLAYLIST_ITEMS_LIST: u32 = 1;
pub const COST_VIDEOS_LIST: u32 = 1;
/// `maxResults` limit of `playlistItems.list` and `videos.list`.
pub const PAGE_SIZE: usize = 50;

// ---- channels.list -------------------------------------------------------------------

#[derive(Deserialize)]
struct ChannelList {
    #[serde(default)]
    items: Vec<Channel>,
}

#[derive(Deserialize)]
struct Channel {
    id: String,
    snippet: ChannelSnippet,
    statistics: Option<ChannelStats>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSnippet {
    title: String,
    custom_url: Option<String>,
    published_at: Option<String>,
    thumbnails: Option<Thumbnails>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelStats {
    view_count: Option<String>,
    subscriber_count: Option<String>,
    #[serde(default)]
    hidden_subscriber_count: bool,
    video_count: Option<String>,
}

#[derive(Deserialize)]
struct Thumbnails {
    default: Option<Thumb>,
    medium: Option<Thumb>,
    high: Option<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

fn num(s: &Option<String>) -> Option<f64> {
    s.as_deref().and_then(|v| v.parse().ok())
}

fn profile_of(ch: &Channel) -> Profile {
    Profile {
        external_id: ch.id.clone(),
        handle: ch.snippet.custom_url.as_ref().map(|h| {
            if h.starts_with('@') {
                h.clone()
            } else {
                format!("@{h}")
            }
        }),
        display_name: ch.snippet.title.clone(),
        avatar_url: ch
            .snippet
            .thumbnails
            .as_ref()
            .and_then(|t| t.default.as_ref())
            .map(|t| t.url.clone()),
    }
}

async fn my_channel(ctx: &AccountCtx, base: &str, part: &str) -> Result<Channel> {
    ctx.quota.spend(COST_CHANNELS_LIST)?;
    let list: ChannelList = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/channels"),
            &[("part", part), ("mine", "true"), ("maxResults", "1")],
        )
        .await?;
    list.items.into_iter().next().ok_or(AppError::NoChannel)
}

/// `channels.list?mine=true` (1 unit). The channel is the one picked on Google's consent
/// screen (personal or brand account).
pub async fn profile(ctx: &AccountCtx, base: &str) -> Result<Profile> {
    Ok(profile_of(&my_channel(ctx, base, "snippet").await?))
}

pub async fn snapshot(ctx: &AccountCtx, base: &str) -> Result<ChannelSnapshot> {
    let ch = my_channel(ctx, base, "snippet,statistics").await?;
    if !ctx.external_id.is_empty() && ch.id != ctx.external_id {
        // The grant now points at another channel (e.g. a brand account was re-picked).
        tracing::warn!("token belongs to a different channel than the stored account");
        return Err(AppError::NotConnected);
    }
    let mut totals = Vec::new();
    if let Some(s) = &ch.statistics {
        if !s.hidden_subscriber_count {
            if let Some(v) = num(&s.subscriber_count) {
                totals.push(MetricValue::new("followers", v));
            }
        }
        if let Some(v) = num(&s.view_count) {
            totals.push(MetricValue::new("views_total", v));
        }
        if let Some(v) = num(&s.video_count) {
            totals.push(MetricValue::new("contents_total", v));
        }
    }
    Ok(ChannelSnapshot {
        profile: profile_of(&ch),
        created_at: ch.snippet.published_at.clone(),
        totals,
    })
}

// ---- playlistItems.list (uploads) ----------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistItems {
    #[serde(default)]
    items: Vec<PlaylistItem>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistItem {
    content_details: PlaylistItemDetails,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistItemDetails {
    video_id: String,
}

/// The "uploads" playlist of channel `UCxxxx` is `UUxxxx` (stable convention), which saves a
/// `channels.list contentDetails` call.
pub fn uploads_playlist(channel_id: &str) -> Option<String> {
    channel_id
        .strip_prefix("UC")
        .map(|rest| format!("UU{rest}"))
}

/// One page (50) of the uploads playlist, newest first (1 unit).
pub async fn list_uploads(
    ctx: &AccountCtx,
    base: &str,
    page: Option<String>,
) -> Result<Page<ContentRef>> {
    let playlist = uploads_playlist(&ctx.external_id).ok_or(AppError::Internal)?;
    ctx.quota.spend(COST_PLAYLIST_ITEMS_LIST)?;
    let max = PAGE_SIZE.to_string();
    let mut query = vec![
        ("part", "contentDetails"),
        ("playlistId", playlist.as_str()),
        ("maxResults", max.as_str()),
    ];
    if let Some(p) = page.as_deref() {
        query.push(("pageToken", p));
    }
    let res: Result<PlaylistItems> = ctx
        .api
        .get_json(&format!("{base}/youtube/v3/playlistItems"), &query)
        .await;
    let list = match res {
        Ok(l) => l,
        // A channel that never uploaded has no uploads playlist.
        Err(AppError::NotFound) => {
            return Ok(Page {
                items: Vec::new(),
                next: None,
            })
        }
        Err(e) => return Err(e),
    };
    Ok(Page {
        items: list
            .items
            .into_iter()
            .map(|i| ContentRef {
                external_id: i.content_details.video_id,
            })
            .collect(),
        next: list.next_page_token,
    })
}

// ---- videos.list ---------------------------------------------------------------------

#[derive(Deserialize)]
struct VideoList {
    #[serde(default)]
    items: Vec<Video>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Video {
    id: String,
    snippet: Option<VideoSnippet>,
    content_details: Option<VideoContentDetails>,
    statistics: Option<VideoStats>,
    status: Option<VideoStatus>,
    live_streaming_details: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoSnippet {
    title: Option<String>,
    description: Option<String>,
    published_at: Option<String>,
    thumbnails: Option<Thumbnails>,
}

#[derive(Deserialize)]
struct VideoContentDetails {
    duration: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoStats {
    view_count: Option<String>,
    like_count: Option<String>,
    comment_count: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoStatus {
    privacy_status: Option<String>,
}

fn details_of(v: Video) -> ContentDetails {
    let is_stream = v.live_streaming_details.is_some();
    let snippet = v.snippet;
    let thumbnail_url = snippet
        .as_ref()
        .and_then(|s| s.thumbnails.as_ref())
        .and_then(|t| t.medium.as_ref().or(t.high.as_ref()).or(t.default.as_ref()))
        .map(|t| t.url.clone());
    let mut stats = Vec::new();
    if let Some(s) = &v.statistics {
        for (metric, val) in [
            ("views", &s.view_count),
            ("likes", &s.like_count),
            ("comments", &s.comment_count),
        ] {
            if let Some(x) = num(val) {
                stats.push(MetricValue::new(metric, x));
            }
        }
    }
    ContentDetails {
        content: Content {
            url: Some(format!("https://www.youtube.com/watch?v={}", v.id)),
            external_id: v.id,
            kind: if is_stream { kind::STREAM } else { kind::VIDEO }.into(),
            title: snippet.as_ref().and_then(|s| s.title.clone()),
            description: snippet.as_ref().and_then(|s| s.description.clone()),
            thumbnail_url,
            published_at: snippet.as_ref().and_then(|s| s.published_at.clone()),
            duration_s: v
                .content_details
                .and_then(|c| c.duration)
                .and_then(|d| parse_iso8601(&d)),
            visibility: v.status.and_then(|s| s.privacy_status),
        },
        kind_final: is_stream,
        stats,
    }
}

/// `videos.list` in batches of 50 (1 unit per batch). Deleted videos are simply absent.
pub async fn videos(ctx: &AccountCtx, base: &str, ids: &[String]) -> Result<Vec<ContentDetails>> {
    let mut out = Vec::with_capacity(ids.len());
    for batch in ids.chunks(PAGE_SIZE) {
        ctx.quota.spend(COST_VIDEOS_LIST)?;
        let joined = batch.join(",");
        let list: VideoList = ctx
            .api
            .get_json(
                &format!("{base}/youtube/v3/videos"),
                &[
                    (
                        "part",
                        "snippet,contentDetails,statistics,status,liveStreamingDetails",
                    ),
                    ("id", joined.as_str()),
                    ("maxResults", "50"),
                ],
            )
            .await?;
        out.extend(list.items.into_iter().map(details_of));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_playlist_from_channel_id() {
        assert_eq!(uploads_playlist("UCabc").as_deref(), Some("UUabc"));
        assert_eq!(uploads_playlist("xyz"), None);
    }

    #[test]
    fn maps_video_details() {
        let v: Video = serde_json::from_str(
            r#"{"id":"v1","snippet":{"title":"T","publishedAt":"2026-11-01T10:00:00Z",
                "thumbnails":{"default":{"url":"d"},"medium":{"url":"m"}}},
                "contentDetails":{"duration":"PT45S"},
                "statistics":{"viewCount":"120","likeCount":"7"},
                "status":{"privacyStatus":"public"}}"#,
        )
        .unwrap();
        let d = details_of(v);
        assert_eq!(d.content.kind, "video");
        assert!(!d.kind_final);
        assert_eq!(d.content.duration_s, Some(45));
        assert_eq!(d.content.thumbnail_url.as_deref(), Some("m"));
        assert_eq!(d.content.visibility.as_deref(), Some("public"));
        assert_eq!(
            d.stats,
            vec![
                MetricValue::new("views", 120.0),
                MetricValue::new("likes", 7.0)
            ]
        );

        let live: Video = serde_json::from_str(
            r#"{"id":"s1","liveStreamingDetails":{"actualStartTime":"2026-11-01T10:00:00Z"}}"#,
        )
        .unwrap();
        let d = details_of(live);
        assert_eq!(d.content.kind, "stream");
        assert!(d.kind_final);
    }
}
