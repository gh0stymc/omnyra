//! Live streams observed by Omnyra (A2). Helix only reports the viewer count "right now",
//! so the app samples it every couple of minutes while it is running; average and peak
//! viewers exist only for streams that were sampled. Hours streamed and the number of
//! streams also count the past broadcasts Twitch keeps as VODs (archives).

use crate::connectors::LiveStatus;
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// A sample older than this means Omnyra stopped watching: the stream is not "live now".
const LIVE_STALE_S: i64 = 10 * 60;

/// Records what the platform says right now: a sample of the live stream, or the end of
/// the open sessions when the account is offline.
pub fn record(
    conn: &mut Connection,
    account_id: &str,
    status: Option<&LiveStatus>,
    now: &str,
) -> Result<()> {
    let tx = conn.transaction()?;
    let current = match status {
        Some(s) => {
            let id = match tx
                .query_row(
                    "SELECT id FROM live_sessions WHERE account_id = ?1 AND external_id = ?2",
                    params![account_id, s.stream_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::now_v7().to_string();
                    tx.execute(
                        "INSERT INTO live_sessions (id, account_id, external_id, started_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![id, account_id, s.stream_id, s.started_at],
                    )?;
                    id
                }
            };
            tx.execute(
                "INSERT OR IGNORE INTO live_samples (session_id, ts, viewers) VALUES (?1, ?2, ?3)",
                params![id, now, s.viewers],
            )?;
            tx.execute(
                "UPDATE live_sessions SET title = ?2, category = ?3, ended_at = NULL,
                   peak_viewers = (SELECT MAX(viewers) FROM live_samples WHERE session_id = ?1),
                   avg_viewers = (SELECT AVG(viewers) FROM live_samples WHERE session_id = ?1)
                 WHERE id = ?1",
                params![id, s.title, s.category],
            )?;
            Some(id)
        }
        None => None,
    };
    // Any other open session has ended: at its last sample.
    tx.execute(
        "UPDATE live_sessions SET ended_at = COALESCE(
             (SELECT MAX(ts) FROM live_samples WHERE session_id = live_sessions.id), ?3)
         WHERE account_id = ?1 AND ended_at IS NULL AND id IS NOT ?2",
        params![account_id, current, now],
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub id: String,
    pub title: Option<String>,
    pub category: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    /// Up to the last sample (or now, if still live).
    pub duration_s: f64,
    pub peak_viewers: Option<u32>,
    pub avg_viewers: Option<f64>,
    pub samples: u32,
}

