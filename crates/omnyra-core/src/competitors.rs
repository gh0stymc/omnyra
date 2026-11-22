//! Competitors and trends (step 6b), YouTube first (A8): public channels the user follows
//! (daily snapshots of subscribers/views/videos and their latest videos) and keyword
//! trends (most viewed videos of the last 7 days for a search).
//!
//! Every call goes through one of the user's YouTube accounts and its quota: a channel
//! refresh costs ~3 units, a keyword search ~101, so trends are refreshed at most once a
//! day per keyword.

use crate::connectors::{AccountCtx, Connector, PublicChannel, PublicVideo};
use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

/// `(id, platform, external_id, handle, display_name, refreshed/synced at)`.
type ChannelRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub const MAX_TRACKED: usize = 50;
pub const MAX_KEYWORDS: usize = 20;
const RECENT_VIDEOS: usize = 30;

fn day(now: &str) -> &str {
    now.get(..10).unwrap_or(now)
}

fn save_channel(conn: &Connection, tracked_id: &str, c: &PublicChannel, now: &str) -> Result<()> {
    conn.execute(
        "UPDATE tracked_accounts SET handle = ?2, display_name = ?3, refreshed_at = ?4 WHERE id = ?1",
        params![tracked_id, c.handle, c.display_name, now],
    )?;
    for (m, v) in [
        ("followers", c.followers),
        ("views_total", c.views_total),
        ("contents_total", c.contents_total),
    ] {
        if let Some(v) = v {
            conn.execute(
                "INSERT INTO tracked_snapshots (tracked_id, day, metric, value) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (tracked_id, metric, day) DO UPDATE SET value = excluded.value",
                params![tracked_id, day(now), m, v],
            )?;
        }
    }
    Ok(())
}

fn save_videos(
    conn: &Connection,
    tracked_id: &str,
    videos: &[PublicVideo],
    now: &str,
) -> Result<()> {
    for v in videos {
        conn.execute(
            "INSERT INTO tracked_contents (id, tracked_id, external_id, title, published_at, views,
                                           likes, comments, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (tracked_id, external_id) DO UPDATE SET title = excluded.title,
                views = excluded.views, likes = excluded.likes, comments = excluded.comments,
                fetched_at = excluded.fetched_at",
            params![
                uuid::Uuid::now_v7().to_string(),
                tracked_id,
                v.external_id,
                v.title,
                v.published_at,
                v.views,
                v.likes,
                v.comments,
                now
            ],
        )?;
    }
    Ok(())
}

/// Adds a channel from a handle, ID or URL and takes its first snapshot.
pub async fn add(
    conn: &std::sync::Mutex<Connection>,
    connector: &dyn Connector,
    ctx: &AccountCtx,
    query: &str,
    now: &str,
) -> Result<String> {
    let count: i64 =
        lock(conn)?.query_row("SELECT COUNT(*) FROM tracked_accounts", [], |r| r.get(0))?;
    if count as usize >= MAX_TRACKED {
        return Err(AppError::Validation {
            field: "limit".into(),
        });
    }
    let platform = connector.info().id.clone();
    let ch = connector.find_public_channel(ctx, query).await?;
    if ch.external_id == ctx.external_id {
        return Err(AppError::Validation {
            field: "own".into(),
        });
    }
    let videos = connector
        .public_recent_videos(ctx, &ch.external_id, RECENT_VIDEOS)
        .await
        .unwrap_or_default();
    let c = lock(conn)?;
    let existing: Option<String> = c
        .query_row(
            "SELECT id FROM tracked_accounts WHERE platform = ?1 AND external_id = ?2",
            params![platform, ch.external_id],
            |r| r.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::now_v7().to_string();
            c.execute(
                "INSERT INTO tracked_accounts (id, platform, external_id, handle, display_name, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, platform, ch.external_id, ch.handle, ch.display_name, now],
            )?;
            id
        }
    };
    save_channel(&c, &id, &ch, now)?;
    save_videos(&c, &id, &videos, now)?;
    Ok(id)
}

fn lock(db: &std::sync::Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Internal)
}

pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM tracked_accounts WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Refreshes every tracked channel of `platform` not refreshed today.
pub async fn refresh_all(
    db: &std::sync::Mutex<Connection>,
    connector: &dyn Connector,
    ctx: &AccountCtx,
    now: &str,
    force: bool,
) -> Result<u32> {
    let platform = connector.info().id.clone();
    let due: Vec<(String, String)> = {
        let c = lock(db)?;
        let mut st = c.prepare(
            "SELECT id, external_id FROM tracked_accounts
             WHERE platform = ?1 AND (?3 OR refreshed_at IS NULL OR substr(refreshed_at, 1, 10) < ?2)",
        )?;
        let r = st.query_map(params![platform, day(now), force], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        r.collect::<std::result::Result<_, _>>()?
    };
    if due.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = due.iter().map(|d| d.1.clone()).collect();
    let channels = connector.public_channels(ctx, &ids).await?;
    let mut n = 0;
    for (tid, ext) in &due {
        let Some(ch) = channels.iter().find(|c| &c.external_id == ext) else {
            continue;
        };
        let videos = connector
            .public_recent_videos(ctx, ext, RECENT_VIDEOS)
            .await
            .unwrap_or_default();
        let c = lock(db)?;
        save_channel(&c, tid, ch, now)?;
        save_videos(&c, tid, &videos, now)?;
        n += 1;
    }
    Ok(n)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackedSummary {
    pub id: String,
    pub platform: String,
    pub external_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub followers: Option<f64>,
    /// Change over the last 30 days of snapshots (needs a snapshot that old).
    pub followers_change_30d: Option<f64>,
    pub views_total: Option<f64>,
    pub contents_total: Option<f64>,
    /// Videos per week over the last 90 days (from the latest videos seen).
    pub uploads_per_week: f64,
    /// Median views of the latest videos.
    pub median_views: Option<f64>,
    pub refreshed_at: Option<String>,
    /// True for the user's own account (comparison baseline).
    pub own: bool,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// Uploads per week in the 90 days before `now`.
pub fn rhythm(published: &[String], now: OffsetDateTime) -> f64 {
    let from = now - Duration::days(90);
    let n = published
        .iter()
        .filter_map(|p| OffsetDateTime::parse(p, &Rfc3339).ok())
        .filter(|t| *t >= from && *t <= now)
        .count();
    n as f64 / (90.0 / 7.0)
}

fn latest_metric(
    conn: &Connection,
    sql_table: &str,
    key_col: &str,
    id: &str,
    metric: &str,
    before: Option<&str>,
) -> Result<Option<f64>> {
    let sql = format!(
        "SELECT value FROM {sql_table} WHERE {key_col} = ?1 AND metric = ?2 AND (?3 IS NULL OR day <= ?3)
         ORDER BY day DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, params![id, metric, before], |r| r.get(0))
        .optional()?)
}

/// Every tracked channel plus the user's own YouTube channels, for comparison.
pub fn list(conn: &Connection, now: &str) -> Result<Vec<TrackedSummary>> {
    let t = OffsetDateTime::parse(now, &Rfc3339).map_err(|_| AppError::Internal)?;
    let ago = crate::format_day(t - Duration::days(30));
    let mut out = Vec::new();
    let tracked: Vec<ChannelRow> = {
        let mut st = conn.prepare(
            "SELECT id, platform, external_id, handle, display_name, refreshed_at
             FROM tracked_accounts ORDER BY display_name",
        )?;
        let r = st.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        r.collect::<std::result::Result<_, _>>()?
    };
    for (id, platform, ext, handle, name, refreshed) in tracked {
        let f = latest_metric(
            conn,
            "tracked_snapshots",
            "tracked_id",
            &id,
            "followers",
            None,
        )?;
        let old = latest_metric(
            conn,
            "tracked_snapshots",
            "tracked_id",
            &id,
            "followers",
            Some(&ago),
        )?;
        let (dates, views): (Vec<String>, Vec<f64>) = {
            let mut st = conn.prepare(
                "SELECT published_at, views FROM tracked_contents WHERE tracked_id = ?1
                 ORDER BY published_at DESC LIMIT 30",
            )?;
            let r = st.query_map(params![id], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<f64>>(1)?))
            })?;
            let rows: Vec<_> = r.collect::<std::result::Result<_, _>>()?;
            (
                rows.iter().filter_map(|x| x.0.clone()).collect(),
                rows.iter().filter_map(|x| x.1).collect(),
            )
        };
        out.push(TrackedSummary {
            followers_change_30d: f.zip(old).map(|(a, b)| a - b),
            followers: f,
            views_total: latest_metric(
                conn,
                "tracked_snapshots",
                "tracked_id",
                &id,
                "views_total",
                None,
            )?,
            contents_total: latest_metric(
                conn,
                "tracked_snapshots",
                "tracked_id",
                &id,
                "contents_total",
                None,
            )?,
            uploads_per_week: rhythm(&dates, t),
            median_views: median(views),
            refreshed_at: refreshed,
            own: false,
            id,
            platform,
            external_id: ext,
            handle,
            display_name: name,
        });
    }
    // Own channels as the baseline.
    let own: Vec<ChannelRow> = {
        let mut st = conn.prepare(
            "SELECT id, platform, external_id, handle, display_name, last_sync_at FROM accounts
             WHERE platform = 'youtube' AND status <> 'disconnected'",
        )?;
        let r = st.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        r.collect::<std::result::Result<_, _>>()?
    };
    for (id, platform, ext, handle, name, synced) in own {
        let f = latest_metric(
            conn,
            "account_snapshots",
            "account_id",
            &id,
            "followers",
            None,
        )?;
        let old = latest_metric(
            conn,
            "account_snapshots",
            "account_id",
            &id,
            "followers",
            Some(&ago),
        )?;
        let rows: Vec<(Option<String>, Option<f64>)> =
            crate::db::repo::contents::list_with_stats(conn, &id)?
                .into_iter()
                .take(30)
                .map(|c| (c.published_at, c.views))
                .collect();
        out.push(TrackedSummary {
            followers_change_30d: f.zip(old).map(|(a, b)| a - b),
            followers: f,
            views_total: latest_metric(
                conn,
                "account_snapshots",
                "account_id",
                &id,
                "views_total",
                None,
            )?,
            contents_total: latest_metric(
                conn,
                "account_snapshots",
                "account_id",
                &id,
                "contents_total",
                None,
            )?,
            uploads_per_week: rhythm(
                &rows.iter().filter_map(|r| r.0.clone()).collect::<Vec<_>>(),
                t,
            ),
            median_views: median(rows.iter().filter_map(|r| r.1).collect()),
            refreshed_at: synced,
            own: true,
            id,
            platform,
            external_id: ext,
            handle,
            display_name: name,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayValue {
    pub day: String,
    pub value: f64,
}

pub fn followers_series(conn: &Connection, tracked_id: &str) -> Result<Vec<DayValue>> {
    let mut st = conn.prepare(
        "SELECT day, value FROM tracked_snapshots WHERE tracked_id = ?1 AND metric = 'followers' ORDER BY day",
    )?;
    let r = st.query_map(params![tracked_id], |r| {
        Ok(DayValue {
            day: r.get(0)?,
            value: r.get(1)?,
        })
    })?;
    Ok(r.collect::<std::result::Result<_, _>>()?)
}

pub fn top_videos(conn: &Connection, tracked_id: &str, limit: u32) -> Result<Vec<PublicVideo>> {
    let mut st = conn.prepare(
        "SELECT t.external_id, a.external_id, a.display_name, t.title, t.published_at, t.views, t.likes, t.comments
         FROM tracked_contents t JOIN tracked_accounts a ON a.id = t.tracked_id
         WHERE t.tracked_id = ?1 ORDER BY t.views DESC LIMIT ?2",
    )?;
    let r = st.query_map(params![tracked_id, limit], |r| {
        let id: String = r.get(0)?;
        Ok(PublicVideo {
            url: Some(format!("https://www.youtube.com/watch?v={id}")),
            external_id: id,
            channel_id: r.get(1)?,
            channel_title: r.get(2)?,
            title: r.get(3)?,
            published_at: r.get(4)?,
            views: r.get(5)?,
            likes: r.get(6)?,
            comments: r.get(7)?,
        })
    })?;
    Ok(r.collect::<std::result::Result<_, _>>()?)
}

// ---- trends ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendKeyword {
    pub id: String,
    pub keyword: String,
    pub last_day: Option<String>,
    /// Latest results, most viewed first.
    pub videos: Vec<PublicVideo>,
    /// Total views of the top results per day (trend line).
    pub history: Vec<DayValue>,
}

pub fn add_keyword(conn: &Connection, keyword: &str, now: &str) -> Result<String> {
    let k = keyword.trim();
    if k.is_empty() || k.chars().count() > 100 {
        return Err(AppError::Validation {
            field: "keyword".into(),
        });
    }
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM trend_keywords", [], |r| r.get(0))?;
    if count as usize >= MAX_KEYWORDS {
        return Err(AppError::Validation {
            field: "limit".into(),
        });
    }
    let id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO trend_keywords (id, keyword, platform, created_at) VALUES (?1, ?2, 'youtube', ?3)",
        params![id, k, now],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM trend_keywords WHERE platform = 'youtube' AND keyword = ?1",
        params![k],
        |r| r.get(0),
    )?)
}

