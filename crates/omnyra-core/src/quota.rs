//! Daily API quota ledger (architecture R5). YouTube's Data API gives 10,000 units per day
//! per Google Cloud project, reset at midnight Pacific time. Every metered call is checked
//! against a budget *before* it is made, so background work can never eat the units that
//! uploads need.

use crate::clock::Clock;
use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::sync::{Arc, Mutex};
use time::{Date, Duration, Month, OffsetDateTime, Weekday};

/// Quota buckets (rows of `quota_ledger`).
pub const YOUTUBE: &str = "youtube";
/// `videos.insert` has its own bucket: 100 calls a day, 1 unit each (checked 2026-11-17 on
/// developers.google.com/youtube/v3/determine_quota_cost).
pub const YOUTUBE_UPLOAD: &str = "youtube.upload";

/// YouTube Data API daily quota per project.
pub const YOUTUBE_DAILY_UNITS: u32 = 10_000;
pub const YOUTUBE_DAILY_UPLOADS: u32 = 100;
/// Share of the daily quota that syncing may use; the rest is kept for uploads (custom
/// thumbnails and playlist inserts cost 50 units each) and later features (R5).
pub const YOUTUBE_SYNC_BUDGET: u32 = YOUTUBE_DAILY_UNITS * 60 / 100;

/// What a quota gate is spending for: each purpose has its own budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Sync,
    Upload,
}

/// Budgets for `platform` and `purpose`; empty for platforms without a metered quota.
pub fn budgets(platform: &str, purpose: Purpose) -> Vec<(&'static str, u32)> {
    match (platform, purpose) {
        ("youtube", Purpose::Sync) => vec![(YOUTUBE, YOUTUBE_SYNC_BUDGET)],
        ("youtube", Purpose::Upload) => vec![
            (YOUTUBE, YOUTUBE_DAILY_UNITS),
            (YOUTUBE_UPLOAD, YOUTUBE_DAILY_UPLOADS),
        ],
        _ => Vec::new(),
    }
}

/// Charges quota units before a metered API call. Connectors receive one through
/// [`crate::connectors::AccountCtx`].
pub trait QuotaGate: Send + Sync {
    /// Records `units` in `bucket` if they fit in the budget; `QuotaExceeded` (and nothing
    /// recorded) otherwise. Buckets without a budget are not metered.
    fn spend(&self, bucket: &str, units: u32) -> Result<()>;
}

/// No metering (mock connector, the connect flow, tests).
pub struct Unmetered;

impl QuotaGate for Unmetered {
    fn spend(&self, _bucket: &str, _units: u32) -> Result<()> {
        Ok(())
    }
}

/// Quota tracked in SQLite (`quota_ledger`), per bucket and Pacific-time day.
pub struct DbQuota {
    pub db: Arc<Mutex<Connection>>,
    pub clock: Arc<dyn Clock>,
    pub budgets: Vec<(&'static str, u32)>,
}

impl DbQuota {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        clock: Arc<dyn Clock>,
        platform: &str,
        purpose: Purpose,
    ) -> Self {
        Self {
            db,
            clock,
            budgets: budgets(platform, purpose),
        }
    }
}

impl QuotaGate for DbQuota {
    fn spend(&self, bucket: &str, units: u32) -> Result<()> {
        let Some(&(_, budget)) = self.budgets.iter().find(|(b, _)| *b == bucket) else {
            return Ok(());
        };
        let mut conn = self.db.lock().map_err(|_| AppError::Internal)?;
        try_spend(
            &mut conn,
            bucket,
            &pacific_day(self.clock.now()),
            units,
            budget,
        )
    }
}