fn ts(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

/// Observed sessions that started in the period, newest first.
pub fn sessions(
    conn: &Connection,
    account_id: &str,
    from: Option<&str>,
    to: &str,
    now: OffsetDateTime,
) -> Result<Vec<LiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.category, s.started_at, s.ended_at, s.peak_viewers,
                s.avg_viewers, (SELECT COUNT(*) FROM live_samples WHERE session_id = s.id),
                (SELECT MAX(ts) FROM live_samples WHERE session_id = s.id)
         FROM live_sessions s
         WHERE s.account_id = ?1 AND substr(s.started_at, 1, 10) >= COALESCE(?2, '')
           AND substr(s.started_at, 1, 10) <= ?3
         ORDER BY s.started_at DESC",
    )?;
    let rows = stmt.query_map(params![account_id, from, to], |r| {
        let started: String = r.get(3)?;
        let ended: Option<String> = r.get(4)?;
        let last: Option<String> = r.get(8)?;
        let end = ended
            .as_deref()
            .or(last.as_deref())
            .and_then(ts)
            .unwrap_or(now);
        let end = if ended.is_none() { now.max(end) } else { end };
        let duration = ts(&started).map_or(0.0, |s| (end - s).as_seconds_f64().max(0.0));
        Ok(LiveSession {
            id: r.get(0)?,
            title: r.get(1)?,
            category: r.get(2)?,
            started_at: started,
            ended_at: ended,
            duration_s: duration,
            peak_viewers: r.get(5)?,
            avg_viewers: r.get(6)?,
            samples: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveStats {
    /// Streams in the period: archived VODs plus observed sessions without a VOD.
    pub streams: u32,
    pub hours_streamed: f64,
    /// How many of those streams Omnyra sampled (the viewer figures come only from them).
    pub observed: u32,
    /// Time-weighted average of the observed sessions.
    pub avg_viewers: Option<f64>,
    pub peak_viewers: Option<u32>,
    pub live_now: bool,
}

pub fn stats(
    conn: &Connection,
    account_id: &str,
    from: Option<&str>,
    to: &str,
    now: OffsetDateTime,
) -> Result<LiveStats> {
    let mut stmt = conn.prepare(
        "SELECT json_extract(extra, '$.stream_id'), COALESCE(duration_s, 0) FROM contents
         WHERE account_id = ?1 AND kind = 'vod'
           AND json_extract(extra, '$.twitch_type') = 'archive'
           AND substr(published_at, 1, 10) >= COALESCE(?2, '')
           AND substr(published_at, 1, 10) <= ?3",
    )?;
    let vods: Vec<(Option<String>, f64)> = stmt
        .query_map(params![account_id, from, to], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let vod_streams: HashSet<String> = vods.iter().filter_map(|(s, _)| s.clone()).collect();

    let observed = sessions(conn, account_id, from, to, now)?;
    let external: Vec<Option<String>> = {
        let mut st = conn.prepare("SELECT external_id FROM live_sessions WHERE id = ?1")?;
        observed
            .iter()
            .map(|s| st.query_row(params![s.id], |r| r.get(0)))
            .collect::<std::result::Result<_, _>>()?
    };
    let mut streams = vods.len() as u32;
    let mut seconds: f64 = vods.iter().map(|(_, d)| d).sum();
    for (s, ext) in observed.iter().zip(&external) {
        if !ext.as_ref().is_some_and(|e| vod_streams.contains(e)) {
            streams += 1;
            seconds += s.duration_s;
        }
    }
    let sampled: Vec<&LiveSession> = observed
        .iter()
        .filter(|s| s.avg_viewers.is_some())
        .collect();
    let weight: f64 = sampled.iter().map(|s| s.duration_s.max(1.0)).sum();
    let avg = (!sampled.is_empty()).then(|| {
        sampled
            .iter()
            .map(|s| s.avg_viewers.unwrap_or(0.0) * s.duration_s.max(1.0))
            .sum::<f64>()
            / weight
    });
    let live_now: bool = conn
        .query_row(
            "SELECT MAX(p.ts) FROM live_sessions s JOIN live_samples p ON p.session_id = s.id
             WHERE s.account_id = ?1 AND s.ended_at IS NULL",
            params![account_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .as_deref()
        .and_then(ts)
        .is_some_and(|last| (now - last).whole_seconds() <= LIVE_STALE_S);
    Ok(LiveStats {
        streams,
        hours_streamed: seconds / 3600.0,
        observed: sampled.len() as u32,
        avg_viewers: avg,
        peak_viewers: sampled.iter().filter_map(|s| s.peak_viewers).max(),
        live_now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::Content;
    use crate::db::{open_in_memory, repo::accounts, repo::contents};
    use time::macros::datetime;

    fn setup() -> (Connection, String) {
        let conn = open_in_memory().unwrap();
        let id = accounts::upsert(
            &conn,
            "acc",
            &accounts::NewAccount {
                platform: "twitch",
                external_id: "1234",
                handle: None,
                display_name: "T",
                granted_scopes: &[],
            },
            "t",
        )
        .unwrap()
        .id;
        (conn, id)
    }

    fn live(stream: &str, viewers: u32) -> LiveStatus {
        LiveStatus {
            stream_id: stream.into(),
            started_at: "2026-11-17T18:00:00Z".into(),
            viewers,
            title: Some("Directo".into()),
            category: Some("Minecraft".into()),
        }
    }

    #[test]
    fn samples_build_a_session_and_offline_closes_it() {
        let (mut conn, acc) = setup();
        record(
            &mut conn,
            &acc,
            Some(&live("s1", 10)),
            "2026-11-17T18:02:00Z",
        )
        .unwrap();
        record(
            &mut conn,
            &acc,
            Some(&live("s1", 30)),
            "2026-11-17T18:04:00Z",
        )
        .unwrap();
        // Same timestamp twice: ignored.
        record(
            &mut conn,
            &acc,
            Some(&live("s1", 99)),
            "2026-11-17T18:04:00Z",
        )
        .unwrap();
        let now = datetime!(2026-11-17 18:05 UTC);
        let st = stats(&conn, &acc, None, "2026-11-17", now).unwrap();
        assert!(st.live_now);
        assert_eq!(st.peak_viewers, Some(30));
        assert_eq!(st.avg_viewers, Some(20.0));

        record(&mut conn, &acc, None, "2026-11-17T18:06:00Z").unwrap();
        let s = &sessions(&conn, &acc, None, "2026-11-17", now).unwrap()[0];
        assert_eq!(s.ended_at.as_deref(), Some("2026-11-17T18:04:00Z"));
        assert_eq!(s.duration_s, 240.0);
        assert_eq!(s.samples, 2);
        assert!(
            !stats(&conn, &acc, None, "2026-11-17", now)
                .unwrap()
                .live_now
        );
    }

    #[test]
    fn a_new_stream_closes_the_previous_one() {
        let (mut conn, acc) = setup();
        record(
            &mut conn,
            &acc,
            Some(&live("s1", 5)),
            "2026-11-17T18:02:00Z",
        )
        .unwrap();
        record(
            &mut conn,
            &acc,
            Some(&live("s2", 5)),
            "2026-11-17T20:00:00Z",
        )
        .unwrap();
        let all = sessions(
            &conn,
            &acc,
            None,
            "2026-11-17",
            datetime!(2026-11-17 20:01 UTC),
        )
        .unwrap();
        let open: Vec<_> = all.iter().filter(|s| s.ended_at.is_none()).collect();
        assert_eq!(open.len(), 1);
    }

    #[test]
    fn hours_count_vods_and_unrecorded_sessions_once() {
        let (mut conn, acc) = setup();
        for (id, stream, dur) in [("v1", "s1", 3600u32), ("v2", "s2", 1800)] {
            let mut extra = serde_json::Map::new();
            extra.insert("stream_id".into(), stream.into());
            extra.insert("twitch_type".into(), "archive".into());
            contents::upsert(
                &conn,
                &acc,
                &Content {
                    external_id: id.into(),
                    kind: "vod".into(),
                    title: None,
                    description: None,
                    url: None,
                    thumbnail_url: None,
                    published_at: Some("2026-11-10T18:00:00Z".into()),
                    duration_s: Some(dur),
                    visibility: None,
                    extra,
                },
                "vod",
                true,
                "t",
            )
            .unwrap();
        }
        // s1 was also observed live (counted once, via its VOD); s3 has no VOD.
        let mut s1 = live("s1", 8);
        s1.started_at = "2026-11-10T18:00:00Z".into();
        record(&mut conn, &acc, Some(&s1), "2026-11-10T18:30:00Z").unwrap();
        let mut s3 = live("s3", 4);
        s3.started_at = "2026-11-12T18:00:00Z".into();
        record(&mut conn, &acc, Some(&s3), "2026-11-12T19:00:00Z").unwrap();
        record(&mut conn, &acc, None, "2026-11-12T19:05:00Z").unwrap();
        let st = stats(
            &conn,
            &acc,
            Some("2026-11-01"),
            "2026-11-17",
            datetime!(2026-11-17 12:00 UTC),
        )
        .unwrap();
        assert_eq!(st.streams, 3);
        assert!((st.hours_streamed - (1.0 + 0.5 + 1.0)).abs() < 1e-9);
        assert_eq!(st.observed, 2);
        assert_eq!(st.peak_viewers, Some(8));
    }
}
