//! "Best time to publish" (step 1c): which weekday and hour, in the user's time zone,
//! have historically given this account's contents more views than usual.
//!
//! Algorithm (see `docs/ARCHITECTURE.md` §6, refined here):
//! 1. **Sample:** contents of one format (Shorts and regular videos never mix; live streams
//!    are excluded because their timing is the stream's, not a publishing choice) that have
//!    first-week views (`views_7d`, stored by the sync). Public only, unless the user opts
//!    into unlisted ones: their views depend on where the link is shared, so that result is
//!    only indicative.
//! 2. **Performance:** `y = ln(1 + views_7d)`. The log tames viral outliers; comparing the
//!    first week only removes the advantage older videos have from simply being older.
//! 3. **Recency weight:** `w = 0.5^(age_days / 180)`: what worked six months ago counts half.
//! 4. **Score:** `s = y − weighted mean(y)`. 0 means "as usual for this channel";
//!    `exp(s)` is the multiplier on views (×1.5 = 50 % more than usual).
//! 5. **Buckets:** each content adds `w·s` and `w` to its (weekday, hour) cell and half of
//!    that to the hour before and after (publishing at 17:55 or 18:05 is the same decision).
//! 6. **Smoothing:** `cell = Σ w·s / (Σ w + 1)`, a Bayesian shrink towards 0: a cell backed
//!    by little weight stays close to "as usual" instead of jumping on one lucky video.
//! 7. **Not enough data:** fewer than 15 contents → no map at all. Cells with total weight
//!    below 0.75 are marked as not having enough data and are never recommended.

use crate::db::repo::contents::{self, Performance};
use crate::sync::EARLY_VIEWS_METRIC;
use crate::{AppError, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;
use time::OffsetDateTime;

pub const MIN_CONTENTS: u32 = 15;
const HALF_LIFE_DAYS: f64 = 180.0;
/// Weight of the "as usual" prior in each cell.
const PRIOR_WEIGHT: f64 = 1.0;
/// Share of a content's weight that also goes to the neighbouring hours.
const NEIGHBOUR_SHARE: f64 = 0.5;
const MIN_CELL_WEIGHT: f64 = 0.75;
const TOP: usize = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Video,
    Short,
}

