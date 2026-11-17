//! YouTube Analytics API: daily channel metrics, used to backfill history and fill gaps.
//! It has its own quota, separate from the Data API's 10,000 units, so it is not metered
//! in the ledger. Data lags 2–3 days behind.

use super::super::{AccountCtx, MetricPoint};
use crate::{AppError, Result};
use serde::Deserialize;
use time::{macros::format_description, Date, Duration};

/// Analytics metric → (Omnyra metric, multiplier).
const METRICS: &[(&str, &str, f64)] = &[
    ("views", "views", 1.0),
    ("estimatedMinutesWatched", "watch_time_s", 60.0),
    ("averageViewDuration", "avg_view_duration_s", 1.0),
    ("subscribersGained", "subscribers_gained", 1.0),
    ("subscribersLost", "subscribers_lost", 1.0),
    ("likes", "likes", 1.0),
    ("comments", "comments", 1.0),
    ("shares", "shares", 1.0),
];

/// Days per request (one year keeps every response small).
const CHUNK_DAYS: i64 = 365;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    #[serde(default)]
    column_headers: Vec<Header>,
    #[serde(default)]
    rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct Header {
    name: String,
}

fn parse_day(s: &str, field: &str) -> Result<Date> {
    Date::parse(s, format_description!("[year]-[month]-[day]")).map_err(|_| AppError::Validation {
        field: field.into(),
    })
}

fn fmt_day(d: Date) -> String {
    d.format(format_description!("[year]-[month]-[day]"))
        .expect("format date")
}

/// Splits `[from, to]` into consecutive chunks of at most [`CHUNK_DAYS`] days.
pub fn chunks(from: &str, to: &str) -> Result<Vec<(String, String)>> {
    let (mut start, end) = (parse_day(from, "from")?, parse_day(to, "to")?);
    let mut out = Vec::new();
    while start <= end {
        let stop = (start + Duration::days(CHUNK_DAYS - 1)).min(end);
        out.push((fmt_day(start), fmt_day(stop)));
        start = stop + Duration::days(1);
    }
    Ok(out)
}

pub async fn daily_metrics(
    ctx: &AccountCtx,
    base: &str,
    from: &str,
    to: &str,
) -> Result<Vec<MetricPoint>> {
    let metric_list = METRICS.iter().map(|m| m.0).collect::<Vec<_>>().join(",");
    let mut out = Vec::new();
    for (start, end) in chunks(from, to)? {
        let report: Report = ctx
            .api
            .get_json(
                &format!("{base}/v2/reports"),
                &[
                    ("ids", "channel==MINE"),
                    ("startDate", &start),
                    ("endDate", &end),
                    ("metrics", &metric_list),
                    ("dimensions", "day"),
                    ("sort", "day"),
                ],
            )
            .await?;
        out.extend(parse_report(report));
    }
    Ok(out)
}

/// Columns are matched by name, so their order in the response does not matter.
fn parse_report(report: Report) -> Vec<MetricPoint> {
    let names: Vec<&str> = report
        .column_headers
        .iter()
        .map(|h| h.name.as_str())
        .collect();
    let Some(day_col) = names.iter().position(|n| *n == "day") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in &report.rows {
        let Some(day) = row.get(day_col).and_then(|v| v.as_str()) else {
            continue;
        };
        for (col, name) in names.iter().enumerate() {
            let Some((_, ours, mult)) = METRICS.iter().find(|m| m.0 == *name) else {
                continue;
            };
            if let Some(v) = row.get(col).and_then(|v| v.as_f64()) {
                out.push(MetricPoint {
                    day: day.to_owned(),
                    metric: (*ours).to_owned(),
                    value: v * mult,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_long_ranges_into_years() {
        let c = chunks("2024-01-01", "2026-02-15").unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], ("2024-01-01".into(), "2024-12-30".into()));
        assert_eq!(c[1].0, "2024-12-31");
        assert_eq!(c[2].1, "2026-02-15");
        assert_eq!(chunks("2026-01-01", "2026-01-01").unwrap().len(), 1);
        assert!(chunks("2026-01-02", "2026-01-01").unwrap().is_empty());
    }

    #[test]
    fn parses_columns_by_name() {
        let r: Report = serde_json::from_str(
            r#"{"columnHeaders":[{"name":"day"},{"name":"estimatedMinutesWatched"},{"name":"views"}],
                "rows":[["2026-11-01",2,10],["2026-11-02",0.5,3]]}"#,
        )
        .unwrap();
        let pts = parse_report(r);
        assert_eq!(pts.len(), 4);
        assert!(pts.contains(&MetricPoint {
            day: "2026-11-01".into(),
            metric: "watch_time_s".into(),
            value: 120.0
        }));
        assert!(pts.contains(&MetricPoint {
            day: "2026-11-02".into(),
            metric: "views".into(),
            value: 3.0
        }));
    }
}
