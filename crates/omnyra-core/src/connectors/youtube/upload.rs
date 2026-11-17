//! YouTube uploads: form rules, resumable upload protocol, thumbnail and playlist.
//!
//! Resumable protocol (https://developers.google.com/youtube/v3/guides/using_resumable_upload_protocol):
//! 1. `POST /upload/youtube/v3/videos?uploadType=resumable` with the metadata → `Location`
//!    is the session URI. It allows finishing the upload without a token for days, so the
//!    caller keeps it in the keyring (H6), never in SQLite or logs.
//! 2. `PUT <session>` with `Content-Range: bytes a-b/total` per chunk → `308` plus
//!    `Range: bytes=0-n` while incomplete, `200/201` with the video at the end.
//! 3. After an interruption: `PUT <session>` with `Content-Range: bytes */total` says how
//!    much the server has; `404/410` means the session is gone and we start again.

use super::super::{
    http::error_from, AccountCtx, FileFacts, Issue, NamedOption, UploadOptions, UploadOutcome,
    UploadRequest, UploadSink,
};
use crate::{quota, AppError, Result};
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub const COST_VIDEOS_INSERT: u32 = 1; // in its own bucket (quota::YOUTUBE_UPLOAD)
pub const COST_THUMBNAILS_SET: u32 = 50;
pub const COST_PLAYLIST_ITEMS_INSERT: u32 = 50;
pub const COST_LIST: u32 = 1;

/// 8 MiB: a multiple of 256 KiB, as the protocol requires for every chunk but the last.
pub const CHUNK_SIZE: usize = 8 * 1024 * 1024;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "wmv", "flv", "webm", "mkv", "mpg", "mpeg", "3gp", "3g2", "mts",
    "m2ts", "ts", "ogv",
];
const MAX_SIZE_BYTES: u64 = 256 * 1024 * 1024 * 1024;
const MAX_DURATION_S: f64 = 12.0 * 3600.0;
/// Accounts without phone verification are limited to 15 minutes.
const UNVERIFIED_MAX_DURATION_S: f64 = 15.0 * 60.0;
const MAX_TITLE_CHARS: usize = 100;
const MAX_DESCRIPTION_BYTES: usize = 5000;
const MAX_TAGS_CHARS: usize = 500;
/// With more hashtags than this, YouTube ignores all of them.
const MAX_HASHTAGS: usize = 60;
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;
const VISIBILITIES: &[&str] = &["private", "unlisted", "public"];
/// "People & Blogs", YouTube's own default.
const DEFAULT_CATEGORY: &str = "22";

/// Length of the tags field as YouTube counts it: tags with spaces are quoted, and they
/// are joined with commas.
fn tags_len(tags: &[String]) -> usize {
    let content: usize = tags
        .iter()
        .map(|t| t.chars().count() + if t.contains(' ') { 2 } else { 0 })
        .sum();
    content + tags.len().saturating_sub(1)
}

fn hashtags(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.len() > 1 && w.starts_with('#'))
        .count()
}

