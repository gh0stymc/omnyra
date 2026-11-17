//! Upload queue (step 1d): validation, persistence in `publications`, and the runner that
//! uploads one publication with resume and retries.
//!
//! Statuses: `queued` → `uploading` → `published` | `failed` | `cancelled`. Transient
//! failures (network, 5xx, rate limits) go back to `queued` with `retry_at` and
//! exponential backoff, up to [`MAX_ATTEMPTS`]. The resumable session URI lives in the
//! keyring (`upload/<publication_id>`, H6), so an upload interrupted by a crash or a
//! closed laptop continues where it stopped.

use crate::accounts::account_ctx;
use crate::auth::AuthService;
use crate::clock::Clock;
use crate::connectors::{
    kind, Connector, Content, FileFacts, Issue, Severity, ThumbnailFacts, UploadDraft,
    UploadRequest, UploadSink,
};
use crate::db::repo::{accounts, contents};
use crate::media::{self, MediaInfo};
use crate::quota::{DbQuota, Purpose};
use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::path::Path;
use std::sync::{Arc, Mutex};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

pub const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_BASE_S: i64 = 30;
const BACKOFF_MAX_S: i64 = 15 * 60;

fn session_key(publication_id: &str) -> String {
    format!("upload/{publication_id}")
}

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Internal)
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "ts" | "mts" | "m2ts" => "video/mp2t",
        "ogv" => "video/ogg",
        _ => "application/octet-stream",
    }
}

// ---- validation ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
    pub media: Option<MediaInfo>,
    /// f64 because TypeScript numbers cannot hold every u64 (exact up to 8 PiB).
    pub size_bytes: Option<f64>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

/// Checks the files (existence, size, ffprobe) and then the platform's rules.
pub async fn validate(connector: &dyn Connector, draft: &UploadDraft) -> ValidationReport {
    let meta = match tokio::fs::metadata(&draft.file_path).await {
        Ok(m) if m.is_file() => m,
        _ => {
            return ValidationReport {
                issues: vec![Issue::error("file", "missing")],
                media: None,
                size_bytes: None,
            }
        }
    };
    let mut issues = Vec::new();
    let media = match media::probe(Path::new(&draft.file_path)).await {
        Ok(m) => Some(m),
        Err(AppError::Validation { .. }) => {
            issues.push(Issue::error("file", "unreadable"));
            None
        }
        // ffprobe missing or failing: the platform check warns about it.
        Err(_) => None,
    };
    let thumbnail = match &draft.thumbnail_path {
        Some(p) => match tokio::fs::metadata(p).await {
            Ok(m) if m.is_file() => Some(ThumbnailFacts {
                size_bytes: m.len(),
                extension: extension(p),
            }),
            _ => {
                issues.push(Issue::error("thumbnail", "missing"));
                None
            }
        },
        None => None,
    };
    let facts = FileFacts {
        size_bytes: meta.len(),
        extension: extension(&draft.file_path),
        media: media.clone(),
        thumbnail,
    };
    let unreadable = !issues.is_empty() && media.is_none();
    issues.extend(
        connector
            .check_upload(draft, &facts)
            .into_iter()
            // "No ffprobe" is noise when ffprobe did run and rejected the file.
            .filter(|i| !(unreadable && i.code == "probe_unavailable")),
    );
    ValidationReport {
        issues,
        media,
        size_bytes: Some(meta.len() as f64),
    }
}

// ---- queue ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadRow {
    pub id: String,
    pub account_id: String,
    pub title: Option<String>,
    pub visibility: Option<String>,
    pub status: String,
    pub progress: Option<f64>,
    pub attempts: u32,
    pub retry_at: Option<String>,
    pub error_code: Option<String>,
    pub warnings: Vec<String>,
    pub external_id: Option<String>,
    pub created_at: String,
}

fn insert_file(
    conn: &Connection,
    path: &str,
    size: u64,
    info: Option<&MediaInfo>,
    now: &str,
) -> Result<String> {
    let id = uuid::Uuid::now_v7().to_string();
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    conn.execute(
        "INSERT INTO files (id, path, managed, original_name, mime, size_bytes, media_info,
                            created_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            path,
            name,
            mime_for(&extension(path)),
            size as i64,
            info.and_then(|m| serde_json::to_string(m).ok()),
            now
        ],
    )?;
    Ok(id)
}

