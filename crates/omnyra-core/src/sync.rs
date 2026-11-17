//! Account synchronisation (step 1b): channel snapshot → contents → per-content stats →
//! kind classification → daily history (gap filling) → follower reconstruction.
//!
//! Each phase writes in its own transaction, so a failure (quota, network) keeps what was
//! already fetched. Running out of Data API quota degrades the sync instead of failing it:
//! the Analytics history does not use that quota and is still fetched.

use crate::accounts::account_ctx;
use crate::auth::AuthService;
use crate::clock::Clock;
use crate::connectors::{AccountCtx, Connector, DateRange};
use crate::db::repo::{
    accounts::{self, AccountStatus},
    contents, snapshots,
};
use crate::quota::{self, DbQuota, QuotaGate, Unmetered};
use crate::{AppError, Result};
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use time::{macros::format_description, Date, Duration};

/// Analytics data can still change for this many days, so they are fetched again.
const HISTORY_OVERLAP_DAYS: i64 = 3;
/// Backfill start when the platform does not say when the account was created.
const DEFAULT_BACKFILL_DAYS: i64 = 365;
/// Content IDs per details request (the connector may batch further).
const DETAILS_BATCH: usize = 50;

#[derive(Debug, Clone, Default, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub new_contents: u32,
    pub refreshed_contents: u32,
    /// Contents whose kind (Short or video) could not be settled yet.
    pub unclassified: u32,
    pub historic_days: u32,
    /// Data API units spent by this sync.
    pub quota_used: u32,
    /// The sync budget ran out: some steps were skipped (they resume on the next sync).
    pub quota_limited: bool,
}

/// Prevents two syncs of the same account at once (startup auto-sync vs. the button).
#[derive(Default)]
pub struct SyncLocks(Mutex<HashSet<String>>);

pub struct SyncGuard<'a> {
    locks: &'a SyncLocks,
    account_id: String,
}

impl SyncLocks {
    pub fn try_begin(&self, account_id: &str) -> Result<SyncGuard<'_>> {
        let mut set = self.0.lock().map_err(|_| AppError::Internal)?;
        if !set.insert(account_id.to_owned()) {
            return Err(AppError::SyncInProgress);
        }
        Ok(SyncGuard {
            locks: self,
            account_id: account_id.to_owned(),
        })
    }
}

impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut set) = self.locks.0.lock() {
            set.remove(&self.account_id);
        }
    }
}

/// Counts what a sync spends on top of the real gate.
struct Counting {
    inner: Arc<dyn QuotaGate>,
    used: AtomicU32,
}

impl QuotaGate for Counting {
    fn spend(&self, units: u32) -> Result<()> {
        self.inner.spend(units)?;
        self.used.fetch_add(units, Ordering::SeqCst);
        Ok(())
    }
}

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Internal)
}

/// Runs `f` in a transaction.
fn write<T>(
    db: &Mutex<Connection>,
    f: impl FnOnce(&rusqlite::Transaction) -> Result<T>,
) -> Result<T> {
    let mut conn = lock(db)?;
    let tx = conn.transaction()?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}

const DAY_FMT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

fn parse_day(s: &str) -> Result<Date> {
    // Accepts both `YYYY-MM-DD` and full ISO-8601 timestamps.
    Date::parse(s.get(..10).unwrap_or(s), DAY_FMT).map_err(|_| AppError::Internal)
}

fn fmt_day(d: Date) -> String {
    d.format(DAY_FMT).expect("format date")
}