pub fn check(draft: &super::super::UploadDraft, f: &FileFacts) -> Vec<Issue> {
    let mut out = Vec::new();
    if !VIDEO_EXTENSIONS.contains(&f.extension.as_str()) {
        out.push(Issue::error("file", "format"));
    }
    if f.size_bytes == 0 {
        out.push(Issue::error("file", "empty"));
    } else if f.size_bytes > MAX_SIZE_BYTES {
        out.push(Issue::error("file", "too_big").with_limit(MAX_SIZE_BYTES as f64));
    }
    match &f.media {
        None => out.push(Issue::warning("file", "probe_unavailable")),
        Some(m) if !m.has_video() => out.push(Issue::error("file", "no_video")),
        Some(m) => match m.duration_s {
            Some(d) if d > MAX_DURATION_S => {
                out.push(Issue::error("file", "too_long").with_limit(MAX_DURATION_S))
            }
            Some(d) if d > UNVERIFIED_MAX_DURATION_S => out.push(
                Issue::warning("file", "needs_verification").with_limit(UNVERIFIED_MAX_DURATION_S),
            ),
            _ => {}
        },
    }

    let title = draft.title.trim();
    if title.is_empty() {
        out.push(Issue::error("title", "required"));
    } else if title.chars().count() > MAX_TITLE_CHARS {
        out.push(Issue::error("title", "too_long").with_limit(MAX_TITLE_CHARS as f64));
    }
    if title.contains(['<', '>']) {
        out.push(Issue::error("title", "invalid_chars"));
    }
    if draft.description.len() > MAX_DESCRIPTION_BYTES {
        out.push(Issue::error("description", "too_long").with_limit(MAX_DESCRIPTION_BYTES as f64));
    }
    if draft.description.contains(['<', '>']) {
        out.push(Issue::error("description", "invalid_chars"));
    }
    if hashtags(&draft.title) + hashtags(&draft.description) > MAX_HASHTAGS {
        out.push(
            Issue::warning("description", "too_many_hashtags").with_limit(MAX_HASHTAGS as f64),
        );
    }
    if draft
        .tags
        .iter()
        .any(|t| t.trim().is_empty() || t.contains(['<', '>', ',']))
    {
        out.push(Issue::error("tags", "invalid_chars"));
    }
    if tags_len(&draft.tags) > MAX_TAGS_CHARS {
        out.push(Issue::error("tags", "too_long").with_limit(MAX_TAGS_CHARS as f64));
    }

    if !VISIBILITIES.contains(&draft.visibility.as_str()) {
        out.push(Issue::error("visibility", "invalid"));
    } else if draft.visibility != "private" {
        // API projects without Google's compliance audit can only upload private videos.
        out.push(Issue::warning("visibility", "private_until_audit"));
    }

    if let Some(t) = &f.thumbnail {
        if !["jpg", "jpeg", "png"].contains(&t.extension.as_str()) {
            out.push(Issue::error("thumbnail", "format"));
        }
        if t.size_bytes > MAX_THUMBNAIL_BYTES {
            out.push(Issue::error("thumbnail", "too_big").with_limit(MAX_THUMBNAIL_BYTES as f64));
        }
    }
    out
}

// ---- form options --------------------------------------------------------------------

#[derive(Deserialize)]
struct List {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: String,
    snippet: ItemSnippet,
}

#[derive(Deserialize)]
struct ItemSnippet {
    title: String,
    #[serde(default = "yes")]
    assignable: bool,
}

fn yes() -> bool {
    true
}

pub async fn options(ctx: &AccountCtx, base: &str, lang: &str) -> Result<UploadOptions> {
    let region = if lang.starts_with("es") { "ES" } else { "US" };
    ctx.quota.spend(quota::YOUTUBE, COST_LIST)?;
    let cats: List = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/videoCategories"),
            &[("part", "snippet"), ("regionCode", region), ("hl", lang)],
        )
        .await?;
    ctx.quota.spend(quota::YOUTUBE, COST_LIST)?;
    let lists: List = ctx
        .api
        .get_json(
            &format!("{base}/youtube/v3/playlists"),
            &[("part", "snippet"), ("mine", "true"), ("maxResults", "50")],
        )
        .await?;
    let named = |items: Vec<Item>| {
        items
            .into_iter()
            .filter(|i| i.snippet.assignable)
            .map(|i| NamedOption {
                id: i.id,
                title: i.snippet.title,
            })
            .collect()
    };
    Ok(UploadOptions {
        categories: named(cats.items),
        playlists: named(lists.items),
    })
}

// ---- resumable upload ----------------------------------------------------------------

#[derive(Deserialize)]
struct Video {
    id: String,
}

enum ServerState {
    Done(String),
    /// Next byte the server expects.
    Resume(u64),
}

/// Parses `Range: bytes=0-1234` into the next offset (1235). No header: nothing stored.
fn next_offset(resp: &reqwest::Response) -> u64 {
    resp.headers()
        .get(reqwest::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('-').next())
        .and_then(|n| n.parse::<u64>().ok())
        .map_or(0, |n| n + 1)
}

async fn finished(resp: reqwest::Response) -> Result<String> {
    let v: Video = resp.json().await.map_err(|_| AppError::Internal)?;
    Ok(v.id)
}

async fn interpret(resp: reqwest::Response) -> Result<ServerState> {
    match resp.status().as_u16() {
        200 | 201 => Ok(ServerState::Done(finished(resp).await?)),
        308 => Ok(ServerState::Resume(next_offset(&resp))),
        404 | 410 => Err(AppError::UploadSessionExpired),
        _ => Err(error_from(resp).await),
    }
}