/// Validates again (the form may be stale) and queues the upload.
pub async fn enqueue(
    db: &Mutex<Connection>,
    clock: &dyn Clock,
    connector: &dyn Connector,
    account_id: &str,
    draft: &UploadDraft,
) -> Result<String> {
    let account = accounts::get(&*lock(db)?, account_id)?;
    if account.status != accounts::AccountStatus::Active {
        return Err(AppError::NotConnected);
    }
    let needed = connector.upload_scopes(draft.playlist_id.is_some());
    if !needed.iter().all(|s| account.granted_scopes.contains(s)) {
        return Err(AppError::ScopesDenied);
    }
    let report = validate(connector, draft).await;
    if let Some(e) = report.issues.iter().find(|i| i.severity == Severity::Error) {
        return Err(AppError::Validation {
            field: e.field.clone(),
        });
    }
    let size = report.size_bytes.ok_or(AppError::FileUnavailable)? as u64;
    let now = clock.now_iso();
    let id = uuid::Uuid::now_v7().to_string();
    let mut conn = lock(db)?;
    let tx = conn.transaction()?;
    let source = insert_file(&tx, &draft.file_path, size, report.media.as_ref(), &now)?;
    let thumb = match &draft.thumbnail_path {
        Some(p) => {
            let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            Some(insert_file(&tx, p, len, None, &now)?)
        }
        None => None,
    };
    let options = json!({
        "categoryId": draft.category_id,
        "playlistId": draft.playlist_id,
        "madeForKids": draft.made_for_kids,
    });
    tx.execute(
        "INSERT INTO publications (id, account_id, source_file_id, thumbnail_file_id, title,
                                   description, tags, visibility, platform_options, status,
                                   schedule_mode, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'queued', 'now', ?10, ?10)",
        params![
            id,
            account_id,
            source,
            thumb,
            draft.title.trim(),
            draft.description,
            serde_json::to_string(&draft.tags).map_err(|_| AppError::Internal)?,
            draft.visibility,
            options.to_string(),
            now
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

const ROW_COLUMNS: &str = "id, account_id, title, visibility, status, progress, attempts,
                           retry_at, error_code, warnings, external_id, created_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<UploadRow> {
    let warnings: String = r.get(9)?;
    Ok(UploadRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        title: r.get(2)?,
        visibility: r.get(3)?,
        status: r.get(4)?,
        progress: r.get(5)?,
        attempts: r.get(6)?,
        retry_at: r.get(7)?,
        error_code: r.get(8)?,
        warnings: serde_json::from_str(&warnings).unwrap_or_default(),
        external_id: r.get(10)?,
        created_at: r.get(11)?,
    })
}

/// Uploads with a direct file upload (not drafts or scheduled posts of later steps),
/// newest first.
pub fn list(conn: &Connection) -> Result<Vec<UploadRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ROW_COLUMNS} FROM publications WHERE source_file_id IS NOT NULL
         ORDER BY created_at DESC LIMIT 100"
    ))?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<UploadRow> {
    conn.query_row(
        &format!("SELECT {ROW_COLUMNS} FROM publications WHERE id = ?1"),
        params![id],
        row,
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

/// Uploads left `uploading` by a crash go back to the queue (they resume from the server).
pub fn recover(conn: &Connection, now: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE publications SET status = 'queued', updated_at = ?1 WHERE status = 'uploading'",
        params![now],
    )?)
}

/// Oldest queued upload whose retry time has come.
pub fn next_due(conn: &Connection, now: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM publications
             WHERE status = 'queued' AND (retry_at IS NULL OR retry_at <= ?1)
             ORDER BY created_at LIMIT 1",
            params![now],
            |r| r.get(0),
        )
        .optional()?)
}

