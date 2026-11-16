use crate::connectors::MetricPoint;
use crate::Result;
use rusqlite::{params, Connection};

/// Upsert: the last snapshot of the day wins.
pub fn upsert_account_snapshot(
    conn: &Connection,
    account_id: &str,
    day: &str,
    metric: &str,
    value: f64,
    source: &str,
    captured_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO account_snapshots (account_id, day, metric, value, source, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account_id, metric, day) DO UPDATE SET
           value = excluded.value, source = excluded.source, captured_at = excluded.captured_at",
        params![account_id, day, metric, value, source, captured_at],
    )?;
    Ok(())
}

pub fn account_series(
    conn: &Connection,
    account_id: &str,
    metric: &str,
) -> Result<Vec<MetricPoint>> {
    let mut stmt = conn.prepare(
        "SELECT day, metric, value FROM account_snapshots
         WHERE account_id = ?1 AND metric = ?2 ORDER BY day",
    )?;
    let rows = stmt.query_map(params![account_id, metric], |r| {
        Ok(MetricPoint {
            day: r.get(0)?,
            metric: r.get(1)?,
            value: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, repo::accounts};

    #[test]
    fn last_snapshot_of_the_day_wins() {
        let conn = open_in_memory().unwrap();
        let acc = accounts::upsert(
            &conn,
            &accounts::NewAccount {
                platform: "mock",
                external_id: "1",
                handle: None,
                display_name: "M",
            },
            "2026-11-16T00:00:00Z",
        )
        .unwrap();
        upsert_account_snapshot(
            &conn,
            &acc.id,
            "2026-11-16",
            "followers",
            10.0,
            "current",
            "t1",
        )
        .unwrap();
        upsert_account_snapshot(
            &conn,
            &acc.id,
            "2026-11-16",
            "followers",
            12.0,
            "current",
            "t2",
        )
        .unwrap();
        let s = account_series(&conn, &acc.id, "followers").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].value, 12.0);
    }
}