/// Check and record in one transaction, so two concurrent syncs cannot both pass the check.
pub fn try_spend(
    conn: &mut Connection,
    platform: &str,
    day: &str,
    units: u32,
    budget: u32,
) -> Result<()> {
    let tx = conn.transaction()?;
    let used = used_on(&tx, platform, day)?;
    if used + units > budget {
        return Err(AppError::QuotaExceeded);
    }
    tx.execute(
        "INSERT INTO quota_ledger (platform, quota_day, units_used) VALUES (?1, ?2, ?3)
         ON CONFLICT (platform, quota_day) DO UPDATE SET units_used = units_used + ?3",
        params![platform, day, units],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn used_on(conn: &Connection, platform: &str, day: &str) -> Result<u32> {
    Ok(conn
        .query_row(
            "SELECT units_used FROM quota_ledger WHERE platform = ?1 AND quota_day = ?2",
            params![platform, day],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsage {
    /// Bucket: `youtube` (Data API units) or `youtube.upload` (uploads).
    pub bucket: String,
    /// Quota day (`YYYY-MM-DD`, Pacific time).
    pub day: String,
    pub used: u32,
    pub daily_limit: u32,
    /// What syncing may use of `daily_limit`.
    pub sync_budget: u32,
}

/// Today's usage for the metered platforms.
pub fn usage(conn: &Connection, now: OffsetDateTime) -> Result<Vec<QuotaUsage>> {
    let day = pacific_day(now);
    [
        (YOUTUBE, YOUTUBE_DAILY_UNITS, YOUTUBE_SYNC_BUDGET),
        (YOUTUBE_UPLOAD, YOUTUBE_DAILY_UPLOADS, 0),
    ]
    .into_iter()
    .map(|(bucket, daily_limit, sync_budget)| {
        Ok(QuotaUsage {
            bucket: bucket.into(),
            used: used_on(conn, bucket, &day)?,
            day: day.clone(),
            daily_limit,
            sync_budget,
        })
    })
    .collect()
}

/// Current day in US Pacific time (when YouTube quota resets), as `YYYY-MM-DD`.
/// DST: from the second Sunday of March at 02:00 PST (10:00 UTC) to the first Sunday of
/// November at 02:00 PDT (09:00 UTC).
pub fn pacific_day(now: OffsetDateTime) -> String {
    let now = now.to_offset(time::UtcOffset::UTC);
    let year = now.year();
    let dst_start = nth_sunday(year, Month::March, 2)
        .with_hms(10, 0, 0)
        .expect("valid time")
        .assume_utc();
    let dst_end = nth_sunday(year, Month::November, 1)
        .with_hms(9, 0, 0)
        .expect("valid time")
        .assume_utc();
    let offset_h = if now >= dst_start && now < dst_end {
        7
    } else {
        8
    };
    crate::format_day(now - Duration::hours(offset_h))
}

fn nth_sunday(year: i32, month: Month, n: u8) -> Date {
    let first = Date::from_calendar_date(year, month, 1).expect("valid date");
    let to_sunday = (7 + 7 - first.weekday().number_days_from_sunday() as i64) % 7;
    let d = first + Duration::days(to_sunday + 7 * (n as i64 - 1));
    debug_assert_eq!(d.weekday(), Weekday::Sunday);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use time::macros::datetime;

    #[test]
    fn pacific_day_follows_dst() {
        // Winter (PST, UTC-8): 07:59 UTC is still the previous day.
        assert_eq!(pacific_day(datetime!(2026-01-15 07:59 UTC)), "2026-01-14");
        assert_eq!(pacific_day(datetime!(2026-01-15 08:00 UTC)), "2026-01-15");
        // Summer (PDT, UTC-7).
        assert_eq!(pacific_day(datetime!(2026-07-15 06:59 UTC)), "2026-07-14");
        assert_eq!(pacific_day(datetime!(2026-07-15 07:00 UTC)), "2026-07-15");
    }

    #[test]
    fn dst_switch_dates() {
        // 2026: DST from Sunday 8 March to Sunday 1 November.
        assert_eq!(
            nth_sunday(2026, Month::March, 2),
            time::macros::date!(2026 - 03 - 08)
        );
        assert_eq!(
            nth_sunday(2026, Month::November, 1),
            time::macros::date!(2026 - 11 - 01)
        );
        // 8 March 2026 07:30 UTC = 23:30 PST on the 7th (DST not started yet).
        assert_eq!(pacific_day(datetime!(2026-03-08 07:30 UTC)), "2026-03-07");
        // 8 March 2026 23:30 UTC = 16:30 PDT on the 8th.
        assert_eq!(pacific_day(datetime!(2026-03-08 23:30 UTC)), "2026-03-08");
        // 1 November 2026 07:30 UTC = 00:30 PDT on the 1st.
        assert_eq!(pacific_day(datetime!(2026-11-01 07:30 UTC)), "2026-11-01");
        // 2 November 2026 07:30 UTC = 23:30 PST on the 1st (DST over).
        assert_eq!(pacific_day(datetime!(2026-11-02 07:30 UTC)), "2026-11-01");
    }

    #[test]
    fn spend_respects_budget() {
        let mut conn = open_in_memory().unwrap();
        try_spend(&mut conn, "youtube", "2026-11-17", 5, 10).unwrap();
        try_spend(&mut conn, "youtube", "2026-11-17", 5, 10).unwrap();
        assert!(matches!(
            try_spend(&mut conn, "youtube", "2026-11-17", 1, 10),
            Err(AppError::QuotaExceeded)
        ));
        assert_eq!(used_on(&conn, "youtube", "2026-11-17").unwrap(), 10);
        // A new quota day starts from zero.
        try_spend(&mut conn, "youtube", "2026-11-18", 1, 10).unwrap();
    }
}