/// Earliest pending retry, so the runner knows how long it may sleep.
pub fn next_retry_at(conn: &Connection) -> Result<Option<String>> {
    Ok(conn.query_row(
        "SELECT MIN(retry_at) FROM publications WHERE status = 'queued' AND retry_at IS NOT NULL",
        [],
        |r| r.get(0),
    )?)
}

fn set_status(conn: &Connection, id: &str, status: &str, now: &str) -> Result<()> {
    conn.execute(
        "UPDATE publications SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, now],
    )?;
    Ok(())
}

pub fn set_progress(conn: &Connection, id: &str, progress: f64) -> Result<()> {
    conn.execute(
        "UPDATE publications SET progress = ?2 WHERE id = ?1",
        params![id, progress],
    )?;
    Ok(())
}

/// Cancels a `queued` or `uploading` upload; returns whether something changed. The
/// runner notices the cancellation and removes the session secret.
pub fn cancel(conn: &Connection, id: &str, now: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE publications SET status = 'cancelled', updated_at = ?2
         WHERE id = ?1 AND status IN ('queued', 'uploading')",
        params![id, now],
    )? > 0)
}

/// Failed or cancelled uploads go back to the queue with a fresh attempt count.
pub fn retry(conn: &Connection, id: &str, now: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE publications SET status = 'queued', attempts = 0, retry_at = NULL,
                error_code = NULL, updated_at = ?2
         WHERE id = ?1 AND status IN ('failed', 'cancelled')",
        params![id, now],
    )? > 0)
}

/// Removes a finished, failed or cancelled upload from the list (the video file is never
/// touched).
pub fn remove(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM publications WHERE id = ?1
         AND status IN ('published', 'failed', 'cancelled')",
        params![id],
    )? > 0)
}

// ---- runner --------------------------------------------------------------------------

/// Where the runner reports what happens (Tauri events in the app).
pub trait UploadEvents: Send + Sync {
    fn progress(&self, publication_id: &str, sent: u64, total: u64);
    fn changed(&self, publication_id: &str, status: &str);
}

struct Sink<'a> {
    auth: &'a AuthService,
    db: &'a Mutex<Connection>,
    id: &'a str,
    events: &'a dyn UploadEvents,
    last_saved: Mutex<f64>,
}

#[async_trait::async_trait]
impl UploadSink for Sink<'_> {
    async fn session_started(&self, uri: &str) -> Result<()> {
        self.auth.set_secret(&session_key(self.id), uri).await
    }

    fn progress(&self, sent: u64, total: u64) {
        self.events.progress(self.id, sent, total);
        let p = if total == 0 {
            1.0
        } else {
            sent as f64 / total as f64
        };
        // Persist every 5 % so the list shows something sensible after a restart.
        let mut last = self.last_saved.lock().unwrap_or_else(|e| e.into_inner());
        if p - *last >= 0.05 || p >= 1.0 {
            *last = p;
            if let Ok(c) = self.db.lock() {
                let _ = set_progress(&c, self.id, p);
            }
        }
    }
}

struct Loaded {
    account_id: String,
    req: UploadRequest,
    attempts: u32,
}