async fn start_session(ctx: &AccountCtx, base: &str, req: &UploadRequest) -> Result<String> {
    ctx.quota.spend(quota::YOUTUBE_UPLOAD, COST_VIDEOS_INSERT)?;
    let d = &req.draft;
    let body = json!({
        "snippet": {
            "title": d.title.trim(),
            "description": d.description,
            "tags": d.tags,
            "categoryId": d.category_id.as_deref().unwrap_or(DEFAULT_CATEGORY),
        },
        "status": {
            "privacyStatus": d.visibility,
            "selfDeclaredMadeForKids": d.made_for_kids,
        },
    });
    let url = format!("{base}/upload/youtube/v3/videos");
    let resp = ctx
        .api
        .send(|http| {
            http.post(&url)
                .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
                .header("X-Upload-Content-Length", req.size_bytes)
                .header("X-Upload-Content-Type", &req.mime)
                .json(&body)
        })
        .await?;
    if !resp.status().is_success() {
        return Err(error_from(resp).await);
    }
    resp.headers()
        .get(reqwest::header::LOCATION)
        .and_then(|l| l.to_str().ok())
        .map(str::to_owned)
        .ok_or(AppError::Internal)
}

async fn server_state(ctx: &AccountCtx, session: &str, total: u64) -> Result<ServerState> {
    let resp = ctx
        .api
        .send(|http| {
            http.put(session)
                .header(reqwest::header::CONTENT_RANGE, format!("bytes */{total}"))
                .header(reqwest::header::CONTENT_LENGTH, 0)
        })
        .await?;
    interpret(resp).await
}

/// Reads up to `buf.len()` bytes (fewer only at end of file).
async fn read_chunk(file: &mut tokio::fs::File, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = file
            .read(&mut buf[filled..])
            .await
            .map_err(|_| AppError::FileUnavailable)?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

async fn send_from(
    ctx: &AccountCtx,
    session: &str,
    req: &UploadRequest,
    mut offset: u64,
    chunk_size: usize,
    sink: &dyn UploadSink,
) -> Result<String> {
    let total = req.size_bytes;
    let mut file = tokio::fs::File::open(&req.draft.file_path)
        .await
        .map_err(|_| AppError::FileUnavailable)?;
    let mut buf = vec![0u8; chunk_size];
    loop {
        sink.progress(offset, total);
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| AppError::FileUnavailable)?;
        let want = chunk_size.min((total - offset) as usize);
        let n = read_chunk(&mut file, &mut buf[..want]).await?;
        if n < want {
            // The file shrank since it was queued.
            return Err(AppError::FileUnavailable);
        }
        let end = offset + n as u64 - 1;
        let body = buf[..n].to_vec();
        let resp = ctx
            .api
            .send(|http| {
                http.put(session)
                    .header(
                        reqwest::header::CONTENT_RANGE,
                        format!("bytes {offset}-{end}/{total}"),
                    )
                    .body(body.clone())
            })
            .await?;
        match interpret(resp).await? {
            ServerState::Done(id) => {
                sink.progress(total, total);
                return Ok(id);
            }
            ServerState::Resume(next) => offset = next,
        }
    }
}

async fn set_thumbnail(ctx: &AccountCtx, base: &str, video_id: &str, path: &str) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| AppError::FileUnavailable)?;
    let mime = if path.to_lowercase().ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    ctx.quota.spend(quota::YOUTUBE, COST_THUMBNAILS_SET)?;
    let url = format!("{base}/upload/youtube/v3/thumbnails/set");
    let resp = ctx
        .api
        .send(|http| {
            http.post(&url)
                .query(&[("videoId", video_id), ("uploadType", "media")])
                .header(reqwest::header::CONTENT_TYPE, mime)
                .body(bytes.clone())
        })
        .await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(error_from(resp).await)
    }
}

async fn add_to_playlist(
    ctx: &AccountCtx,
    base: &str,
    video_id: &str,
    playlist_id: &str,
) -> Result<()> {
    ctx.quota
        .spend(quota::YOUTUBE, COST_PLAYLIST_ITEMS_INSERT)?;
    let url = format!("{base}/youtube/v3/playlistItems");
    let body = json!({
        "snippet": {
            "playlistId": playlist_id,
            "resourceId": { "kind": "youtube#video", "videoId": video_id },
        }
    });
    let resp = ctx
        .api
        .send(|http| http.post(&url).query(&[("part", "snippet")]).json(&body))
        .await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(error_from(resp).await)
    }
}