pub async fn sync_account(
    db: &Arc<Mutex<Connection>>,
    auth: &Arc<AuthService>,
    clock: &Arc<dyn Clock>,
    connector: &dyn Connector,
    account_id: &str,
) -> Result<SyncReport> {
    let account = accounts::get(&*lock(db)?, account_id)?;
    if account.status != AccountStatus::Active {
        return Err(AppError::NotConnected);
    }
    let gate: Arc<dyn QuotaGate> = match quota::sync_budget(&account.platform) {
        Some(budget) => Arc::new(DbQuota {
            db: db.clone(),
            clock: clock.clone(),
            platform: account.platform.clone(),
            budget,
        }),
        None => Arc::new(Unmetered),
    };
    let counting = Arc::new(Counting {
        inner: gate,
        used: AtomicU32::new(0),
    });
    let ctx = account_ctx(auth, connector, &account, counting.clone());
    let today = clock.today();
    let now = clock.now_iso();
    let mut report = SyncReport::default();

    // 1. Channel snapshot: profile and cumulative totals.
    let mut created_at = None;
    match connector.fetch_channel_snapshot(&ctx).await {
        Ok(snap) => {
            created_at = snap.created_at.clone();
            write(db, |tx| {
                accounts::update_profile(
                    tx,
                    account_id,
                    snap.profile.handle.as_deref(),
                    &snap.profile.display_name,
                )?;
                for t in &snap.totals {
                    snapshots::upsert_account_snapshot(
                        tx, account_id, &today, &t.metric, t.value, "current", &now,
                    )?;
                }
                Ok(())
            })?;
        }
        Err(AppError::QuotaExceeded) => report.quota_limited = true,
        Err(e) => return Err(e),
    }

    // 2–4. Contents, their stats and their kind.
    if !report.quota_limited {
        match sync_contents(db, connector, &ctx, &today, &now, &mut report).await {
            Ok(()) => {}
            Err(AppError::QuotaExceeded) => report.quota_limited = true,
            Err(e) => return Err(e),
        }
    }

    // 5. Daily history and gap filling.
    if connector.info().capabilities.historic_analytics {
        let from = {
            let conn = lock(db)?;
            snapshots::last_day(&conn, account_id, "views", "historic")?
        };
        let from = match from {
            Some(d) => parse_day(&d)? - Duration::days(HISTORY_OVERLAP_DAYS),
            None => match created_at.as_deref().map(parse_day) {
                Some(Ok(d)) => d,
                _ => parse_day(&today)? - Duration::days(DEFAULT_BACKFILL_DAYS),
            },
        };
        let to = parse_day(&today)?;
        if from <= to {
            let points = connector
                .fetch_channel_metrics(
                    &ctx,
                    DateRange {
                        from: fmt_day(from),
                        to: fmt_day(to),
                    },
                )
                .await?;
            report.historic_days = points
                .iter()
                .map(|p| p.day.as_str())
                .collect::<HashSet<_>>()
                .len() as u32;
            write(db, |tx| {
                for p in &points {
                    snapshots::upsert_account_snapshot(
                        tx, account_id, &p.day, &p.metric, p.value, "historic", &now,
                    )?;
                }
                Ok(())
            })?;
        }
        // 6. Followers on past days, from today's total and the daily net gains.
        write(db, |tx| reconstruct_followers(tx, account_id, &now))?;
    }

    write(db, |tx| accounts::set_last_sync(tx, account_id, &now))?;
    report.quota_used = counting.used.load(Ordering::SeqCst);
    tracing::info!(?report, platform = %account.platform, "account synced");
    Ok(report)
}