fn load(conn: &Connection, id: &str) -> Result<Loaded> {
    conn.query_row(
        "SELECT p.account_id, p.title, p.description, p.tags, p.visibility,
                p.platform_options, p.attempts, f.path, f.size_bytes, t.path
         FROM publications p
         JOIN files f ON f.id = p.source_file_id
         LEFT JOIN files t ON t.id = p.thumbnail_file_id
         WHERE p.id = ?1",
        params![id],
        |r| {
            let tags: String = r.get(3)?;
            let opts: String = r.get(5)?;
            let opts: serde_json::Value = serde_json::from_str(&opts).unwrap_or_default();
            let path: String = r.get(7)?;
            let size: i64 = r.get(8)?;
            Ok(Loaded {
                account_id: r.get(0)?,
                attempts: r.get(6)?,
                req: UploadRequest {
                    mime: mime_for(&extension(&path)).into(),
                    size_bytes: size as u64,
                    draft: UploadDraft {
                        file_path: path,
                        title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        description: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        tags: serde_json::from_str(&tags).unwrap_or_default(),
                        visibility: r
                            .get::<_, Option<String>>(4)?
                            .unwrap_or_else(|| "private".into()),
                        category_id: opts["categoryId"].as_str().map(str::to_owned),
                        playlist_id: opts["playlistId"].as_str().map(str::to_owned),
                        thumbnail_path: r.get(9)?,
                        made_for_kids: opts["madeForKids"].as_bool().unwrap_or(false),
                    },
                },
            })
        },
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

fn error_code(e: &AppError) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v["code"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "INTERNAL".into())
}

fn transient(e: &AppError) -> bool {
    matches!(e, AppError::Network | AppError::RateLimited { .. })
}

fn backoff(attempt: u32) -> Duration {
    let s = BACKOFF_BASE_S.saturating_mul(1 << attempt.saturating_sub(1).min(10));
    Duration::seconds(s.min(BACKOFF_MAX_S))
}

/// Uploads one queued publication. Errors are recorded on the publication; the returned
/// error is only for logging.
pub async fn run(
    db: &Arc<Mutex<Connection>>,
    auth: &Arc<AuthService>,
    clock: &Arc<dyn Clock>,
    connector: &dyn Connector,
    id: &str,
    events: &dyn UploadEvents,
) -> Result<()> {
    let (loaded, account) = {
        let conn = lock(db)?;
        let l = load(&conn, id)?;
        let a = accounts::get(&conn, &l.account_id)?;
        set_status(&conn, id, "uploading", &clock.now_iso())?;
        conn.execute(
            "UPDATE publications SET attempts = attempts + 1, retry_at = NULL WHERE id = ?1",
            params![id],
        )?;
        (l, a)
    };
    events.changed(id, "uploading");
    let attempt = loaded.attempts + 1;

    let quota = Arc::new(DbQuota::new(
        db.clone(),
        clock.clone(),
        &account.platform,
        Purpose::Upload,
    ));
    let ctx = account_ctx(auth, connector, &account, quota);
    let sink = Sink {
        auth,
        db,
        id,
        events,
        last_saved: Mutex::new(0.0),
    };
    let key = session_key(id);

    let mut result = if account.status != accounts::AccountStatus::Active {
        Err(AppError::NotConnected)
    } else {
        let resume = auth.get_secret(&key).await?;
        let r = connector
            .upload(&ctx, &loaded.req, resume.clone(), &sink)
            .await;
        match r {
            // The old session is gone: start over once, right away.
            Err(AppError::UploadSessionExpired) if resume.is_some() => {
                auth.delete_secret(&key).await?;
                connector.upload(&ctx, &loaded.req, None, &sink).await
            }
            other => other,
        }
    };

    // Cancelled while uploading: leave it cancelled whatever happened.
    if lock(db).and_then(|c| get(&c, id))?.status == "cancelled" {
        let _ = auth.delete_secret(&key).await;
        return Ok(());
    }

    let now = clock.now();
    let now_iso = now.format(&Rfc3339).map_err(|_| AppError::Internal)?;
    match &mut result {
        Ok(out) => {
            auth.delete_secret(&key).await?;
            let mut conn = lock(db)?;
            let tx = conn.transaction()?;
            // Show it among the account's contents right away; the next sync completes it.
            let content = Content {
                external_id: out.external_id.clone(),
                kind: kind::VIDEO.into(),
                title: Some(loaded.req.draft.title.trim().to_owned()),
                description: Some(loaded.req.draft.description.clone()),
                url: None,
                thumbnail_url: None,
                published_at: Some(now_iso.clone()),
                duration_s: None,
                visibility: Some(loaded.req.draft.visibility.clone()),
                extra: Default::default(),
            };
            let content_id =
                contents::upsert(&tx, &account.id, &content, kind::VIDEO, false, &now_iso)?;
            tx.execute(
                "UPDATE publications SET status = 'published', progress = 1, error_code = NULL,
                        external_id = ?2, published_content_id = ?3, warnings = ?4,
                        updated_at = ?5
                 WHERE id = ?1",
                params![
                    id,
                    out.external_id,
                    content_id,
                    serde_json::to_string(&out.warnings).map_err(|_| AppError::Internal)?,
                    now_iso
                ],
            )?;
            tx.commit()?;
            events.changed(id, "published");
            Ok(())
        }
        Err(e) => {
            let code = error_code(e);
            let conn = lock(db)?;
            if transient(e) && attempt < MAX_ATTEMPTS {
                let wait = match e {
                    AppError::RateLimited { retry_after_s } => {
                        Duration::seconds(i64::from(*retry_after_s)).max(backoff(attempt))
                    }
                    _ => backoff(attempt),
                };
                let at = (now + wait)
                    .format(&Rfc3339)
                    .map_err(|_| AppError::Internal)?;
                conn.execute(
                    "UPDATE publications SET status = 'queued', retry_at = ?2, error_code = ?3,
                            updated_at = ?4 WHERE id = ?1",
                    params![id, at, code, now_iso],
                )?;
                events.changed(id, "queued");
            } else {
                conn.execute(
                    "UPDATE publications SET status = 'failed', error_code = ?2, updated_at = ?3
                     WHERE id = ?1",
                    params![id, code, now_iso],
                )?;
                events.changed(id, "failed");
            }
            tracing::warn!(error = %e, attempt, "upload attempt failed");
            Err(std::mem::replace(e, AppError::Internal))
        }
    }
}

/// Small helper for the app: whether `at` (RFC 3339) is in the past.
pub fn is_due(at: &str, now: OffsetDateTime) -> bool {
    OffsetDateTime::parse(at, &Rfc3339).map_or(true, |t| t <= now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{MemoryStore, TokenStore};
    use crate::clock::FixedClock;
    use crate::connectors::mock::MockConnector;
    use crate::connectors::{
        AccountCtx, ChannelSnapshot, ContentDetails, ContentRef, DateRange, MetricPoint, Page,
        PlatformInfo, Profile, UploadOutcome,
    };
    use crate::db::{open_in_memory, repo::accounts::NewAccount};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};
    use time::macros::datetime;

    #[derive(Default)]
    struct Events(Mutex<Vec<String>>);
    impl UploadEvents for Events {
        fn progress(&self, _: &str, _: u64, _: u64) {}
        fn changed(&self, _: &str, status: &str) {
            self.0.lock().unwrap().push(status.to_owned());
        }
    }

    /// Mock connector whose uploads fail with `err` the first `fail_times` calls.
    struct Flaky {
        inner: MockConnector,
        fail_times: u32,
        calls: AtomicU32,
        err: fn() -> AppError,
        resumes: Mutex<Vec<Option<String>>>,
    }

    #[async_trait]
    impl Connector for Flaky {
        fn info(&self) -> &PlatformInfo {
            self.inner.info()
        }
        async fn fetch_profile(&self, c: &AccountCtx) -> Result<Profile> {
            self.inner.fetch_profile(c).await
        }
        async fn fetch_channel_snapshot(&self, c: &AccountCtx) -> Result<ChannelSnapshot> {
            self.inner.fetch_channel_snapshot(c).await
        }
        async fn fetch_channel_metrics(
            &self,
            c: &AccountCtx,
            r: DateRange,
        ) -> Result<Vec<MetricPoint>> {
            self.inner.fetch_channel_metrics(c, r).await
        }
        async fn list_contents(
            &self,
            c: &AccountCtx,
            p: Option<String>,
        ) -> Result<Page<ContentRef>> {
            self.inner.list_contents(c, p).await
        }
        async fn fetch_content_details(
            &self,
            c: &AccountCtx,
            ids: &[String],
        ) -> Result<Vec<ContentDetails>> {
            self.inner.fetch_content_details(c, ids).await
        }
        async fn upload(
            &self,
            c: &AccountCtx,
            req: &UploadRequest,
            resume: Option<String>,
            sink: &dyn UploadSink,
        ) -> Result<UploadOutcome> {
            self.resumes.lock().unwrap().push(resume.clone());
            if self.calls.fetch_add(1, Ordering::SeqCst) < self.fail_times {
                if resume.is_none() {
                    sink.session_started("sess-1").await?;
                }
                return Err((self.err)());
            }
            self.inner.upload(c, req, resume, sink).await
        }
    }

    fn flaky(fail_times: u32, err: fn() -> AppError) -> Flaky {
        Flaky {
            inner: MockConnector::new(),
            fail_times,
            calls: AtomicU32::new(0),
            err,
            resumes: Mutex::new(Vec::new()),
        }
    }

    struct Env {
        db: Arc<Mutex<Connection>>,
        store: Arc<MemoryStore>,
        auth: Arc<AuthService>,
        clock: Arc<dyn Clock>,
        id: String,
    }

    fn env() -> Env {
        let conn = open_in_memory().unwrap();
        let acc = accounts::upsert(
            &conn,
            "acc-1",
            &NewAccount {
                platform: "mock",
                external_id: "m1",
                handle: None,
                display_name: "M",
                granted_scopes: &[],
            },
            "t0",
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("omnyra-up-{}.mp4", uuid::Uuid::now_v7()));
        std::fs::write(&path, b"0123456789").unwrap();
        let file = insert_file(&conn, &path.to_string_lossy(), 10, None, "t0").unwrap();
        conn.execute(
            "INSERT INTO publications (id, account_id, source_file_id, title, visibility, status,
                                       created_at, updated_at)
             VALUES ('pub-1', ?1, ?2, 'Hola', 'private', 'queued', 't0', 't0')",
            params![acc.id, file],
        )
        .unwrap();
        let store = Arc::new(MemoryStore::new());
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(datetime!(2026-11-17 12:00 UTC)));
        Env {
            db: Arc::new(Mutex::new(conn)),
            auth: Arc::new(AuthService::new(
                store.clone(),
                reqwest::Client::new(),
                clock.clone(),
            )),
            store,
            clock,
            id: "pub-1".into(),
        }
    }

    fn row(e: &Env) -> UploadRow {
        get(&e.db.lock().unwrap(), &e.id).unwrap()
    }

    #[tokio::test]
    async fn successful_upload_is_published_and_listed_as_content() {
        let e = env();
        let ev = Events::default();
        run(
            &e.db,
            &e.auth,
            &e.clock,
            &flaky(0, || AppError::Network),
            &e.id,
            &ev,
        )
        .await
        .unwrap();
        let r = row(&e);
        assert_eq!(r.status, "published");
        assert_eq!(r.external_id.as_deref(), Some("mock-upload-10"));
        assert_eq!(r.progress, Some(1.0));
        assert_eq!(*ev.0.lock().unwrap(), vec!["uploading", "published"]);
        let n: i64 =
            e.db.lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM contents WHERE external_id = 'mock-upload-10'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(n, 1);
        assert!(e.store.get_secret("upload/pub-1").unwrap().is_none());
    }

    #[tokio::test]
    async fn transient_errors_retry_with_backoff_then_fail() {
        let e = env();
        let c = flaky(u32::MAX, || AppError::Network);
        let ev = Events::default();
        assert!(run(&e.db, &e.auth, &e.clock, &c, &e.id, &ev).await.is_err());
        let r = row(&e);
        assert_eq!(r.status, "queued");
        assert_eq!(r.attempts, 1);
        assert_eq!(r.retry_at.as_deref(), Some("2026-11-17T12:00:30Z"));
        assert_eq!(r.error_code.as_deref(), Some("NETWORK"));
        // Not due yet; the session is kept for resuming.
        assert_eq!(
            next_due(&e.db.lock().unwrap(), "2026-11-17T12:00:10Z").unwrap(),
            None
        );
        assert_eq!(
            e.store.get_secret("upload/pub-1").unwrap().as_deref(),
            Some("sess-1")
        );
        for _ in 1..MAX_ATTEMPTS {
            let _ = run(&e.db, &e.auth, &e.clock, &c, &e.id, &ev).await;
        }
        assert_eq!(row(&e).status, "failed");
        // Every retry resumed the same session.
        assert!(c.resumes.lock().unwrap()[1..]
            .iter()
            .all(|r| r.as_deref() == Some("sess-1")));
    }

    #[tokio::test]
    async fn permanent_errors_fail_immediately_and_can_be_retried() {
        let e = env();
        let ev = Events::default();
        let c = flaky(1, || AppError::QuotaExceeded);
        let _ = run(&e.db, &e.auth, &e.clock, &c, &e.id, &ev).await;
        let r = row(&e);
        assert_eq!(
            (r.status.as_str(), r.error_code.as_deref()),
            ("failed", Some("QUOTA_EXCEEDED"))
        );
        assert!(retry(&e.db.lock().unwrap(), &e.id, "t1").unwrap());
        assert_eq!(row(&e).attempts, 0);
        run(&e.db, &e.auth, &e.clock, &c, &e.id, &ev).await.unwrap();
        assert_eq!(row(&e).status, "published");
        assert!(remove(&e.db.lock().unwrap(), &e.id).unwrap());
    }

    #[tokio::test]
    async fn expired_session_restarts_from_scratch_once() {
        let e = env();
        e.store.set_secret("upload/pub-1", "old-session").unwrap();
        let c = flaky(1, || AppError::UploadSessionExpired);
        run(&e.db, &e.auth, &e.clock, &c, &e.id, &Events::default())
            .await
            .unwrap();
        assert_eq!(
            *c.resumes.lock().unwrap(),
            vec![Some("old-session".to_string()), None]
        );
        assert_eq!(row(&e).status, "published");
    }

    #[tokio::test]
    async fn cancelled_queue_entries_are_not_picked() {
        let e = env();
        let conn = e.db.lock().unwrap();
        assert_eq!(next_due(&conn, "t9").unwrap().as_deref(), Some("pub-1"));
        assert!(cancel(&conn, &e.id, "t1").unwrap());
        assert!(!cancel(&conn, &e.id, "t1").unwrap());
        assert_eq!(next_due(&conn, "t9").unwrap(), None);
    }

    #[tokio::test]
    async fn crash_recovery_requeues_uploading() {
        let e = env();
        let conn = e.db.lock().unwrap();
        set_status(&conn, &e.id, "uploading", "t1").unwrap();
        assert_eq!(recover(&conn, "t2").unwrap(), 1);
        assert_eq!(next_due(&conn, "t9").unwrap().as_deref(), Some("pub-1"));
    }

    #[tokio::test]
    async fn enqueue_requires_the_upload_scope() {
        let e = env();
        let yt = crate::connectors::youtube::YouTubeConnector::new(None);
        e.db.lock()
            .unwrap()
            .execute("UPDATE accounts SET platform = 'youtube'", [])
            .unwrap();
        let draft = UploadDraft {
            file_path: "/nope.mp4".into(),
            title: "t".into(),
            description: String::new(),
            tags: vec![],
            visibility: "private".into(),
            category_id: None,
            playlist_id: None,
            thumbnail_path: None,
            made_for_kids: false,
        };
        assert!(matches!(
            enqueue(&e.db, e.clock.as_ref(), &yt, "acc-1", &draft).await,
            Err(AppError::ScopesDenied)
        ));
    }

    #[tokio::test]
    async fn validation_reports_missing_file_and_bad_title() {
        let mock = MockConnector::new();
        let mut draft = UploadDraft {
            file_path: "/definitely/not/here.mp4".into(),
            title: String::new(),
            description: String::new(),
            tags: vec![],
            visibility: "private".into(),
            category_id: None,
            playlist_id: None,
            thumbnail_path: None,
            made_for_kids: false,
        };
        let r = validate(&mock, &draft).await;
        assert_eq!(r.issues, vec![Issue::error("file", "missing")]);
        let path = std::env::temp_dir().join(format!("omnyra-v-{}.txt", uuid::Uuid::now_v7()));
        std::fs::write(&path, b"not a video").unwrap();
        draft.file_path = path.to_string_lossy().into();
        let r = validate(&mock, &draft).await;
        assert!(r.has_errors());
        assert!(r.issues.contains(&Issue::error("title", "required")));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::seconds(30));
        assert_eq!(backoff(2), Duration::seconds(60));
        assert_eq!(backoff(4), Duration::seconds(240));
        assert_eq!(backoff(20), Duration::seconds(BACKOFF_MAX_S));
    }
}
