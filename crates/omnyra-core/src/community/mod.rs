//! Community (phase 4): unified comment inbox across networks.
//!
//! - **Sync:** account-wide listing when the platform has one (YouTube
//!   `commentThreads`), otherwise the first page of comments of the most recent contents
//!   (Instagram). Stops at the first page with nothing new.
//! - **Model:** one row per comment (`comments`), replies linked to their top-level
//!   comment; the creator's own replies mark the parent as `replied`.
//! - **Replies** are sent through the connector, stored right away and audited.

pub mod classify;
pub mod cluster;

use crate::accounts::account_ctx;
use crate::auth::AuthService;
use crate::clock::Clock;
use crate::connectors::{Comment, CommentSupport, Connector};
use crate::db::repo::{accounts, audit_log};
use crate::quota::{DbQuota, Purpose};
use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::sync::{Arc, Mutex};

/// Pages read per sync when every comment is new (100 comments each on YouTube).
const MAX_PAGES: usize = 5;
/// Contents scanned on platforms without an account-wide listing.
const RECENT_CONTENTS: usize = 20;
pub const STATUSES: &[&str] = &["pending", "replied", "archived"];
const MAX_REPLY_CHARS: usize = 10_000;

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Internal)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRow {
    pub id: String,
    pub author_name: Option<String>,
    pub text: String,
    pub published_at: Option<String>,
    pub is_own: bool,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommentRow {
    pub id: String,
    pub account_id: String,
    pub platform: String,
    pub content_id: Option<String>,
    pub content_title: Option<String>,
    pub content_url: Option<String>,
    pub author_name: Option<String>,
    pub author_external_id: Option<String>,
    pub text: String,
    pub published_at: Option<String>,
    pub like_count: Option<u32>,
    pub status: String,
    pub category: Option<String>,
    pub category_source: Option<String>,
    pub phishing_score: Option<f64>,
    pub phishing_reasons: Vec<String>,
    pub cluster_id: Option<String>,
    pub replies: Vec<ReplyRow>,
}

#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommentFilter {
    pub platform: Option<String>,
    pub account_id: Option<String>,
    pub content_id: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub cluster_id: Option<String>,
    /// Case-insensitive substring of the text or the author.
    pub search: Option<String>,
    /// Only comments flagged as possible phishing (6c).
    pub suspicious: Option<bool>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommentSyncReport {
    pub fetched: u32,
    pub new: u32,
}

/// Stores a batch from the connector. Top-level comments go first so replies can link
/// to them. Returns the IDs of the comments that were new.
pub fn upsert_comments(
    conn: &Connection,
    account_id: &str,
    comments: &[Comment],
    now: &str,
) -> Result<Vec<String>> {
    let mut ordered: Vec<&Comment> = comments.iter().collect();
    ordered.sort_by_key(|c| c.parent_external_id.is_some());
    let mut new_ids = Vec::new();
    for c in ordered {
        let content_id: Option<String> = match &c.content_external_id {
            Some(ext) => conn
                .query_row(
                    "SELECT id FROM contents WHERE account_id = ?1 AND external_id = ?2",
                    params![account_id, ext],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let parent_id: Option<String> = match &c.parent_external_id {
            Some(ext) => conn
                .query_row(
                    "SELECT id FROM comments WHERE account_id = ?1 AND external_id = ?2",
                    params![account_id, ext],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM comments WHERE account_id = ?1 AND external_id = ?2",
                params![account_id, c.external_id],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE comments SET text = ?2, like_count = ?3, fetched_at = ?4,
                            content_id = COALESCE(content_id, ?5)
                     WHERE id = ?1",
                    params![id, c.text, c.like_count, now, content_id],
                )?;
            }
            None => {
                let id = uuid::Uuid::now_v7().to_string();
                // The creator's own comments never need an answer.
                let status = if c.is_own { "replied" } else { "pending" };
                conn.execute(
                    "INSERT INTO comments (id, account_id, content_id, external_id, parent_id,
                                           author_name, author_external_id, is_own, text,
                                           published_at, like_count, status, fetched_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        id,
                        account_id,
                        content_id,
                        c.external_id,
                        parent_id,
                        c.author_name,
                        c.author_external_id,
                        c.is_own,
                        c.text,
                        c.published_at,
                        c.like_count,
                        status,
                        now
                    ],
                )?;
                new_ids.push(id);
            }
        }
        if c.is_own {
            if let Some(p) = &parent_id {
                conn.execute(
                    "UPDATE comments SET status = 'replied' WHERE id = ?1 AND status = 'pending'",
                    params![p],
                )?;
            }
        }
    }
    Ok(new_ids)
}

fn known_all(conn: &Connection, account_id: &str, page: &[Comment]) -> Result<bool> {
    for c in page {
        let found: Option<i32> = conn
            .query_row(
                "SELECT 1 FROM comments WHERE account_id = ?1 AND external_id = ?2",
                params![account_id, c.external_id],
                |r| r.get(0),
            )
            .optional()?;
        if found.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Fetches new comments of one account and stores them (classified, clustered and
/// checked for phishing).
pub async fn sync(
    db: &Arc<Mutex<Connection>>,
    auth: &Arc<AuthService>,
    clock: &Arc<dyn Clock>,
    connector: &dyn Connector,
    account_id: &str,
) -> Result<CommentSyncReport> {
    if connector.info().capabilities.comments == CommentSupport::None {
        return Err(AppError::NotSupported);
    }
    let account = accounts::get(&*lock(db)?, account_id)?;
    if account.status != accounts::AccountStatus::Active {
        return Err(AppError::NotConnected);
    }
    let quota = Arc::new(DbQuota::new(
        db.clone(),
        clock.clone(),
        &account.platform,
        Purpose::Sync,
    ));
    let ctx = account_ctx(auth, connector, &account, quota);
    let mut report = CommentSyncReport::default();
    let mut new_ids = Vec::new();
    let mut store = |page: &[Comment]| -> Result<bool> {
        let conn = lock(db)?;
        let all_known = known_all(&conn, account_id, page)?;
        let ids = upsert_comments(&conn, account_id, page, &clock.now_iso())?;
        report.fetched += page.len() as u32;
        report.new += ids.len() as u32;
        new_ids.extend(ids);
        Ok(all_known)
    };

    match connector.list_account_comments(&ctx, None).await {
        Ok(first) => {
            let mut next = first.next.clone();
            let stop = store(&first.items)?;
            if !stop {
                for _ in 1..MAX_PAGES {
                    let Some(token) = next.take() else { break };
                    let page = connector.list_account_comments(&ctx, Some(token)).await?;
                    next = page.next.clone();
                    if store(&page.items)? {
                        break;
                    }
                }
            }
        }
        Err(AppError::NotSupported) => {
            let recent: Vec<String> = {
                let conn = lock(db)?;
                let mut st = conn.prepare(
                    "SELECT external_id FROM contents WHERE account_id = ?1
                     ORDER BY published_at DESC LIMIT ?2",
                )?;
                let rows =
                    st.query_map(params![account_id, RECENT_CONTENTS as i64], |r| r.get(0))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            for ext in recent {
                let page = connector.list_comments(&ctx, &ext, None).await?;
                store(&page.items)?;
            }
        }
        Err(e) => return Err(e),
    }

    let conn = lock(db)?;
    classify::apply(&conn, &new_ids)?;
    crate::security::phishing::apply(&conn, &new_ids)?;
    cluster::rebuild(&conn, &clock.now_iso())?;
    Ok(report)
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<CommentRow> {
    let reasons: Option<String> = r.get(15)?;
    Ok(CommentRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        platform: r.get(2)?,
        content_id: r.get(3)?,
        content_title: r.get(4)?,
        content_url: r.get(5)?,
        author_name: r.get(6)?,
        author_external_id: r.get(7)?,
        text: r.get(8)?,
        published_at: r.get(9)?,
        like_count: r.get(10)?,
        status: r.get(11)?,
        category: r.get(12)?,
        category_source: r.get(13)?,
        phishing_score: r.get(14)?,
        phishing_reasons: reasons
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        cluster_id: r.get(16)?,
        replies: Vec::new(),
    })
}

/// Top-level comments matching `f`, newest first, with their replies.
pub fn list(conn: &Connection, f: &CommentFilter) -> Result<Vec<CommentRow>> {
    let mut sql = String::from(
        "SELECT c.id, c.account_id, a.platform, c.content_id, k.title, k.url, c.author_name,
                c.author_external_id, c.text, c.published_at, c.like_count, c.status,
                c.category, c.category_source, c.phishing_score, c.phishing_reasons,
                c.cluster_id
         FROM comments c JOIN accounts a ON a.id = c.account_id
         LEFT JOIN contents k ON k.id = c.content_id
         WHERE c.parent_id IS NULL AND c.is_own = 0",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut add = |clause: &str, v: String| {
        args.push(Box::new(v));
        sql.push_str(&format!(" AND {clause} ?{}", args.len()));
    };
    if let Some(v) = &f.platform {
        add("a.platform =", v.clone());
    }
    if let Some(v) = &f.account_id {
        add("c.account_id =", v.clone());
    }
    if let Some(v) = &f.content_id {
        add("c.content_id =", v.clone());
    }
    if let Some(v) = &f.status {
        add("c.status =", v.clone());
    }
    if let Some(v) = &f.category {
        add("c.category =", v.clone());
    }
    if let Some(v) = &f.cluster_id {
        add("c.cluster_id =", v.clone());
    }
    if let Some(v) = f.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = format!(
            "%{}%",
            v.to_lowercase()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        args.push(Box::new(pattern));
        let n = args.len();
        sql.push_str(&format!(
            " AND (lower(c.text) LIKE ?{n} ESCAPE '\\' OR lower(COALESCE(c.author_name, '')) LIKE ?{n} ESCAPE '\\')"
        ));
    }
    if f.suspicious == Some(true) {
        sql.push_str(" AND c.phishing_score >= 0.5");
    }
    sql.push_str(&format!(
        " ORDER BY c.published_at DESC LIMIT {}",
        f.limit.unwrap_or(200).min(1000)
    ));
    let mut st = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut out: Vec<CommentRow> = st
        .query_map(params.as_slice(), row)?
        .collect::<std::result::Result<_, _>>()?;
    let mut rep = conn.prepare(
        "SELECT id, author_name, text, published_at, is_own FROM comments
         WHERE parent_id = ?1 ORDER BY published_at",
    )?;
    for c in &mut out {
        c.replies = rep
            .query_map(params![c.id], |r| {
                Ok(ReplyRow {
                    id: r.get(0)?,
                    author_name: r.get(1)?,
                    text: r.get(2)?,
                    published_at: r.get(3)?,
                    is_own: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
    }
    Ok(out)
}

pub fn set_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    if !STATUSES.contains(&status) {
        return Err(AppError::Validation {
            field: "status".into(),
        });
    }
    if conn.execute(
        "UPDATE comments SET status = ?2 WHERE id = ?1",
        params![id, status],
    )? == 0
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Type, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    pub pending: u32,
    pub questions: u32,
    pub suspicious: u32,
}

pub fn counts(conn: &Connection) -> Result<InboxCounts> {
    Ok(conn.query_row(
        "SELECT
           COALESCE(SUM(status = 'pending'), 0),
           COALESCE(SUM(status = 'pending' AND category = 'question'), 0),
           COALESCE(SUM(status <> 'archived' AND phishing_score >= 0.5), 0)
         FROM comments WHERE parent_id IS NULL AND is_own = 0",
        [],
        |r| {
            Ok(InboxCounts {
                pending: r.get(0)?,
                questions: r.get(1)?,
                suspicious: r.get(2)?,
            })
        },
    )?)
}

/// Answers a comment on its platform (top-level comment of the thread when replying to
/// a reply, as YouTube and Instagram only nest one level).
pub async fn reply(
    db: &Arc<Mutex<Connection>>,
    auth: &Arc<AuthService>,
    clock: &Arc<dyn Clock>,
    connector: &dyn Connector,
    comment_id: &str,
    text: &str,
) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_REPLY_CHARS {
        return Err(AppError::Validation {
            field: "text".into(),
        });
    }
    if connector.info().capabilities.comments != CommentSupport::ReadReply {
        return Err(AppError::NotSupported);
    }
    let (account, parent_ext, parent_id, content_ext) = {
        let conn = lock(db)?;
        let (account_id, ext, id, content_ext): (String, String, String, Option<String>) = conn
            .query_row(
                "SELECT c.account_id, COALESCE(p.external_id, c.external_id),
                        COALESCE(p.id, c.id), k.external_id
                 FROM comments c LEFT JOIN comments p ON p.id = c.parent_id
                 LEFT JOIN contents k ON k.id = c.content_id
                 WHERE c.id = ?1",
                params![comment_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or(AppError::NotFound)?;
        (accounts::get(&conn, &account_id)?, ext, id, content_ext)
    };
    let needed = connector.comment_scopes();
    if !needed.iter().all(|s| account.granted_scopes.contains(s)) {
        return Err(AppError::ScopesDenied);
    }
    let quota = Arc::new(DbQuota::new(
        db.clone(),
        clock.clone(),
        &account.platform,
        Purpose::Upload,
    ));
    let ctx = account_ctx(auth, connector, &account, quota);
    let mut sent = connector.reply_comment(&ctx, &parent_ext, text).await?;
    sent.parent_external_id = Some(parent_ext);
    sent.content_external_id = sent.content_external_id.or(content_ext);
    sent.is_own = true;
    let now = clock.now_iso();
    sent.published_at = sent.published_at.or_else(|| Some(now.clone()));
    let mut conn = lock(db)?;
    let tx = conn.transaction()?;
    upsert_comments(&tx, &account.id, std::slice::from_ref(&sent), &now)?;
    tx.execute(
        "UPDATE comments SET status = 'replied' WHERE id = ?1",
        params![parent_id],
    )?;
    audit_log::append(
        &tx,
        &now,
        "reply",
        Some(&account.id),
        &json!({ "platform": account.platform, "comment": sent.external_id }),
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::auth::MemoryStore;
    use crate::clock::FixedClock;
    use crate::connectors::mock::MockConnector;
    use crate::db::{open_in_memory, repo::accounts::NewAccount};
    use time::macros::datetime;

    pub(crate) fn env() -> (Arc<Mutex<Connection>>, Arc<AuthService>, Arc<dyn Clock>) {
        let conn = open_in_memory().unwrap();
        accounts::upsert(
            &conn,
            "acc",
            &NewAccount {
                platform: "mock",
                external_id: "me",
                handle: None,
                display_name: "Me",
                granted_scopes: &[],
            },
            "t0",
        )
        .unwrap();
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(datetime!(2026-11-22 12:00 UTC)));
        let auth = Arc::new(AuthService::new(
            Arc::new(MemoryStore::new()),
            reqwest::Client::new(),
            clock.clone(),
        ));
        (Arc::new(Mutex::new(conn)), auth, clock)
    }

    #[tokio::test]
    async fn sync_stores_threads_and_marks_answered_ones() {
        let (db, auth, clock) = env();
        let m = MockConnector::new();
        let r = sync(&db, &auth, &clock, &m, "acc").await.unwrap();
        assert_eq!((r.fetched, r.new), (9, 9));
        // Second sync: nothing new.
        let r = sync(&db, &auth, &clock, &m, "acc").await.unwrap();
        assert_eq!(r.new, 0);
        let conn = db.lock().unwrap();
        let all = list(&conn, &CommentFilter::default()).unwrap();
        assert_eq!(all.len(), 8, "own replies are not inbox entries");
        let thanked = all
            .iter()
            .find(|c| c.text.starts_with("¡Me encanta"))
            .unwrap();
        assert_eq!(thanked.status, "replied");
        assert_eq!(thanked.replies.len(), 1);
        assert!(thanked.replies[0].is_own);
        assert_eq!(counts(&conn).unwrap().pending, 7);
    }

    #[tokio::test]
    async fn filters_by_text_status_and_category() {
        let (db, auth, clock) = env();
        sync(&db, &auth, &clock, &MockConnector::new(), "acc")
            .await
            .unwrap();
        let conn = db.lock().unwrap();
        let f = |search: Option<&str>, status: Option<&str>| {
            list(
                &conn,
                &CommentFilter {
                    search: search.map(Into::into),
                    status: status.map(Into::into),
                    ..Default::default()
                },
            )
            .unwrap()
            .len()
        };
        assert_eq!(f(Some("MOD"), None), 2);
        assert_eq!(f(Some("100%"), None), 0, "LIKE wildcards are escaped");
        assert_eq!(f(None, Some("replied")), 1);
        let id = list(&conn, &CommentFilter::default()).unwrap()[0]
            .id
            .clone();
        set_status(&conn, &id, "archived").unwrap();
        assert_eq!(f(None, Some("archived")), 1);
        assert!(set_status(&conn, &id, "nope").is_err());
    }

    #[tokio::test]
    async fn reply_is_stored_audited_and_answers_the_thread() {
        let (db, auth, clock) = env();
        let m = MockConnector::new();
        sync(&db, &auth, &clock, &m, "acc").await.unwrap();
        let target = {
            let conn = db.lock().unwrap();
            list(&conn, &CommentFilter::default())
                .unwrap()
                .into_iter()
                .find(|c| c.status == "pending")
                .unwrap()
        };
        assert!(matches!(
            reply(&db, &auth, &clock, &m, &target.id, "  ").await,
            Err(AppError::Validation { .. })
        ));
        reply(&db, &auth, &clock, &m, &target.id, "¡Gracias por comentar!")
            .await
            .unwrap();
        let conn = db.lock().unwrap();
        let after = list(&conn, &CommentFilter::default())
            .unwrap()
            .into_iter()
            .find(|c| c.id == target.id)
            .unwrap();
        assert_eq!(after.status, "replied");
        assert_eq!(after.replies.last().unwrap().text, "¡Gracias por comentar!");
        assert!(audit_log::recent_actions(&conn, 5)
            .unwrap()
            .iter()
            .any(|(a, _)| a == "reply"));
    }
}