async fn sync_contents(
    db: &Arc<Mutex<Connection>>,
    connector: &dyn Connector,
    ctx: &AccountCtx,
    today: &str,
    now: &str,
    report: &mut SyncReport,
) -> Result<()> {
    let account_id = ctx.account_id.as_str();
    let known = contents::known(&*lock(db)?, account_id)?;

    // Newest first: stop paging at the first page that reaches a known content.
    let mut new_ids = Vec::new();
    let mut page = None;
    loop {
        let p = connector.list_contents(ctx, page).await?;
        let mut reached_known = false;
        for item in p.items {
            if known.contains_key(&item.external_id) {
                reached_known = true;
            } else if !new_ids.contains(&item.external_id) {
                new_ids.push(item.external_id);
            }
        }
        match p.next {
            Some(n) if !reached_known => page = Some(n),
            _ => break,
        }
    }
    report.new_contents = new_ids.len() as u32;

    // New contents first, so they get stored even if the quota runs out halfway.
    let mut ids = new_ids;
    let mut old: Vec<String> = known.keys().cloned().collect();
    old.sort();
    ids.extend(old);

    let mut to_classify = Vec::new();
    for batch in ids.chunks(DETAILS_BATCH) {
        let details = connector.fetch_content_details(ctx, batch).await?;
        write(db, |tx| {
            for d in &details {
                let prev = known.get(&d.content.external_id);
                let (kind, checked) = match prev {
                    _ if d.kind_final => (d.content.kind.as_str(), true),
                    Some(k) if k.kind_checked => (k.kind.as_str(), true),
                    _ => (d.content.kind.as_str(), false),
                };
                if !checked {
                    to_classify.push(d.content.external_id.clone());
                }
                let id = contents::upsert(tx, account_id, &d.content, kind, checked, now)?;
                for s in &d.stats {
                    snapshots::upsert_content_snapshot(
                        tx, &id, today, &s.metric, s.value, "current", now,
                    )?;
                }
                if prev.is_some() {
                    report.refreshed_contents += 1;
                }
            }
            Ok(())
        })?;
    }

    if !to_classify.is_empty() {
        let settled: HashMap<String, String> = connector
            .classify(ctx, &to_classify)
            .await
            .into_iter()
            .collect();
        write(db, |tx| {
            for (ext, kind) in &settled {
                contents::set_kind(tx, account_id, ext, kind)?;
            }
            Ok(())
        })?;
        report.unclassified = (to_classify.len() - settled.len()) as u32;
    }
    Ok(())
}