impl Format {
    fn kind(self) -> &'static str {
        match self {
            Format::Video => crate::connectors::kind::VIDEO,
            Format::Short => crate::connectors::kind::SHORT,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BestTimeQuery {
    pub format: Format,
    pub include_unlisted: bool,
    /// IANA time zone (`Europe/Madrid`).
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
    pub hour: u8,
    /// Smoothed score; 0 = as usual.
    pub score: f64,
    /// `exp(score)`: multiplier on first-week views.
    pub factor: f64,
    /// Total recency weight backing the cell (neighbours included).
    pub weight: f64,
    /// Contents published in exactly this hour.
    pub contents: u32,
    pub enough_data: bool,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BestTime {
    pub format: Format,
    pub include_unlisted: bool,
    pub timezone: String,
    /// Contents in the sample.
    pub sample: u32,
    pub needed: u32,
    /// False when `sample < needed`: `cells` and `best` are then empty.
    pub sufficient: bool,
    /// 7 × 24 cells, Monday 00:00 first.
    pub cells: Vec<Cell>,
    /// Up to three cells with enough data and a positive score, best first, never two
    /// consecutive hours.
    pub best: Vec<Cell>,
}

pub fn best_time(
    conn: &Connection,
    account_id: &str,
    q: &BestTimeQuery,
    now: OffsetDateTime,
) -> Result<BestTime> {
    let tz = jiff::tz::TimeZone::get(&q.timezone).map_err(|_| AppError::Validation {
        field: "timezone".into(),
    })?;
    let rows = contents::performance(conn, account_id, EARLY_VIEWS_METRIC)?;
    Ok(compute(&rows, q, &tz, now))
}

struct Sample {
    weekday: u8,
    hour: u8,
    y: f64,
    w: f64,
}

pub(crate) fn compute(
    rows: &[Performance],
    q: &BestTimeQuery,
    tz: &jiff::tz::TimeZone,
    now: OffsetDateTime,
) -> BestTime {
    let now_s = now.unix_timestamp();
    let sample: Vec<Sample> = rows
        .iter()
        .filter(|r| r.kind == q.format.kind())
        .filter(|r| match r.visibility.as_deref() {
            Some("public") => true,
            Some("unlisted") => q.include_unlisted,
            _ => false,
        })
        .filter_map(|r| {
            let ts: jiff::Timestamp = r.published_at.parse().ok()?;
            let local = ts.to_zoned(tz.clone());
            let age_days = ((now_s - ts.as_second()).max(0) as f64) / 86_400.0;
            Some(Sample {
                weekday: local.weekday().to_monday_zero_offset() as u8,
                hour: local.hour() as u8,
                y: (1.0 + r.value.max(0.0)).ln(),
                w: 0.5f64.powf(age_days / HALF_LIFE_DAYS),
            })
        })
        .collect();

    let n = sample.len() as u32;
    let mut out = BestTime {
        format: q.format,
        include_unlisted: q.include_unlisted,
        timezone: q.timezone.clone(),
        sample: n,
        needed: MIN_CONTENTS,
        sufficient: n >= MIN_CONTENTS,
        cells: Vec::new(),
        best: Vec::new(),
    };
    if !out.sufficient {
        return out;
    }

    let total_w: f64 = sample.iter().map(|s| s.w).sum();
    let mean = sample.iter().map(|s| s.w * s.y).sum::<f64>() / total_w;

    // Index = weekday * 24 + hour; hours wrap into the previous/next day (Sunday ↔ Monday).
    let mut ws = [0.0f64; 168];
    let mut w = [0.0f64; 168];
    let mut count = [0u32; 168];
    for s in &sample {
        let i = s.weekday as usize * 24 + s.hour as usize;
        let score = s.y - mean;
        count[i] += 1;
        for (j, share) in [
            (i, 1.0),
            ((i + 167) % 168, NEIGHBOUR_SHARE),
            ((i + 1) % 168, NEIGHBOUR_SHARE),
        ] {
            ws[j] += share * s.w * score;
            w[j] += share * s.w;
        }
    }

    out.cells = (0..168)
        .map(|i| {
            let score = ws[i] / (w[i] + PRIOR_WEIGHT);
            Cell {
                weekday: (i / 24) as u8,
                hour: (i % 24) as u8,
                score,
                factor: score.exp(),
                weight: w[i],
                contents: count[i],
                enough_data: w[i] >= MIN_CELL_WEIGHT,
            }
        })
        .collect();
    let mut best: Vec<Cell> = out
        .cells
        .iter()
        .filter(|c| c.enough_data && c.score > 0.0)
        .cloned()
        .collect();
    best.sort_by(|a, b| b.score.total_cmp(&a.score));
    // Neighbouring hours share weight, so a peak shows up as 2–3 cells: keep only its top.
    for c in best {
        let i = c.weekday as i32 * 24 + c.hour as i32;
        let near = out.best.iter().any(|b| {
            let d = (b.weekday as i32 * 24 + b.hour as i32 - i).rem_euclid(168);
            d <= 1 || d >= 167
        });
        if !near {
            out.best.push(c);
        }
        if out.best.len() == TOP {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::kind;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-09-01 12:00:00 UTC);

    fn row(kind: &str, vis: Option<&str>, published: &str, value: f64) -> Performance {
        Performance {
            kind: kind.into(),
            visibility: vis.map(Into::into),
            published_at: published.into(),
            value,
        }
    }

    fn video(published: &str, value: f64) -> Performance {
        row(kind::VIDEO, Some("public"), published, value)
    }

    /// `n` low-view videos on Wednesday 12:00 UTC (never Monday 18 nor Friday 18 locally).
    fn filler(n: usize, value: f64) -> Vec<Performance> {
        (0..n)
            .map(|_| video("2026-06-03T12:00:00Z", value))
            .collect()
    }

    fn query(format: Format, unlisted: bool, tz: &str) -> BestTimeQuery {
        BestTimeQuery {
            format,
            include_unlisted: unlisted,
            timezone: tz.into(),
        }
    }

    fn run(rows: &[Performance], q: &BestTimeQuery, tz: &str) -> BestTime {
        compute(rows, q, &jiff::tz::TimeZone::get(tz).unwrap(), NOW)
    }

    fn cell(bt: &BestTime, weekday: usize, hour: usize) -> &Cell {
        &bt.cells[weekday * 24 + hour]
    }

    #[test]
    fn fewer_than_min_contents_is_insufficient() {
        let mut rows = filler(14, 100.0);
        // Rows that do not count must not be added to the sample.
        rows.push(row(
            kind::SHORT,
            Some("public"),
            "2026-06-03T12:00:00Z",
            1.0,
        ));
        rows.push(row(
            kind::VIDEO,
            Some("private"),
            "2026-06-03T12:00:00Z",
            1.0,
        ));
        let bt = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        assert_eq!(bt.sample, 14);
        assert_eq!(bt.needed, MIN_CONTENTS);
        assert!(!bt.sufficient);
        assert!(bt.cells.is_empty());
        assert!(bt.best.is_empty());

        let bt = run(
            &filler(15, 100.0),
            &query(Format::Video, false, "UTC"),
            "UTC",
        );
        assert!(bt.sufficient);
        assert_eq!(bt.cells.len(), 168);
    }

    #[test]
    fn shorts_and_videos_never_mix() {
        let mut rows = filler(3, 100.0);
        rows.extend(
            (0..15).map(|_| row(kind::SHORT, Some("public"), "2026-06-03T12:00:00Z", 50.0)),
        );
        let v = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        assert_eq!(v.sample, 3);
        assert!(!v.sufficient);
        let s = run(&rows, &query(Format::Short, false, "UTC"), "UTC");
        assert_eq!(s.sample, 15);
        assert!(s.sufficient);
    }

    #[test]
    fn streams_private_and_unknown_visibility_are_excluded() {
        let mut rows = filler(15, 100.0);
        rows.push(row(
            kind::STREAM,
            Some("public"),
            "2026-06-03T12:00:00Z",
            9.0,
        ));
        rows.push(row(
            kind::VIDEO,
            Some("private"),
            "2026-06-03T12:00:00Z",
            9.0,
        ));
        rows.push(row(kind::VIDEO, None, "2026-06-03T12:00:00Z", 9.0));
        for fmt in [Format::Video, Format::Short] {
            let bt = run(&rows, &query(fmt, true, "UTC"), "UTC");
            assert_eq!(bt.sample, if fmt == Format::Video { 15 } else { 0 });
        }
    }

    #[test]
    fn unlisted_only_with_opt_in() {
        let mut rows = filler(10, 100.0);
        rows.extend(
            (0..5).map(|_| row(kind::VIDEO, Some("unlisted"), "2026-06-03T12:00:00Z", 100.0)),
        );
        let without = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        assert_eq!(without.sample, 10);
        assert!(!without.sufficient);
        let with = run(&rows, &query(Format::Video, true, "UTC"), "UTC");
        assert_eq!(with.sample, 15);
        assert!(with.sufficient);
        assert!(with.include_unlisted);
    }

    #[test]
    fn local_time_conversion_summer_and_winter() {
        // Monday 2026-07-06 16:30Z = 18:30 CEST.
        let mut rows = filler(14, 100.0);
        rows.push(video("2026-07-06T16:30:00Z", 100.0));
        let bt = run(
            &rows,
            &query(Format::Video, false, "Europe/Madrid"),
            "Europe/Madrid",
        );
        assert_eq!(cell(&bt, 0, 18).contents, 1);
        assert_eq!(bt.cells.iter().map(|c| c.contents).sum::<u32>(), 15);

        // Monday 2026-01-05 17:30Z = 18:30 CET.
        let mut rows = filler(14, 100.0);
        rows.push(video("2026-01-05T17:30:00Z", 100.0));
        let bt = run(
            &rows,
            &query(Format::Video, false, "Europe/Madrid"),
            "Europe/Madrid",
        );
        assert_eq!(cell(&bt, 0, 18).contents, 1);

        // Same instant in UTC lands elsewhere.
        let bt = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        assert_eq!(cell(&bt, 0, 17).contents, 1);
        assert_eq!(cell(&bt, 0, 18).contents, 0);
    }

    /// Strong Friday 18:00 (CEST) videos on the given Fridays, weak ones elsewhere.
    fn signal_rows(strong_dates: [&str; 5]) -> Vec<Performance> {
        let mut rows: Vec<Performance> = strong_dates.iter().map(|d| video(d, 5000.0)).collect();
        // 12 weak videos, spread over different days, at 08:00Z (10:00 local).
        rows.extend((0..12).map(|i| video(&format!("2026-07-{:02}T08:00:00Z", 6 + i), 100.0)));
        rows
    }

    const RECENT_FRIDAYS: [&str; 5] = [
        "2026-07-03T16:00:00Z",
        "2026-07-10T16:00:00Z",
        "2026-07-17T16:00:00Z",
        "2026-07-24T16:00:00Z",
        "2026-07-31T16:00:00Z",
    ];
    const OLD_FRIDAYS: [&str; 5] = [
        "2024-07-05T16:00:00Z",
        "2024-07-12T16:00:00Z",
        "2024-07-19T16:00:00Z",
        "2024-07-26T16:00:00Z",
        "2024-08-02T16:00:00Z",
    ];

    #[test]
    fn clear_signal_finds_friday_18_and_best_has_no_adjacent_hours() {
        let q = query(Format::Video, false, "Europe/Madrid");
        let bt = run(&signal_rows(RECENT_FRIDAYS), &q, "Europe/Madrid");
        assert!(bt.sufficient);
        let top = &bt.best[0];
        assert_eq!((top.weekday, top.hour), (4, 18));
        assert!(top.factor > 1.0);
        assert!(top.score > 0.0);
        assert!(top.enough_data);
        assert_eq!(top.contents, 5);
        assert!(bt.best.len() <= TOP);
        for (a, b) in bt
            .best
            .iter()
            .flat_map(|a| bt.best.iter().map(move |b| (a, b)))
        {
            if a == b {
                continue;
            }
            let d = (i32::from(a.weekday) * 24 + i32::from(a.hour)
                - i32::from(b.weekday) * 24
                - i32::from(b.hour))
            .rem_euclid(168);
            assert!(d > 1 && d < 167, "adjacent best cells {a:?} {b:?}");
        }
        // Neighbours of the peak share weight but are never both listed.
        assert!(cell(&bt, 4, 17).score > 0.0);
        assert!(cell(&bt, 4, 19).score > 0.0);
        assert!(!bt
            .best
            .iter()
            .any(|c| c.weekday == 4 && (c.hour == 17 || c.hour == 19)));
    }

    #[test]
    fn single_old_video_is_shrunk_towards_zero() {
        let mut rows = filler(14, 100.0);
        // Friday 12:00 UTC, two years old, huge views.
        rows.push(video("2024-09-06T12:00:00Z", 10_000.0));
        let bt = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        let c = cell(&bt, 4, 12);
        assert_eq!(c.contents, 1);
        let raw = (10_001f64).ln() - (101f64).ln();
        assert!(raw > 4.0);
        assert!(c.score > 0.0);
        assert!(c.score.abs() < raw / 10.0, "score {} raw {raw}", c.score);
        assert!(!c.enough_data);
        assert!(bt.best.iter().all(|b| !(b.weekday == 4 && b.hour == 12)));
    }

    #[test]
    fn recent_signal_scores_higher_than_old_signal() {
        let q = query(Format::Video, false, "Europe/Madrid");
        let recent = run(&signal_rows(RECENT_FRIDAYS), &q, "Europe/Madrid");
        let old = run(&signal_rows(OLD_FRIDAYS), &q, "Europe/Madrid");
        let r = cell(&recent, 4, 18);
        let o = cell(&old, 4, 18);
        assert_eq!((o.weekday, o.hour), (4, 18));
        assert!(r.score > o.score, "recent {} old {}", r.score, o.score);
        assert!(r.weight > o.weight);
        assert!(recent.best[0].score > old.best.first().map_or(0.0, |c| c.score));
    }

    #[test]
    fn neighbour_weight_wraps_from_sunday_23_to_monday_0() {
        let mut rows = filler(14, 100.0);
        // Sunday 2026-07-05 23:30Z, UTC.
        rows.push(video("2026-07-05T23:30:00Z", 100.0));
        let bt = run(&rows, &query(Format::Video, false, "UTC"), "UTC");
        let sun23 = cell(&bt, 6, 23);
        let sun22 = cell(&bt, 6, 22);
        let mon0 = cell(&bt, 0, 0);
        assert_eq!(sun23.contents, 1);
        assert_eq!(mon0.contents, 0);
        assert!(mon0.weight > 0.0);
        assert!((mon0.weight - sun23.weight / 2.0).abs() < 1e-12);
        assert!((mon0.weight - sun22.weight).abs() < 1e-12);
        // Unrelated cell stays empty.
        assert_eq!(cell(&bt, 0, 1).weight, 0.0);
    }

    #[test]
    fn invalid_timezone_is_a_validation_error() {
        let conn = crate::db::open_in_memory().unwrap();
        let q = query(Format::Video, false, "Not/AZone");
        match best_time(&conn, "acc-1", &q, NOW) {
            Err(AppError::Validation { field }) => assert_eq!(field, "timezone"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
