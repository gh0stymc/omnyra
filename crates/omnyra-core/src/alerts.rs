//! Alert computation (step 2c): follower milestones and per-content performance outliers.
//!
//! Run after a sync. The pure helpers ([`zscore`], [`milestones_crossed`]) are unit-tested;
//! [`evaluate_account`] reads the snapshots, inserts the new alerts (deduplicated) and
//! returns them so the app layer can raise a system notification.
//!
//! Engagement-drop alerts (4-week moving average) are intentionally left for later: they
//! need weeks of continuous daily data this app has only started collecting, so firing them
//! now would be noise. See PROGRESS (2c).

use crate::db::repo::alerts::{self, kind, severity, Alert, AlertPayload, NewAlert};
use crate::sync::EARLY_VIEWS_METRIC;
use crate::Result;
use rusqlite::{params, Connection};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

/// A content's first-week views must be at least this many standard deviations from the
/// same-kind baseline to raise a performance alert.
const Z_THRESHOLD: f64 = 2.0;
/// Need at least this many same-kind contents before the baseline is meaningful.
const MIN_SAMPLE: usize = 8;
/// Only raise performance alerts for contents published within this many days (the first-week
/// views metric lands ~10 days after publishing, so this is really a ~10–21 day window).
const RECENT_DAYS: i64 = 21;

/// Population standard-score of `x` within `population` (which must contain `x`).
/// `None` when there are fewer than two points or the values are all equal.
pub fn zscore(x: f64, population: &[f64]) -> Option<f64> {
    let n = population.len();
    if n < 2 {
        return None;
    }
    let mean = population.iter().sum::<f64>() / n as f64;
    let var = population.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let sd = var.sqrt();
    if sd <= f64::EPSILON {
        return None;
    }
    Some((x - mean) / sd)
}

