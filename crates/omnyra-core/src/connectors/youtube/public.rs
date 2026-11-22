//! Public YouTube data of other channels (6b competitors and trends, 6c impersonation).
//! Costs: `channels.list` 1, `playlistItems.list` 1, `videos.list` 1, `search.list` 100.

use super::super::{AccountCtx, PublicChannel, PublicVideo};
use super::api::uploads_playlist;
use crate::{quota, AppError, Result};
use serde::Deserialize;

pub const COST_SEARCH: u32 = 100;

#[derive(Deserialize)]
struct List<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Deserialize)]
struct Channel {
    id: String,
    snippet: Option<ChannelSnippet>,
    statistics: Option<Stats>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSnippet {
    title: Option<String>,
    custom_url: Option<String>,
    published_at: Option<String>,
    thumbnails: Option<Thumbs>,
}

#[derive(Deserialize)]
struct Thumbs {
    default: Option<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    view_count: Option<String>,
    subscriber_count: Option<String>,
    #[serde(default)]
    hidden_subscriber_count: bool,
    video_count: Option<String>,
    like_count: Option<String>,
    comment_count: Option<String>,
}

fn n(s: &Option<String>) -> Option<f64> {
    s.as_deref().and_then(|v| v.parse().ok())
}

fn channel(c: Channel) -> PublicChannel {
    let sn = c.snippet;
    let st = c.statistics;
    PublicChannel {
        handle: sn.as_ref().and_then(|s| s.custom_url.clone()).map(|h| {
            if h.starts_with('@') {
                h
            } else {
                format!("@{h}")
            }
        }),
        display_name: sn
            .as_ref()
            .and_then(|s| s.title.clone())
            .unwrap_or_default(),
        avatar_url: sn
            .as_ref()
            .and_then(|s| s.thumbnails.as_ref())
            .and_then(|t| t.default.as_ref())
            .map(|t| t.url.clone()),
        created_at: sn.and_then(|s| s.published_at),
        followers: st
            .as_ref()
            .filter(|s| !s.hidden_subscriber_count)
            .and_then(|s| n(&s.subscriber_count)),
        views_total: st.as_ref().and_then(|s| n(&s.view_count)),
        contents_total: st.as_ref().and_then(|s| n(&s.video_count)),
        external_id: c.id,
    }
}

/// What the user typed → `("id", UC…)` or `("forHandle", @x)`.
pub fn parse_query(q: &str) -> Option<(&'static str, String)> {
    let q = q.trim();
    let tail = q
        .trim_end_matches('/')
        .rsplit_once("youtube.com/")
        .map(|(_, t)| t)
        .unwrap_or(q);
    let tail = tail.split(['?', '#']).next().unwrap_or(tail);
    if let Some(id) = tail.strip_prefix("channel/") {
        return Some(("id", id.split('/').next()?.to_owned()));
    }
    let first = tail.split('/').next().unwrap_or(tail);
    if first.starts_with("UC") && first.len() == 24 && !first.contains(' ') {
        return Some(("id", first.to_owned()));
    }
    let handle = first.trim_start_matches('@');
    if handle.is_empty()
        || handle.len() > 100
        || !handle
            .chars()
            .all(|c| c.is_alphanumeric() || "._-".contains(c))
    {
        return None;
    }
    Some(("forHandle", format!("@{handle}")))
}

pub async fn find(ctx: &AccountCtx, base: &str, query: &str) -> Result<PublicChannel> {
    let (key, value) = parse_query(query).ok_or(AppError::Validation {
        field: "query".into(),
    })?;
    ctx.quota.spend(quota::YOUTUBE, 1)?;
    let l: List<Channel> = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/channels"),
            &[("part", "snippet,statistics"), (key, value.as_str())],
        )
        .await?;
    l.items
        .into_iter()
        .next()
        .map(channel)
        .ok_or(AppError::NotFound)
}

pub async fn channels(ctx: &AccountCtx, base: &str, ids: &[String]) -> Result<Vec<PublicChannel>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(50) {
        ctx.quota.spend(quota::YOUTUBE, 1)?;
        let joined = chunk.join(",");
        let l: List<Channel> = ctx
            .api
            .get_json(
                &format!("{base}/youtube/v3/channels"),
                &[
                    ("part", "snippet,statistics"),
                    ("id", joined.as_str()),
                    ("maxResults", "50"),
                ],
            )
            .await?;
        out.extend(l.items.into_iter().map(channel));
    }
    Ok(out)
}