pub async fn upload(
    ctx: &AccountCtx,
    base: &str,
    req: &UploadRequest,
    resume: Option<String>,
    sink: &dyn UploadSink,
    chunk_size: usize,
) -> Result<UploadOutcome> {
    let size = tokio::fs::metadata(&req.draft.file_path)
        .await
        .map_err(|_| AppError::FileUnavailable)?
        .len();
    if size != req.size_bytes {
        return Err(AppError::FileUnavailable);
    }

    let (session, offset) = match resume {
        Some(s) => match server_state(ctx, &s, req.size_bytes).await? {
            ServerState::Done(id) => (s, Err(id)),
            ServerState::Resume(n) => (s, Ok(n)),
        },
        None => {
            let s = start_session(ctx, base, req).await?;
            sink.session_started(&s).await?;
            (s, Ok(0))
        }
    };
    let video_id = match offset {
        Err(id) => id,
        Ok(n) => send_from(ctx, &session, req, n, chunk_size, sink).await?,
    };

    // The video exists now: the rest is best effort and only produces warnings.
    let mut warnings = Vec::new();
    if let Some(t) = &req.draft.thumbnail_path {
        if let Err(e) = set_thumbnail(ctx, base, &video_id, t).await {
            tracing::warn!(error = %e, "thumbnail not set");
            warnings.push(
                match e {
                    // Custom thumbnails need a phone-verified channel.
                    AppError::ScopesDenied | AppError::Internal => "thumbnail_not_allowed",
                    AppError::QuotaExceeded => "thumbnail_quota",
                    _ => "thumbnail_failed",
                }
                .to_owned(),
            );
        }
    }
    if let Some(p) = &req.draft.playlist_id {
        if let Err(e) = add_to_playlist(ctx, base, &video_id, p).await {
            tracing::warn!(error = %e, "not added to playlist");
            warnings.push(
                match e {
                    AppError::ScopesDenied => "playlist_scope",
                    AppError::QuotaExceeded => "playlist_quota",
                    _ => "playlist_failed",
                }
                .to_owned(),
            );
        }
    }
    Ok(UploadOutcome {
        external_id: video_id,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{ThumbnailFacts, UploadDraft};
    use crate::media::MediaInfo;

    fn draft() -> UploadDraft {
        UploadDraft {
            file_path: "/tmp/x.mp4".into(),
            title: "Mi vídeo".into(),
            description: String::new(),
            tags: vec!["minecraft".into()],
            visibility: "private".into(),
            category_id: None,
            playlist_id: None,
            thumbnail_path: None,
            made_for_kids: false,
        }
    }

    fn facts() -> FileFacts {
        FileFacts {
            size_bytes: 1000,
            extension: "mp4".into(),
            media: Some(MediaInfo {
                duration_s: Some(30.0),
                width: Some(1080),
                height: Some(1920),
                video_codec: Some("h264".into()),
                audio_codec: None,
                container: None,
            }),
            thumbnail: None,
        }
    }

    fn codes(issues: &[Issue]) -> Vec<String> {
        issues
            .iter()
            .map(|i| format!("{}.{}", i.field, i.code))
            .collect()
    }

    #[test]
    fn a_valid_private_upload_has_no_issues() {
        assert!(check(&draft(), &facts()).is_empty());
    }

    #[test]
    fn flags_form_problems() {
        let mut d = draft();
        d.title = "a".repeat(101);
        d.description = "<b>".into();
        d.tags = vec!["x".repeat(400), "y".repeat(120)];
        d.visibility = "public".into();
        let c = codes(&check(&d, &facts()));
        assert!(c.contains(&"title.too_long".into()));
        assert!(c.contains(&"description.invalid_chars".into()));
        assert!(c.contains(&"tags.too_long".into()));
        assert!(c.contains(&"visibility.private_until_audit".into()));
    }

    #[test]
    fn flags_file_problems() {
        let mut f = facts();
        f.extension = "txt".into();
        f.media.as_mut().unwrap().duration_s = Some(20.0 * 60.0);
        f.thumbnail = Some(ThumbnailFacts {
            size_bytes: 3 * 1024 * 1024,
            extension: "gif".into(),
        });
        let c = codes(&check(&draft(), &f));
        assert_eq!(
            c,
            vec![
                "file.format",
                "file.needs_verification",
                "thumbnail.format",
                "thumbnail.too_big"
            ]
        );
        f.media = None;
        assert!(codes(&check(&draft(), &f)).contains(&"file.probe_unavailable".into()));
    }

    #[test]
    fn tags_length_counts_quotes_and_commas() {
        assert_eq!(tags_len(&["ab".into(), "c d".into()]), 2 + 5 + 1);
        assert_eq!(tags_len(&[]), 0);
    }
}