/// Walks back from every day with a real (`current`) follower count, subtracting each
/// day's net gain, and writes `historic` followers on the days in between. Days with a
/// real count are never overwritten and re-anchor the walk.
fn reconstruct_followers(conn: &Connection, account_id: &str, now: &str) -> Result<()> {
    let followers = snapshots::account_points(conn, account_id, "followers")?;
    let current: BTreeMap<Date, f64> = followers
        .iter()
        .filter(|(_, _, src)| src == "current")
        .filter_map(|(d, v, _)| Some((parse_day(d).ok()?, *v)))
        .collect();
    let Some((&anchor_day, &anchor_value)) = current.iter().next_back() else {
        return Ok(()); // hidden subscriber count, or no snapshot yet
    };
    let mut net: HashMap<Date, f64> = HashMap::new();
    for (metric, sign) in [("subscribers_gained", 1.0), ("subscribers_lost", -1.0)] {
        for (d, v, _) in snapshots::account_points(conn, account_id, metric)? {
            *net.entry(parse_day(&d)?).or_default() += sign * v;
        }
    }
    let Some(&first) = net.keys().min() else {
        return Ok(());
    };

    let mut day = anchor_day;
    let mut value = anchor_value;
    while day > first {
        // followers(d - 1) = followers(d) - net(d)
        value -= net.get(&day).copied().unwrap_or(0.0);
        day -= Duration::days(1);
        if let Some(&real) = current.get(&day) {
            value = real;
            continue;
        }
        snapshots::upsert_account_snapshot(
            conn,
            account_id,
            &fmt_day(day),
            "followers",
            value.max(0.0),
            "historic",
            now,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MemoryStore;
    use crate::clock::FixedClock;
    use crate::connectors::mock::MockConnector;
    use crate::db::{open_in_memory, repo::accounts::NewAccount};
    use time::macros::datetime;

    struct Env {
        db: Arc<Mutex<Connection>>,
        auth: Arc<AuthService>,
        clock: Arc<dyn Clock>,
        account_id: String,
    }

    fn env(platform: &str) -> Env {
        let conn = open_in_memory().unwrap();
        let acc = accounts::upsert(
            &conn,
            "acc-1",
            &NewAccount {
                platform,
                external_id: "UCgh0st",
                handle: None,
                display_name: "old name",
                granted_scopes: &[],
            },
            "2026-11-01T00:00:00Z",
        )
        .unwrap();
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(datetime!(2026-11-17 12:00:00 UTC)));
        Env {
            db: Arc::new(Mutex::new(conn)),
            auth: Arc::new(AuthService::new(
                Arc::new(MemoryStore::new()),
                reqwest::Client::new(),
                clock.clone(),
            )),
            clock,
            account_id: acc.id,
        }
    }

    fn count(db: &Mutex<Connection>, sql: &str) -> i64 {
        db.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[tokio::test]
    async fn mock_sync_fills_everything_and_is_idempotent() {
        let e = env("mock");
        let mock = MockConnector::new();
        let r = sync_account(&e.db, &e.auth, &e.clock, &mock, &e.account_id)
            .await
            .unwrap();
        assert_eq!(r.new_contents, 10);
        assert_eq!(r.refreshed_contents, 0);
        assert_eq!(r.unclassified, 0);
        // 2026-08-01 .. 2026-11-17 inclusive
        assert_eq!(r.historic_days, 109);
        assert_eq!(count(&e.db, "SELECT COUNT(*) FROM contents"), 10);
        assert_eq!(
            count(&e.db, "SELECT COUNT(*) FROM contents WHERE kind = 'short'"),
            4 // 0, 3, 6, 9
        );
        let acc = accounts::get(&e.db.lock().unwrap(), &e.account_id).unwrap();
        assert_eq!(acc.display_name, "Mock Creator");
        assert!(acc.last_sync_at.is_some());

        // Second sync: nothing new, everything refreshed, history only re-fetches the overlap.
        let r2 = sync_account(&e.db, &e.auth, &e.clock, &mock, &e.account_id)
            .await
            .unwrap();
        assert_eq!(r2.new_contents, 0);
        assert_eq!(r2.refreshed_contents, 10);
        assert_eq!(r2.historic_days, 4);
        assert_eq!(count(&e.db, "SELECT COUNT(*) FROM contents"), 10);
        assert_eq!(
            count(&e.db, "SELECT COUNT(*) FROM contents WHERE kind = 'short'"),
            4
        );
    }

    #[test]
    fn followers_are_rebuilt_backwards_from_real_counts() {
        let e = env("mock");
        let conn = e.db.lock().unwrap();
        let put = |day: &str, metric: &str, v: f64, src: &str| {
            snapshots::upsert_account_snapshot(&conn, &e.account_id, day, metric, v, src, "t")
                .unwrap()
        };
        for (day, gained, lost) in [
            ("2026-11-10", 5.0, 1.0),
            ("2026-11-11", 3.0, 0.0),
            ("2026-11-12", 10.0, 2.0),
        ] {
            put(day, "subscribers_gained", gained, "historic");
            put(day, "subscribers_lost", lost, "historic");
        }
        put("2026-11-12", "followers", 100.0, "current");
        reconstruct_followers(&conn, &e.account_id, "t").unwrap();
        let s = snapshots::account_points(&conn, &e.account_id, "followers").unwrap();
        assert_eq!(
            s,
            vec![
                ("2026-11-10".into(), 89.0, "historic".into()),
                ("2026-11-11".into(), 92.0, "historic".into()),
                ("2026-11-12".into(), 100.0, "current".into()),
            ]
        );
    }

    #[tokio::test]
    async fn concurrent_sync_is_rejected() {
        let locks = SyncLocks::default();
        let g = locks.try_begin("a").unwrap();
        assert!(matches!(
            locks.try_begin("a"),
            Err(AppError::SyncInProgress)
        ));
        assert!(locks.try_begin("b").is_ok());
        drop(g);
        assert!(locks.try_begin("a").is_ok());
    }

    /// Full YouTube sync against a fake Google: incremental listing, quota and degradation.
    #[tokio::test]
    async fn youtube_sync_against_fake_api() {
        use crate::auth::TokenSet;
        use crate::connectors::youtube::test_connector;
        use mockito::Matcher;

        let mut srv = mockito::Server::new_async().await;
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(Matcher::Any)
            .with_body(
                r#"{"items":[{"id":"UCgh0st","snippet":{"title":"gh0stymc","customUrl":"@gh0stymc",
                   "publishedAt":"2026-11-10T00:00:00Z"},
                   "statistics":{"subscriberCount":"50","viewCount":"900","videoCount":"2"}}]}"#,
            )
            .create_async()
            .await;
        let playlist = srv
            .mock("GET", "/youtube/v3/playlistItems")
            .match_query(Matcher::UrlEncoded("playlistId".into(), "UUgh0st".into()))
            .with_body(
                r#"{"items":[{"contentDetails":{"videoId":"s1"}},{"contentDetails":{"videoId":"v1"}}]}"#,
            )
            .create_async()
            .await;
        srv.mock("GET", "/youtube/v3/videos")
            .match_query(Matcher::Any)
            .with_body(
                r#"{"items":[
                   {"id":"s1","snippet":{"title":"short"},"contentDetails":{"duration":"PT30S"},
                    "statistics":{"viewCount":"500"}},
                   {"id":"v1","snippet":{"title":"video"},"contentDetails":{"duration":"PT10M"},
                    "statistics":{"viewCount":"400"}}]}"#,
            )
            .create_async()
            .await;
        srv.mock("HEAD", "/shorts/s1")
            .with_status(200)
            .create_async()
            .await;
        srv.mock("HEAD", "/shorts/v1")
            .with_status(303)
            .with_header("location", "/watch?v=v1")
            .create_async()
            .await;
        srv.mock("GET", "/v2/reports")
            .match_query(Matcher::Any)
            .with_body(
                r#"{"columnHeaders":[{"name":"day"},{"name":"subscribersGained"},{"name":"views"}],
                   "rows":[["2026-11-15",4,100],["2026-11-16",6,120]]}"#,
            )
            .create_async()
            .await;

        let e = env("youtube");
        e.auth
            .save(
                &e.account_id,
                &TokenSet {
                    access_token: "at".into(),
                    refresh_token: Some("rt".into()),
                    expires_at: i64::MAX,
                    scopes: vec![],
                },
            )
            .await
            .unwrap();
        let yt = test_connector(&srv.url());
        let r = sync_account(&e.db, &e.auth, &e.clock, &yt, &e.account_id)
            .await
            .unwrap();
        assert_eq!(r.new_contents, 2);
        assert_eq!(r.unclassified, 0);
        // channels + playlistItems + videos = 3 units
        assert_eq!(r.quota_used, 3);
        assert!(!r.quota_limited);
        playlist.assert_async().await;
        assert_eq!(
            count(&e.db, "SELECT COUNT(*) FROM contents WHERE kind = 'short'"),
            1
        );
        // 50 today and on the 16th (no data for the 17th); 50 - 6 = 44 on the 15th.
        let f =
            snapshots::account_points(&e.db.lock().unwrap(), &e.account_id, "followers").unwrap();
        assert_eq!(f.last().unwrap().1, 50.0);
        assert!(f.contains(&("2026-11-15".into(), 44.0, "historic".into())));
        let used: i64 = count(&e.db, "SELECT SUM(units_used) FROM quota_ledger");
        assert_eq!(used, 3);
    }

    #[tokio::test]
    async fn exhausted_budget_degrades_to_history_only() {
        let e = env("youtube");
        {
            let mut conn = e.db.lock().unwrap();
            let day = quota::pacific_day(e.clock.now());
            quota::try_spend(
                &mut conn,
                "youtube",
                &day,
                quota::YOUTUBE_SYNC_BUDGET,
                u32::MAX,
            )
            .unwrap();
        }
        let mut srv = mockito::Server::new_async().await;
        let data_api = srv
            .mock("GET", Matcher::Regex("^/youtube/".into()))
            .expect(0)
            .create_async()
            .await;
        srv.mock("GET", "/v2/reports")
            .match_query(Matcher::Any)
            .with_body(
                r#"{"columnHeaders":[{"name":"day"},{"name":"views"}],"rows":[["2026-11-16",1]]}"#,
            )
            .create_async()
            .await;
        e.auth
            .save(
                &e.account_id,
                &crate::auth::TokenSet {
                    access_token: "at".into(),
                    refresh_token: None,
                    expires_at: i64::MAX,
                    scopes: vec![],
                },
            )
            .await
            .unwrap();
        let yt = crate::connectors::youtube::test_connector(&srv.url());
        let r = sync_account(&e.db, &e.auth, &e.clock, &yt, &e.account_id)
            .await
            .unwrap();
        data_api.assert_async().await;
        assert!(r.quota_limited);
        assert_eq!(r.quota_used, 0);
        assert_eq!(r.historic_days, 1);
    }

    use mockito::Matcher;
}