#[derive(Deserialize)]
struct Video {
    id: String,
    snippet: Option<VideoSnippet>,
    statistics: Option<Stats>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoSnippet {
    channel_id: Option<String>,
    channel_title: Option<String>,
    title: Option<String>,
    published_at: Option<String>,
}

pub async fn videos(ctx: &AccountCtx, base: &str, ids: &[String]) -> Result<Vec<PublicVideo>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(50) {
        ctx.quota.spend(quota::YOUTUBE, 1)?;
        let joined = chunk.join(",");
        let l: List<Video> = ctx
            .api
            .get_json(
                &format!("{base}/youtube/v3/videos"),
                &[
                    ("part", "snippet,statistics"),
                    ("id", joined.as_str()),
                    ("maxResults", "50"),
                ],
            )
            .await?;
        out.extend(l.items.into_iter().map(|v| {
            let sn = v.snippet;
            let st = v.statistics;
            PublicVideo {
                url: Some(format!("https://www.youtube.com/watch?v={}", v.id)),
                channel_id: sn
                    .as_ref()
                    .and_then(|s| s.channel_id.clone())
                    .unwrap_or_default(),
                channel_title: sn.as_ref().and_then(|s| s.channel_title.clone()),
                title: sn.as_ref().and_then(|s| s.title.clone()),
                published_at: sn.and_then(|s| s.published_at),
                views: st.as_ref().and_then(|s| n(&s.view_count)),
                likes: st.as_ref().and_then(|s| n(&s.like_count)),
                comments: st.as_ref().and_then(|s| n(&s.comment_count)),
                external_id: v.id,
            }
        }));
    }
    Ok(out)
}

pub async fn recent(
    ctx: &AccountCtx,
    base: &str,
    channel_id: &str,
    max: usize,
) -> Result<Vec<PublicVideo>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        content_details: Details,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Details {
        video_id: String,
    }
    let playlist = uploads_playlist(channel_id).ok_or(AppError::Validation {
        field: "channel".into(),
    })?;
    ctx.quota.spend(quota::YOUTUBE, 1)?;
    let max_s = max.clamp(1, 50).to_string();
    let l: Result<List<Item>> = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/playlistItems"),
            &[
                ("part", "contentDetails"),
                ("playlistId", playlist.as_str()),
                ("maxResults", max_s.as_str()),
            ],
        )
        .await;
    let ids: Vec<String> = match l {
        Ok(l) => l
            .items
            .into_iter()
            .map(|i| i.content_details.video_id)
            .collect(),
        Err(AppError::NotFound) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    videos(ctx, base, &ids).await
}

#[derive(Deserialize)]
struct SearchItem {
    id: SearchId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchId {
    video_id: Option<String>,
    channel_id: Option<String>,
}

pub async fn search_videos(
    ctx: &AccountCtx,
    base: &str,
    q: &str,
    after: &str,
) -> Result<Vec<PublicVideo>> {
    ctx.quota.spend(quota::YOUTUBE, COST_SEARCH)?;
    let l: List<SearchItem> = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/search"),
            &[
                ("part", "id"),
                ("type", "video"),
                ("q", q),
                ("order", "viewCount"),
                ("publishedAfter", after),
                ("maxResults", "25"),
            ],
        )
        .await?;
    let ids: Vec<String> = l.items.into_iter().filter_map(|i| i.id.video_id).collect();
    videos(ctx, base, &ids).await
}

pub async fn search_channels(ctx: &AccountCtx, base: &str, q: &str) -> Result<Vec<PublicChannel>> {
    ctx.quota.spend(quota::YOUTUBE, COST_SEARCH)?;
    let l: List<SearchItem> = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/search"),
            &[
                ("part", "id"),
                ("type", "channel"),
                ("q", q),
                ("maxResults", "25"),
            ],
        )
        .await?;
    let ids: Vec<String> = l
        .items
        .into_iter()
        .filter_map(|i| i.id.channel_id)
        .collect();
    channels(ctx, base, &ids).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_handles_ids_and_urls() {
        assert_eq!(
            parse_query("@gh0stymc"),
            Some(("forHandle", "@gh0stymc".into()))
        );
        assert_eq!(
            parse_query("gh0stymc"),
            Some(("forHandle", "@gh0stymc".into()))
        );
        assert_eq!(
            parse_query("https://www.youtube.com/@Some.Channel/videos?x=1"),
            Some(("forHandle", "@Some.Channel".into()))
        );
        assert_eq!(
            parse_query("https://youtube.com/channel/UC1234567890123456789012"),
            Some(("id", "UC1234567890123456789012".into()))
        );
        assert_eq!(
            parse_query("UC1234567890123456789012"),
            Some(("id", "UC1234567890123456789012".into()))
        );
        assert_eq!(parse_query("two words"), None);
        assert_eq!(parse_query(""), None);
    }
}
