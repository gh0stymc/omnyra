//! YouTube comments: `commentThreads.list` over the whole channel (1 unit per 100
//! threads, readable with `youtube.readonly`) and `comments.insert` to reply (50 units,
//! needs `youtube.force-ssl`).

use super::super::{AccountCtx, Comment, Page};
use crate::{quota, Result};
use serde::Deserialize;
use serde_json::json;

pub const COST_THREADS_LIST: u32 = 1;
pub const COST_COMMENT_INSERT: u32 = 50;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadList {
    #[serde(default)]
    items: Vec<Thread>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Thread {
    snippet: ThreadSnippet,
    replies: Option<Replies>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadSnippet {
    video_id: Option<String>,
    top_level_comment: YtComment,
}

#[derive(Deserialize)]
struct Replies {
    #[serde(default)]
    comments: Vec<YtComment>,
}

#[derive(Deserialize)]
struct YtComment {
    id: String,
    snippet: CommentSnippet,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentSnippet {
    video_id: Option<String>,
    text_original: Option<String>,
    text_display: Option<String>,
    author_display_name: Option<String>,
    author_channel_id: Option<AuthorId>,
    like_count: Option<u32>,
    published_at: Option<String>,
    parent_id: Option<String>,
}

#[derive(Deserialize)]
struct AuthorId {
    value: Option<String>,
}

fn convert(c: YtComment, channel_id: &str, video: Option<&str>) -> Comment {
    let s = c.snippet;
    let author = s.author_channel_id.and_then(|a| a.value);
    Comment {
        is_own: author.as_deref() == Some(channel_id),
        external_id: c.id,
        parent_external_id: s.parent_id,
        content_external_id: s.video_id.or(video.map(str::to_owned)),
        author_name: s.author_display_name,
        author_external_id: author,
        text: s.text_original.or(s.text_display).unwrap_or_default(),
        published_at: s.published_at,
        like_count: s.like_count,
    }
}

pub async fn list_channel(
    ctx: &AccountCtx,
    api_base: &str,
    page: Option<String>,
) -> Result<Page<Comment>> {
    ctx.quota.spend(quota::YOUTUBE, COST_THREADS_LIST)?;
    let mut q = vec![
        ("part", "snippet,replies"),
        ("allThreadsRelatedToChannelId", ctx.external_id.as_str()),
        ("maxResults", "100"),
        ("order", "time"),
        ("textFormat", "plainText"),
    ];
    if let Some(p) = page.as_deref() {
        q.push(("pageToken", p));
    }
    let r: ThreadList = ctx
        .api
        .get_json(&format!("{api_base}/youtube/v3/commentThreads"), &q)
        .await?;
    let mut items = Vec::new();
    for t in r.items {
        let video = t.snippet.video_id.clone();
        items.push(convert(
            t.snippet.top_level_comment,
            &ctx.external_id,
            video.as_deref(),
        ));
        // The thread embeds up to 5 replies: enough to know whether the creator answered.
        for c in t.replies.map(|r| r.comments).unwrap_or_default() {
            items.push(convert(c, &ctx.external_id, video.as_deref()));
        }
    }
    Ok(Page {
        items,
        next: r.next_page_token,
    })
}

pub async fn reply(ctx: &AccountCtx, api_base: &str, parent: &str, text: &str) -> Result<Comment> {
    ctx.quota.spend(quota::YOUTUBE, COST_COMMENT_INSERT)?;
    let body = json!({ "snippet": { "parentId": parent, "textOriginal": text } });
    let c: YtComment = ctx
        .api
        .send_json(|h| {
            h.post(format!("{api_base}/youtube/v3/comments"))
                .query(&[("part", "snippet")])
                .json(&body)
        })
        .await?;
    let mut out = convert(c, &ctx.external_id, None);
    out.is_own = true;
    out.parent_external_id = Some(parent.to_owned());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, StaticToken};
    use std::sync::Arc;

    #[tokio::test]
    async fn lists_threads_with_replies_and_marks_own() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/commentThreads")
            .match_query(mockito::Matcher::UrlEncoded(
                "allThreadsRelatedToChannelId".into(),
                "UC1".into(),
            ))
            .with_body(
                r#"{"items":[{"snippet":{"videoId":"v1","topLevelComment":{"id":"c1","snippet":{"textOriginal":"¿Qué mod es?","authorDisplayName":"Fan","authorChannelId":{"value":"UCfan"},"likeCount":3,"publishedAt":"2026-11-20T10:00:00Z"}}},
                 "replies":{"comments":[{"id":"c1.r1","snippet":{"parentId":"c1","textOriginal":"Sodium","authorChannelId":{"value":"UC1"}}}]}}],
                 "nextPageToken":"N"}"#,
            )
            .create_async()
            .await;
        let ctx = AccountCtx::unmetered(
            "acc",
            "UC1",
            ApiClient::new(reqwest::Client::new(), Arc::new(StaticToken("t".into()))),
        );
        let p = list_channel(&ctx, &srv.url(), None).await.unwrap();
        assert_eq!(p.next.as_deref(), Some("N"));
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[0].content_external_id.as_deref(), Some("v1"));
        assert!(!p.items[0].is_own);
        assert_eq!(p.items[1].parent_external_id.as_deref(), Some("c1"));
        assert!(p.items[1].is_own);
        assert_eq!(p.items[1].content_external_id.as_deref(), Some("v1"));
    }
}
