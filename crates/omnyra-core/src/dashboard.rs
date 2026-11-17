//! Read models for the MVP screens (step 1e): per-account and global summaries over a
//! period, series combined across accounts, and top contents.

use crate::connectors::MetricPoint;
use crate::db::repo::{
    accounts::{self, AccountStatus},
    contents::{self, ContentRow},
    snapshots,
};
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;
use time::{macros::format_description, Date, Duration};

/// Daily metrics that are summed over a period.
const SUMMED: &[&str] = &[
    "views",
    "watch_time_s",
    "likes",
    "comments",
    "shares",
    "subscribers_gained",
    "subscribers_lost",
];

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub account_id: String,
    pub platform: String,
    pub display_name: String,
    pub handle: Option<String>,
    pub status: AccountStatus,
    pub last_sync_at: Option<String>,
    /// Latest follower count.
    pub followers: Option<f64>,
    /// Change since the start of the period (None without data on both ends).
    pub followers_change: Option<f64>,
    pub views: f64,
    pub watch_time_s: f64,
    pub likes: f64,
    pub comments: f64,
    pub shares: f64,
    pub subscribers_gained: f64,
    pub subscribers_lost: f64,
    /// Views-weighted average over the period.
    pub avg_view_duration_s: Option<f64>,
    pub contents_published: u32,
    /// Last day with daily (historic) data: analytics lag a few days behind.
    pub data_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub followers: Option<f64>,
    pub followers_change: Option<f64>,
    pub views: f64,
    pub watch_time_s: f64,
    pub contents_published: u32,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// First day of the period (`None` = all time).
    pub from: Option<String>,
    pub to: String,
    pub accounts: Vec<AccountSummary>,
    pub totals: Totals,
}

fn sum_metric(
    conn: &Connection,
    account: &str,
    metric: &str,
    from: Option<&str>,
    to: &str,
) -> Result<f64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(value), 0) FROM account_snapshots
         WHERE account_id = ?1 AND metric = ?2 AND day >= COALESCE(?3, '') AND day <= ?4",
        params![account, metric, from, to],
        |r| r.get(0),
    )?)
}

fn followers_on_or_before(conn: &Connection, account: &str, day: &str) -> Result<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT value FROM account_snapshots
             WHERE account_id = ?1 AND metric = 'followers' AND day <= ?2
             ORDER BY day DESC LIMIT 1",
            params![account, day],
            |r| r.get(0),
        )
        .optional()?)
}

fn followers_first(conn: &Connection, account: &str) -> Result<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT value FROM account_snapshots WHERE account_id = ?1 AND metric = 'followers'
             ORDER BY day LIMIT 1",
            params![account],
            |r| r.get(0),
        )
        .optional()?)
}

/// Accounts shown on the screens: everything not disconnected.
fn visible_accounts(conn: &Connection) -> Result<Vec<accounts::Account>> {
    Ok(accounts::list(conn)?
        .into_iter()
        .filter(|a| a.status != AccountStatus::Disconnected)
        .collect())
}

pub fn summary(conn: &Connection, from: Option<&str>, to: &str) -> Result<Summary> {
    let mut out = Vec::new();
    for a in visible_accounts(conn)? {
        let mut sums = BTreeMap::new();
        for m in SUMMED {
            sums.insert(*m, sum_metric(conn, &a.id, m, from, to)?);
        }
        // Average duration weighted by each day's views.
        let weighted: f64 = conn.query_row(
            "SELECT COALESCE(SUM(d.value * v.value), 0) FROM account_snapshots d
             JOIN account_snapshots v ON v.account_id = d.account_id AND v.day = d.day
                  AND v.metric = 'views'
             WHERE d.account_id = ?1 AND d.metric = 'avg_view_duration_s'
               AND d.day >= COALESCE(?2, '') AND d.day <= ?3",
            params![a.id, from, to],
            |r| r.get(0),
        )?;
        let views = sums["views"];
        let followers = followers_on_or_before(conn, &a.id, to)?;
        let start = match from {
            Some(f) => match followers_on_or_before(conn, &a.id, f)? {
                Some(v) => Some(v),
                None => followers_first(conn, &a.id)?,
            },
            None => followers_first(conn, &a.id)?,
        };
        let published: u32 = conn.query_row(
            "SELECT COUNT(*) FROM contents WHERE account_id = ?1
             AND substr(published_at, 1, 10) >= COALESCE(?2, '')
             AND substr(published_at, 1, 10) <= ?3",
            params![a.id, from, to],
            |r| r.get(0),
        )?;
        out.push(AccountSummary {
            account_id: a.id.clone(),
            platform: a.platform.clone(),
            display_name: a.display_name.clone(),
            handle: a.handle.clone(),
            status: a.status,
            last_sync_at: a.last_sync_at.clone(),
            followers,
            followers_change: followers.zip(start).map(|(now, then)| now - then),
            views,
            watch_time_s: sums["watch_time_s"],
            likes: sums["likes"],
            comments: sums["comments"],
            shares: sums["shares"],
            subscribers_gained: sums["subscribers_gained"],
            subscribers_lost: sums["subscribers_lost"],
            avg_view_duration_s: (views > 0.0).then(|| weighted / views),
            contents_published: published,
            data_until: snapshots::last_day(conn, &a.id, "views", "historic")?,
        });
    }
    let sum_opt = |f: fn(&AccountSummary) -> Option<f64>| {
        let vals: Vec<f64> = out.iter().filter_map(f).collect();
        (!vals.is_empty()).then(|| vals.iter().sum())
    };
    let totals = Totals {
        followers: sum_opt(|a| a.followers),
        followers_change: sum_opt(|a| a.followers_change),
        views: out.iter().map(|a| a.views).sum(),
        watch_time_s: out.iter().map(|a| a.watch_time_s).sum(),
        contents_published: out.iter().map(|a| a.contents_published).sum(),
    };
    Ok(Summary {
        from: from.map(str::to_owned),
        to: to.to_owned(),
        accounts: out,
        totals,
    })
}