pub fn remove_keyword(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM trend_keywords WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Searches each keyword not searched today (100+ quota units each).
pub async fn refresh_trends(
    db: &std::sync::Mutex<Connection>,
    connector: &dyn Connector,
    ctx: &AccountCtx,
    now: &str,
) -> Result<u32> {
    let today = day(now).to_owned();
    let due: Vec<(String, String)> = {
        let c = lock(db)?;
        let mut st = c.prepare(
            "SELECT k.id, k.keyword FROM trend_keywords k
             WHERE NOT EXISTS (SELECT 1 FROM trend_results r WHERE r.keyword_id = k.id AND r.day = ?1)",
        )?;
        let r = st.query_map(params![today], |r| Ok((r.get(0)?, r.get(1)?)))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let t = OffsetDateTime::parse(now, &Rfc3339).map_err(|_| AppError::Internal)?;
    let after = (t - Duration::days(7))
        .format(&Rfc3339)
        .map_err(|_| AppError::Internal)?;
    let mut n = 0;
    for (id, kw) in due {
        let videos = connector.search_videos(ctx, &kw, &after).await?;
        let c = lock(db)?;
        c.execute(
            "INSERT INTO trend_results (keyword_id, day, data) VALUES (?1, ?2, ?3)
             ON CONFLICT (keyword_id, day) DO UPDATE SET data = excluded.data",
            params![
                id,
                today,
                serde_json::to_string(&videos).map_err(|_| AppError::Internal)?
            ],
        )?;
        n += 1;
    }
    Ok(n)
}

pub fn list_trends(conn: &Connection) -> Result<Vec<TrendKeyword>> {
    let kws: Vec<(String, String)> = {
        let mut st = conn.prepare("SELECT id, keyword FROM trend_keywords ORDER BY keyword")?;
        let r = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let mut out = Vec::new();
    for (id, keyword) in kws {
        let rows: Vec<(String, String)> = {
            let mut st = conn.prepare(
                "SELECT day, data FROM trend_results WHERE keyword_id = ?1 ORDER BY day",
            )?;
            let r = st.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect::<std::result::Result<_, _>>()?
        };
        let mut history = Vec::new();
        let mut last: Vec<PublicVideo> = Vec::new();
        for (d, data) in &rows {
            let mut v: Vec<PublicVideo> = serde_json::from_str(data).unwrap_or_default();
            v.sort_by(|a, b| {
                b.views
                    .partial_cmp(&a.views)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            history.push(DayValue {
                day: d.clone(),
                value: v.iter().filter_map(|x| x.views).sum(),
            });
            last = v;
        }
        out.push(TrendKeyword {
            id,
            keyword,
            last_day: rows.last().map(|r| r.0.clone()),
            videos: last,
            history,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::http::{ApiClient, NoToken};
    use crate::connectors::mock::MockConnector;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// Mock with fake public data.
    struct Public(MockConnector);

    #[async_trait]
    impl Connector for Public {
        fn info(&self) -> &crate::connectors::PlatformInfo {
            self.0.info()
        }
        async fn fetch_profile(&self, c: &AccountCtx) -> Result<crate::connectors::Profile> {
            self.0.fetch_profile(c).await
        }
        async fn fetch_channel_snapshot(
            &self,
            c: &AccountCtx,
        ) -> Result<crate::connectors::ChannelSnapshot> {
            self.0.fetch_channel_snapshot(c).await
        }
        async fn fetch_channel_metrics(
            &self,
            c: &AccountCtx,
            r: crate::connectors::DateRange,
        ) -> Result<Vec<crate::connectors::MetricPoint>> {
            self.0.fetch_channel_metrics(c, r).await
        }
        async fn list_contents(
            &self,
            c: &AccountCtx,
            p: Option<String>,
        ) -> Result<crate::connectors::Page<crate::connectors::ContentRef>> {
            self.0.list_contents(c, p).await
        }
        async fn fetch_content_details(
            &self,
            c: &AccountCtx,
            ids: &[String],
        ) -> Result<Vec<crate::connectors::ContentDetails>> {
            self.0.fetch_content_details(c, ids).await
        }
        async fn find_public_channel(&self, _c: &AccountCtx, q: &str) -> Result<PublicChannel> {
            Ok(chan(q.trim_start_matches('@'), 1000.0))
        }
        async fn public_channels(
            &self,
            _c: &AccountCtx,
            ids: &[String],
        ) -> Result<Vec<PublicChannel>> {
            Ok(ids.iter().map(|i| chan(i, 1500.0)).collect())
        }
        async fn public_recent_videos(
            &self,
            _c: &AccountCtx,
            ch: &str,
            _m: usize,
        ) -> Result<Vec<PublicVideo>> {
            Ok((0..4)
                .map(|i| PublicVideo {
                    external_id: format!("{ch}-v{i}"),
                    channel_id: ch.into(),
                    channel_title: None,
                    title: Some(format!("v{i}")),
                    published_at: Some(format!("2026-11-{:02}T10:00:00Z", 1 + i * 5)),
                    views: Some(100.0 * (i + 1) as f64),
                    likes: None,
                    comments: None,
                    url: None,
                })
                .collect())
        }
        async fn search_videos(
            &self,
            c: &AccountCtx,
            q: &str,
            _a: &str,
        ) -> Result<Vec<PublicVideo>> {
            self.public_recent_videos(c, q, 3).await
        }
    }

    fn chan(id: &str, followers: f64) -> PublicChannel {
        PublicChannel {
            external_id: id.into(),
            handle: Some(format!("@{id}")),
            display_name: id.to_uppercase(),
            avatar_url: None,
            followers: Some(followers),
            views_total: Some(5.0),
            contents_total: Some(4.0),
            created_at: None,
        }
    }

    fn ctx() -> AccountCtx {
        AccountCtx::unmetered(
            "own",
            "UCown",
            ApiClient::new(reqwest::Client::new(), Arc::new(NoToken)),
        )
    }

    #[tokio::test]
    async fn track_refresh_compare_and_remove() {
        let db = Mutex::new(crate::db::open_in_memory().unwrap());
        let p = Public(MockConnector::new());
        let id = add(&db, &p, &ctx(), "@rival", "2026-11-01T10:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            add(&db, &p, &ctx(), "@rival", "2026-11-01T11:00:00Z")
                .await
                .unwrap(),
            id,
            "no duplicates"
        );
        assert!(matches!(
            add(&db, &p, &ctx(), "UCown", "t").await,
            Err(AppError::Validation { .. })
        ));
        assert_eq!(
            refresh_all(&db, &p, &ctx(), "2026-11-01T12:00:00Z", false)
                .await
                .unwrap(),
            0,
            "already today"
        );
        assert_eq!(
            refresh_all(&db, &p, &ctx(), "2026-12-02T12:00:00Z", false)
                .await
                .unwrap(),
            1
        );
        let conn = db.lock().unwrap();
        let l = list(&conn, "2026-12-02T12:00:00Z").unwrap();
        let r = l.iter().find(|x| x.id == id).unwrap();
        assert_eq!(r.followers, Some(1500.0));
        assert_eq!(r.followers_change_30d, Some(500.0));
        assert_eq!(r.median_views, Some(250.0));
        assert!(r.uploads_per_week > 0.0);
        assert_eq!(followers_series(&conn, &id).unwrap().len(), 2);
        assert_eq!(top_videos(&conn, &id, 2).unwrap()[0].views, Some(400.0));
        remove(&conn, &id).unwrap();
        assert!(list(&conn, "2026-12-02T12:00:00Z").unwrap().is_empty());
    }

    #[tokio::test]
    async fn trends_once_a_day() {
        let db = Mutex::new(crate::db::open_in_memory().unwrap());
        let p = Public(MockConnector::new());
        let k = add_keyword(&db.lock().unwrap(), " minecraft hardcore ", "t").unwrap();
        assert_eq!(
            add_keyword(&db.lock().unwrap(), "minecraft hardcore", "t").unwrap(),
            k
        );
        assert!(add_keyword(&db.lock().unwrap(), "  ", "t").is_err());
        assert_eq!(
            refresh_trends(&db, &p, &ctx(), "2026-11-22T10:00:00Z")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            refresh_trends(&db, &p, &ctx(), "2026-11-22T18:00:00Z")
                .await
                .unwrap(),
            0
        );
        refresh_trends(&db, &p, &ctx(), "2026-11-23T10:00:00Z")
            .await
            .unwrap();
        let t = list_trends(&db.lock().unwrap()).unwrap();
        assert_eq!(t[0].history.len(), 2);
        assert_eq!(t[0].videos[0].views, Some(400.0));
        remove_keyword(&db.lock().unwrap(), &k).unwrap();
    }

    #[test]
    fn rhythm_counts_last_90_days() {
        let now = OffsetDateTime::parse("2026-11-22T00:00:00Z", &Rfc3339).unwrap();
        let dates = vec![
            "2026-11-20T00:00:00Z".to_string(),
            "2026-01-01T00:00:00Z".into(),
            "bad".into(),
        ];
        assert!((rhythm(&dates, now) - 7.0 / 90.0).abs() < 1e-9);
    }
}