/// Milestone thresholds (`{1,5}·10^k`, k ≥ 2, i.e. 100, 500, 1000, 5000, …) strictly above
/// `prev` and at most `cur`. Empty when `cur <= prev` or nothing was crossed.
pub fn milestones_crossed(prev: f64, cur: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if cur <= prev {
        return out;
    }
    let mut base = 100.0_f64; // 10^2
    while base <= cur {
        for t in [base, base * 5.0] {
            if t > prev && t <= cur {
                out.push(t);
            }
        }
        base *= 10.0;
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

/// The two most recent follower totals for an account: `(previous, current)`.
/// `None` until at least two distinct days exist (so the historical backfill on the first
/// sync does not fire a milestone for every threshold ever crossed).
fn latest_followers(conn: &Connection, account_id: &str) -> Result<Option<(f64, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT value FROM account_snapshots
         WHERE account_id = ?1 AND metric = 'followers' ORDER BY day DESC LIMIT 2",
    )?;
    let vals: Vec<f64> = stmt
        .query_map(params![account_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(match vals.as_slice() {
        [cur, prev] => Some((*prev, *cur)),
        _ => None,
    })
}

struct Candidate {
    content_id: String,
    title: Option<String>,
    content_kind: String,
    published_at: String,
    value: f64,
}

/// Contents with a first-week-views snapshot, with their kind and title.
fn early_views(conn: &Connection, account_id: &str) -> Result<Vec<Candidate>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.title, c.kind, c.published_at, s.value
         FROM contents c
         JOIN content_snapshots s ON s.content_id = c.id AND s.metric = ?2
         WHERE c.account_id = ?1 AND c.published_at IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![account_id, EARLY_VIEWS_METRIC], |r| {
        Ok(Candidate {
            content_id: r.get(0)?,
            title: r.get(1)?,
            content_kind: r.get(2)?,
            published_at: r.get(3)?,
            value: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Computes and stores new alerts for an account after a sync. Returns the alerts created
/// now (already deduplicated against the ones stored before).
pub fn evaluate_account(
    conn: &Connection,
    now: OffsetDateTime,
    account_id: &str,
) -> Result<Vec<Alert>> {
    let now_iso = now.format(&Rfc3339).expect("rfc3339");
    let mut created = Vec::new();

    // --- Follower milestones ---------------------------------------------------------
    if let Some((prev, cur)) = latest_followers(conn, account_id)? {
        for threshold in milestones_crossed(prev, cur) {
            if alerts::milestone_exists(conn, account_id, threshold)? {
                continue;
            }
            created.push(alerts::insert(
                conn,
                &now_iso,
                &NewAlert {
                    kind: kind::MILESTONE.into(),
                    severity: severity::POSITIVE.into(),
                    account_id: Some(account_id.into()),
                    content_id: None,
                    payload: AlertPayload {
                        metric: Some("followers".into()),
                        value: Some(threshold),
                        ..Default::default()
                    },
                },
            )?);
        }
    }

    // --- Per-content performance outliers --------------------------------------------
    let candidates = early_views(conn, account_id)?;
    // Baseline per kind: every same-kind value (all ages; first-week views are age-normalised).
    let recent_cutoff = now - Duration::days(RECENT_DAYS);
    for c in &candidates {
        if alerts::performance_exists(conn, &c.content_id)? {
            continue;
        }
        let Ok(published) = OffsetDateTime::parse(&c.published_at, &Rfc3339) else {
            continue;
        };
        if published < recent_cutoff {
            continue; // only alert on recent contents
        }
        let population: Vec<f64> = candidates
            .iter()
            .filter(|o| o.content_kind == c.content_kind)
            .map(|o| o.value)
            .collect();
        if population.len() < MIN_SAMPLE {
            continue;
        }
        let Some(z) = zscore(c.value, &population) else {
            continue;
        };
        if z.abs() < Z_THRESHOLD {
            continue;
        }
        let (k, sev) = if z > 0.0 {
            (kind::OVERPERFORMING, severity::POSITIVE)
        } else {
            (kind::UNDERPERFORMING, severity::WARNING)
        };
        created.push(alerts::insert(
            conn,
            &now_iso,
            &NewAlert {
                kind: k.into(),
                severity: sev.into(),
                account_id: Some(account_id.into()),
                content_id: Some(c.content_id.clone()),
                payload: AlertPayload {
                    value: Some(c.value),
                    content_title: c.title.clone(),
                    content_kind: Some(c.content_kind.clone()),
                    z: Some(z),
                    ..Default::default()
                },
            },
        )?);
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use time::macros::datetime;

    #[test]
    fn zscore_needs_spread_and_points() {
        assert_eq!(zscore(1.0, &[1.0]), None);
        assert_eq!(zscore(1.0, &[5.0, 5.0, 5.0]), None);
        // mean 2, variance (1+1+4)/3 = 2, sd √2 → (4-2)/√2 = √2
        let z = zscore(4.0, &[1.0, 1.0, 4.0]).unwrap();
        assert!((z - std::f64::consts::SQRT_2).abs() < 0.01, "z={z}");
    }

    #[test]
    fn milestones_between_values() {
        assert_eq!(milestones_crossed(90.0, 120.0), vec![100.0]);
        assert_eq!(milestones_crossed(400.0, 1200.0), vec![500.0, 1000.0]);
        // Exact boundary: inclusive on cur, exclusive on prev.
        assert_eq!(milestones_crossed(500.0, 1000.0), vec![1000.0]);
        assert!(milestones_crossed(1000.0, 999.0).is_empty());
        assert!(milestones_crossed(100.0, 100.0).is_empty());
        assert_eq!(milestones_crossed(9000.0, 60000.0), vec![10000.0, 50000.0]);
    }

    fn seed_account(conn: &Connection) {
        conn.execute(
            "INSERT INTO accounts (id, platform, external_id, display_name, connected_at)
             VALUES ('a1', 'youtube', 'x', 'Chan', '2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    fn add_followers(conn: &Connection, day: &str, value: f64) {
        conn.execute(
            "INSERT INTO account_snapshots (account_id, day, metric, value, source, captured_at)
             VALUES ('a1', ?1, 'followers', ?2, 'current', '2026-11-18T00:00:00Z')",
            params![day, value],
        )
        .unwrap();
    }

    fn add_content(conn: &Connection, id: &str, kind: &str, published_at: &str, views_7d: f64) {
        conn.execute(
            "INSERT INTO contents (id, account_id, external_id, kind, published_at, first_seen_at, last_fetched_at)
             VALUES (?1, 'a1', ?1, ?2, ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![id, kind, published_at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO content_snapshots (content_id, day, metric, value, source, captured_at)
             VALUES (?1, '2026-01-01', ?2, ?3, 'historic', '2026-11-18T00:00:00Z')",
            params![id, EARLY_VIEWS_METRIC, views_7d],
        )
        .unwrap();
    }

    #[test]
    fn milestone_fires_once_and_dedups() {
        let conn = open_in_memory().unwrap();
        seed_account(&conn);
        add_followers(&conn, "2026-11-16", 940.0);
        add_followers(&conn, "2026-11-17", 1010.0); // crosses 1000
        let now = datetime!(2026-11-17 12:00 UTC);

        let first = evaluate_account(&conn, now, "a1").unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, kind::MILESTONE);
        assert_eq!(first[0].payload.value, Some(1000.0));

        // Running again does not duplicate it.
        let second = evaluate_account(&conn, now, "a1").unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn single_day_history_fires_no_milestone() {
        let conn = open_in_memory().unwrap();
        seed_account(&conn);
        add_followers(&conn, "2026-11-17", 1010.0);
        let now = datetime!(2026-11-17 12:00 UTC);
        assert!(evaluate_account(&conn, now, "a1").unwrap().is_empty());
    }

    #[test]
    fn performance_flags_recent_outlier_only() {
        let conn = open_in_memory().unwrap();
        seed_account(&conn);
        let now = datetime!(2026-11-18 12:00 UTC);
        // Baseline: nine videos around 100 views.
        for (i, v) in [100.0, 110.0, 90.0, 105.0, 95.0, 100.0, 98.0, 102.0, 100.0]
            .iter()
            .enumerate()
        {
            // Old (published long ago): part of the baseline but never alerted on.
            add_content(
                &conn,
                &format!("old{i}"),
                "video",
                "2026-09-01T00:00:00Z",
                *v,
            );
        }
        // A recent breakout video.
        add_content(&conn, "hit", "video", "2026-11-05T00:00:00Z", 900.0);

        let created = evaluate_account(&conn, now, "a1").unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].kind, kind::OVERPERFORMING);
        assert_eq!(created[0].content_id.as_deref(), Some("hit"));
        assert!(created[0].payload.z.unwrap() > Z_THRESHOLD);

        // Dedup: nothing new on a second pass.
        assert!(evaluate_account(&conn, now, "a1").unwrap().is_empty());
    }

    #[test]
    fn small_sample_raises_nothing() {
        let conn = open_in_memory().unwrap();
        seed_account(&conn);
        let now = datetime!(2026-11-18 12:00 UTC);
        add_content(&conn, "a", "video", "2026-11-10T00:00:00Z", 100.0);
        add_content(&conn, "b", "video", "2026-11-11T00:00:00Z", 5000.0);
        assert!(evaluate_account(&conn, now, "a1").unwrap().is_empty());
    }
}