const DAY: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

/// `metric` summed across the visible accounts, day by day, from `from` (or the first
/// day with data). Cumulative metrics (`followers`) carry each account's last value
/// forward so that a missing day does not look like a drop.
pub fn combined_series(
    conn: &Connection,
    metric: &str,
    from: Option<&str>,
    to: &str,
) -> Result<Vec<MetricPoint>> {
    let cumulative = metric == "followers";
    let mut per_account = Vec::new();
    for a in visible_accounts(conn)? {
        let pts = snapshots::account_series(conn, &a.id, metric)?;
        if !pts.is_empty() {
            per_account.push(pts);
        }
    }
    if per_account.is_empty() {
        return Ok(Vec::new());
    }
    if !cumulative {
        let mut by_day: BTreeMap<String, f64> = BTreeMap::new();
        for p in per_account.iter().flatten() {
            if from.is_none_or(|f| p.day.as_str() >= f) && p.day.as_str() <= to {
                *by_day.entry(p.day.clone()).or_default() += p.value;
            }
        }
        return Ok(by_day
            .into_iter()
            .map(|(day, value)| MetricPoint {
                day,
                metric: metric.into(),
                value,
            })
            .collect());
    }
    let first = per_account
        .iter()
        .filter_map(|s| s.first())
        .map(|p| p.day.clone())
        .min()
        .unwrap_or_default();
    let parse = |s: &str| Date::parse(s, DAY).ok();
    let (Some(mut day), Some(end)) = (parse(&first), parse(to)) else {
        return Ok(Vec::new());
    };
    let start = from.and_then(parse);
    let mut cursors = vec![0usize; per_account.len()];
    let mut last: Vec<Option<f64>> = vec![None; per_account.len()];
    let mut out = Vec::new();
    while day <= end {
        let d = day.format(DAY).expect("format date");
        for (i, s) in per_account.iter().enumerate() {
            while cursors[i] < s.len() && s[cursors[i]].day <= d {
                last[i] = Some(s[cursors[i]].value);
                cursors[i] += 1;
            }
        }
        if start.is_none_or(|st| day >= st) && last.iter().any(Option::is_some) {
            out.push(MetricPoint {
                day: d,
                metric: metric.into(),
                value: last.iter().flatten().sum(),
            });
        }
        day += Duration::days(1);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopContent {
    pub account_id: String,
    pub account_name: String,
    pub platform: String,
    pub content: ContentRow,
}

/// Contents published in the period with the most views (latest snapshot), across
/// accounts.
pub fn top_contents(
    conn: &Connection,
    from: Option<&str>,
    to: &str,
    limit: usize,
) -> Result<Vec<TopContent>> {
    let mut all = Vec::new();
    for a in visible_accounts(conn)? {
        for c in contents::list_with_stats(conn, &a.id)? {
            let day = c.published_at.as_deref().map(|p| p.get(..10).unwrap_or(p));
            let in_range = day.is_some_and(|d| from.is_none_or(|f| d >= f) && d <= to);
            if in_range {
                all.push(TopContent {
                    account_id: a.id.clone(),
                    account_name: a.display_name.clone(),
                    platform: a.platform.clone(),
                    content: c,
                });
            }
        }
    }
    all.sort_by(|x, y| {
        y.content
            .views
            .unwrap_or(0.0)
            .total_cmp(&x.content.views.unwrap_or(0.0))
    });
    all.truncate(limit);
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::Content;
    use crate::db::{open_in_memory, repo::accounts::NewAccount};

    fn account(conn: &Connection, ext: &str) -> String {
        accounts::upsert(
            conn,
            &format!("acc-{ext}"),
            &NewAccount {
                platform: "mock",
                external_id: ext,
                handle: None,
                display_name: ext,
                granted_scopes: &[],
            },
            "t",
        )
        .unwrap()
        .id
    }

    fn put(conn: &Connection, acc: &str, day: &str, metric: &str, v: f64) {
        snapshots::upsert_account_snapshot(conn, acc, day, metric, v, "historic", "t").unwrap();
    }

    #[test]
    fn summary_sums_the_period_and_measures_follower_change() {
        let conn = open_in_memory().unwrap();
        let a = account(&conn, "a");
        for (day, views, dur, f) in [
            ("2026-11-01", 100.0, 60.0, 10.0),
            ("2026-11-02", 300.0, 20.0, 12.0),
            ("2026-11-03", 50.0, 0.0, 15.0),
        ] {
            put(&conn, &a, day, "views", views);
            put(&conn, &a, day, "avg_view_duration_s", dur);
            put(&conn, &a, day, "followers", f);
        }
        let s = summary(&conn, Some("2026-11-02"), "2026-11-02").unwrap();
        let acc = &s.accounts[0];
        assert_eq!(acc.views, 300.0);
        assert_eq!(acc.followers, Some(12.0));
        assert_eq!(acc.followers_change, Some(0.0));
        let all = summary(&conn, None, "2026-11-03").unwrap();
        let acc = &all.accounts[0];
        assert_eq!(acc.views, 450.0);
        assert_eq!(acc.followers_change, Some(5.0));
        // (100·60 + 300·20 + 50·0) / 450
        assert!((acc.avg_view_duration_s.unwrap() - 12_000.0 / 450.0).abs() < 1e-9);
        assert_eq!(all.totals.views, 450.0);
        assert_eq!(acc.data_until.as_deref(), Some("2026-11-03"));
    }

    #[test]
    fn combined_followers_carry_values_forward() {
        let conn = open_in_memory().unwrap();
        let a = account(&conn, "a");
        let b = account(&conn, "b");
        put(&conn, &a, "2026-11-01", "followers", 10.0);
        put(&conn, &a, "2026-11-03", "followers", 12.0);
        put(&conn, &b, "2026-11-02", "followers", 100.0);
        let s = combined_series(&conn, "followers", None, "2026-11-04").unwrap();
        let vals: Vec<(&str, f64)> = s.iter().map(|p| (p.day.as_str(), p.value)).collect();
        assert_eq!(
            vals,
            vec![
                ("2026-11-01", 10.0),
                ("2026-11-02", 110.0),
                ("2026-11-03", 112.0),
                ("2026-11-04", 112.0)
            ]
        );
        let since = combined_series(&conn, "followers", Some("2026-11-03"), "2026-11-04").unwrap();
        assert_eq!(since.len(), 2);
    }

    #[test]
    fn combined_daily_metrics_are_summed_per_day() {
        let conn = open_in_memory().unwrap();
        let a = account(&conn, "a");
        let b = account(&conn, "b");
        put(&conn, &a, "2026-11-01", "views", 1.0);
        put(&conn, &b, "2026-11-01", "views", 2.0);
        put(&conn, &b, "2026-11-02", "views", 5.0);
        let s = combined_series(&conn, "views", Some("2026-11-01"), "2026-11-02").unwrap();
        assert_eq!(
            s.iter().map(|p| p.value).collect::<Vec<_>>(),
            vec![3.0, 5.0]
        );
    }

    #[test]
    fn top_contents_are_filtered_by_period_and_sorted_by_views() {
        let conn = open_in_memory().unwrap();
        let a = account(&conn, "a");
        for (id, day, views) in [
            ("old", "2025-01-01", 9999.0),
            ("x", "2026-11-02", 10.0),
            ("y", "2026-11-03", 50.0),
        ] {
            let cid = contents::upsert(
                &conn,
                &a,
                &Content {
                    external_id: id.into(),
                    kind: "video".into(),
                    title: Some(id.into()),
                    description: None,
                    url: None,
                    thumbnail_url: None,
                    published_at: Some(format!("{day}T10:00:00Z")),
                    duration_s: None,
                    visibility: None,
                },
                "video",
                true,
                "t",
            )
            .unwrap();
            snapshots::upsert_content_snapshot(
                &conn,
                &cid,
                "2026-11-17",
                "views",
                views,
                "current",
                "t",
            )
            .unwrap();
        }
        let top = top_contents(&conn, Some("2026-11-01"), "2026-11-17", 5).unwrap();
        let ids: Vec<&str> = top.iter().map(|t| t.content.external_id.as_str()).collect();
        assert_eq!(ids, vec!["y", "x"]);
        assert_eq!(
            top_contents(&conn, None, "2026-11-17", 1).unwrap()[0]
                .content
                .external_id,
            "old"
        );
    }
}
