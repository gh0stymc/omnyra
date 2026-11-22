//! Moments for shorts (step 5c): where in a long video or a stream the audience reacted,
//! proposed as 15–60 s fragments to cut in vertical.
//!
//! - **Retention (YouTube):** the audience-watch-ratio curve normally decays; bumps above
//!   its running median are parts people re-watched or skipped to. Score = residual.
//! - **Chat (Twitch):** messages per 10 s while Omnyra watched the stream (A3); robust
//!   z-score with median and MAD. Chat reacts after the moment, so windows start earlier.
//! - Nearby windows are merged; at most [`MAX_MOMENTS`], best first.

use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// `(x, y)` points of a series.
pub type Series = Vec<(f64, f64)>;

pub const MIN_LEN_S: f64 = 15.0;
pub const MAX_LEN_S: f64 = 60.0;
pub const MAX_MOMENTS: usize = 8;
pub const CHAT_BUCKET_S: i64 = 10;
/// Chat reacts this long after what happened on stream.
const CHAT_LAG_S: f64 = 15.0;
const CHAT_Z: f64 = 3.0;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    pub start_s: f64,
    pub end_s: f64,
    pub score: f64,
    /// `retention` | `chat`
    pub source: String,
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Clamps a window to `[MIN_LEN_S, MAX_LEN_S]` and to the content.
fn shape(mut start: f64, mut end: f64, duration: Option<f64>) -> (f64, f64) {
    if end - start < MIN_LEN_S {
        let pad = (MIN_LEN_S - (end - start)) / 2.0;
        start -= pad;
        end += pad;
    }
    if end - start > MAX_LEN_S {
        end = start + MAX_LEN_S;
    }
    if start < 0.0 {
        end -= start;
        start = 0.0;
    }
    if let Some(d) = duration {
        if end > d {
            start = (start - (end - d)).max(0.0);
            end = d;
        }
    }
    (start, end)
}

/// Merges overlapping or touching windows (keeping the best score) and keeps the best.
pub fn merge(mut ms: Vec<Moment>, duration: Option<f64>) -> Vec<Moment> {
    ms.sort_by(|a, b| {
        a.start_s
            .partial_cmp(&b.start_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<Moment> = Vec::new();
    for m in ms {
        if let Some(last) = out.last_mut() {
            if m.start_s <= last.end_s + 2.0 && m.end_s.max(last.end_s) - last.start_s <= MAX_LEN_S
            {
                last.end_s = last.end_s.max(m.end_s);
                if m.score > last.score {
                    last.score = m.score;
                    last.source = m.source;
                }
                continue;
            }
        }
        out.push(m);
    }
    for m in &mut out {
        let (s, e) = shape(m.start_s, m.end_s, duration);
        m.start_s = s;
        m.end_s = e;
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(MAX_MOMENTS);
    out
}

/// Bumps of a retention curve (`(ratio 0..1, watch ratio)`) of a `duration_s` video.
pub fn retention_peaks(points: &[(f64, f64)], duration_s: f64) -> Vec<Moment> {
    if points.len() < 10 || duration_s < MIN_LEN_S * 2.0 {
        return Vec::new();
    }
    let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
    let half = 4;
    let residual: Vec<f64> = (0..ys.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(ys.len());
            let mut w: Vec<f64> = ys[lo..hi].to_vec();
            ys[i] - median(&mut w)
        })
        .collect();
    let mut abs: Vec<f64> = residual.iter().map(|r| r.abs()).collect();
    let noise = median(&mut abs).max(0.005);
    let mut out = Vec::new();
    for i in 1..residual.len().saturating_sub(1) {
        let r = residual[i];
        // Skip the first 3 % (everyone is there) and keep clear local maxima.
        if points[i].0 < 0.03 || r < noise * 3.0 || r < residual[i - 1] || r < residual[i + 1] {
            continue;
        }
        let t = points[i].0 * duration_s;
        out.push(Moment {
            start_s: t - 10.0,
            end_s: t + 20.0,
            score: r / noise,
            source: "retention".into(),
        });
    }
    merge(out, Some(duration_s))
}

/// Peaks of chat activity: `(seconds from stream start, messages in the 10 s bucket)`.
pub fn chat_peaks(buckets: &[(f64, f64)], duration_s: Option<f64>) -> Vec<Moment> {
    if buckets.len() < 12 {
        return Vec::new();
    }
    let mut v: Vec<f64> = buckets.iter().map(|b| b.1).collect();
    let med = median(&mut v);
    let mut dev: Vec<f64> = buckets.iter().map(|b| (b.1 - med).abs()).collect();
    let mad = median(&mut dev).max(0.5) * 1.4826;
    let mut out = Vec::new();
    for &(t, n) in buckets {
        let z = (n - med) / mad;
        if z >= CHAT_Z {
            out.push(Moment {
                start_s: t - CHAT_LAG_S - 10.0,
                end_s: t + CHAT_BUCKET_S as f64,
                score: z,
                source: "chat".into(),
            });
        }
    }
    merge(out, duration_s)
}

// ---- storage -------------------------------------------------------------------------

pub fn save_series(
    conn: &Connection,
    content_id: &str,
    kind: &str,
    points: &[(f64, f64)],
    now: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO content_series (content_id, kind, data, fetched_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (content_id, kind) DO UPDATE SET data = excluded.data,
                                                       fetched_at = excluded.fetched_at",
        params![
            content_id,
            kind,
            serde_json::to_string(points).map_err(|_| AppError::Internal)?,
            now
        ],
    )?;
    Ok(())
}

pub fn load_series(
    conn: &Connection,
    content_id: &str,
    kind: &str,
) -> Result<Option<(Series, String)>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT data, fetched_at FROM content_series WHERE content_id = ?1 AND kind = ?2",
            params![content_id, kind],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(d, f)| serde_json::from_str(&d).ok().map(|p| (p, f))))
}

/// Start of the 10 s bucket `ts` falls in.
pub fn bucket_of(ts: OffsetDateTime) -> String {
    let secs = ts.unix_timestamp();
    let b = secs - secs.rem_euclid(CHAT_BUCKET_S);
    OffsetDateTime::from_unix_timestamp(b)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_default()
}

pub fn add_chat(conn: &Connection, session_id: &str, bucket: &str, msgs: u32) -> Result<()> {
    conn.execute(
        "INSERT INTO live_chat (session_id, bucket, msgs) VALUES (?1, ?2, ?3)
         ON CONFLICT (session_id, bucket) DO UPDATE SET msgs = msgs + excluded.msgs",
        params![session_id, bucket, msgs],
    )?;
    Ok(())
}

/// Open session of an account (being streamed right now), if any.
pub fn open_session(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM live_sessions WHERE account_id = ?1 AND ended_at IS NULL
             ORDER BY started_at DESC LIMIT 1",
            params![account_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Chat buckets of a session, as seconds from its start (missing buckets are 0).
pub fn chat_series(conn: &Connection, session_id: &str) -> Result<Vec<(f64, f64)>> {
    let started: String = conn.query_row(
        "SELECT started_at FROM live_sessions WHERE id = ?1",
        params![session_id],
        |r| r.get(0),
    )?;
    let start = OffsetDateTime::parse(&started, &Rfc3339).map_err(|_| AppError::Internal)?;
    let mut st =
        conn.prepare("SELECT bucket, msgs FROM live_chat WHERE session_id = ?1 ORDER BY bucket")?;
    let rows: Vec<(String, i64)> = st
        .query_map(params![session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (b, n) in rows {
        let Ok(t) = OffsetDateTime::parse(&b, &Rfc3339) else {
            continue;
        };
        let off = (t - start).whole_seconds() as f64;
        if off < 0.0 {
            continue;
        }
        // Fill gaps with zeros so the baseline is honest.
        if let Some(&(last, _)) = out.last() {
            let mut x = last + CHAT_BUCKET_S as f64;
            while x < off {
                out.push((x, 0.0));
                x += CHAT_BUCKET_S as f64;
            }
        }
        out.push((off, n as f64));
    }
    Ok(out)
}

// ---- Twitch chat (anonymous IRC, read-only) ------------------------------------------

/// True for a chat message line (`... PRIVMSG #channel :text`).
pub fn is_privmsg(line: &str) -> bool {
    let rest = line
        .strip_prefix('@')
        .map(|l| l.split_once(' ').map_or("", |x| x.1))
        .unwrap_or(line);
    rest.split(' ').nth(1) == Some("PRIVMSG")
}

/// Reads the public chat of `login` anonymously (`justinfan` nick, no token) and calls
/// `on_msg` for every message. Returns when the connection drops. Plain-text IRC: the chat
/// is public and no credentials are sent.
pub async fn read_chat(login: &str, mut on_msg: impl FnMut()) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let channel: String = login
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if channel.is_empty() {
        return Err(AppError::Validation {
            field: "login".into(),
        });
    }
    let stream = tokio::net::TcpStream::connect("irc.chat.twitch.tv:6667")
        .await
        .map_err(|_| AppError::Network)?;
    let (r, mut w) = stream.into_split();
    let nick = format!("justinfan{}", 10_000 + rand::random::<u32>() % 89_999);
    w.write_all(format!("NICK {nick}\r\nJOIN #{channel}\r\n").as_bytes())
        .await
        .map_err(|_| AppError::Network)?;
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await.map_err(|_| AppError::Network)? {
        if let Some(rest) = line.strip_prefix("PING") {
            w.write_all(format!("PONG{rest}\r\n").as_bytes())
                .await
                .map_err(|_| AppError::Network)?;
        } else if is_privmsg(&line) {
            on_msg();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_bump_becomes_a_moment() {
        // Smooth decay with a re-watch bump at 40 % of a 10-minute video.
        let pts: Vec<(f64, f64)> = (1..=100)
            .map(|i| {
                let x = i as f64 / 100.0;
                let bump = if (39..=41).contains(&i) { 0.15 } else { 0.0 };
                (x, 0.9 - 0.5 * x + bump)
            })
            .collect();
        let m = retention_peaks(&pts, 600.0);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].start_s <= 240.0 && m[0].end_s >= 240.0, "{m:?}");
        let len = m[0].end_s - m[0].start_s;
        assert!((MIN_LEN_S..=MAX_LEN_S).contains(&len));
        // A flat decay has no moments; too little data neither.
        let flat: Vec<(f64, f64)> = (1..=100)
            .map(|i| (i as f64 / 100.0, 1.0 - i as f64 / 200.0))
            .collect();
        assert!(retention_peaks(&flat, 600.0).is_empty());
        assert!(retention_peaks(&pts[..5], 600.0).is_empty());
    }

    #[test]
    fn chat_spike_is_found_and_shifted_back() {
        let mut b: Vec<(f64, f64)> = (0..60)
            .map(|i| (i as f64 * 10.0, 3.0 + (i % 3) as f64))
            .collect();
        b[30].1 = 40.0; // at 300 s
        b[31].1 = 35.0;
        let m = chat_peaks(&b, Some(600.0));
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(
            m[0].start_s < 300.0 - 10.0,
            "starts before the reaction: {m:?}"
        );
        assert!(m[0].end_s >= 310.0);
        assert_eq!(m[0].source, "chat");
    }

    #[test]
    fn windows_are_shaped_and_merged() {
        assert_eq!(shape(-5.0, 5.0, None), (0.0, 15.0));
        assert_eq!(shape(0.0, 100.0, None), (0.0, 60.0));
        assert_eq!(shape(590.0, 610.0, Some(600.0)), (580.0, 600.0));
        let ms = vec![
            Moment {
                start_s: 10.0,
                end_s: 30.0,
                score: 1.0,
                source: "chat".into(),
            },
            Moment {
                start_s: 25.0,
                end_s: 45.0,
                score: 5.0,
                source: "retention".into(),
            },
            Moment {
                start_s: 200.0,
                end_s: 220.0,
                score: 2.0,
                source: "chat".into(),
            },
        ];
        let out = merge(ms, None);
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].start_s, out[0].end_s, out[0].source.as_str()),
            (10.0, 45.0, "retention")
        );
    }

    #[test]
    fn chat_storage_buckets_and_series() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, connected_at)
               VALUES ('a', 'twitch', 'x', 'X', 't');
             INSERT INTO live_sessions (id, account_id, external_id, started_at)
               VALUES ('s', 'a', 'st', '2026-11-22T20:00:00Z');",
        )
        .unwrap();
        assert_eq!(open_session(&conn, "a").unwrap().as_deref(), Some("s"));
        let t = OffsetDateTime::parse("2026-11-22T20:00:27Z", &Rfc3339).unwrap();
        assert_eq!(bucket_of(t), "2026-11-22T20:00:20Z");
        add_chat(&conn, "s", "2026-11-22T20:00:00Z", 2).unwrap();
        add_chat(&conn, "s", "2026-11-22T20:00:00Z", 3).unwrap();
        add_chat(&conn, "s", "2026-11-22T20:00:30Z", 1).unwrap();
        assert_eq!(
            chat_series(&conn, "s").unwrap(),
            vec![(0.0, 5.0), (10.0, 0.0), (20.0, 0.0), (30.0, 1.0)]
        );
    }

    #[test]
    fn recognises_chat_lines() {
        assert!(is_privmsg(":a!a@a.tmi.twitch.tv PRIVMSG #gh0sty :hola"));
        assert!(is_privmsg(
            "@badge-info=;color= :a!a@a.tmi.twitch.tv PRIVMSG #x :gg"
        ));
        assert!(!is_privmsg(":tmi.twitch.tv 001 justinfan1 :Welcome"));
        assert!(!is_privmsg("PING :tmi.twitch.tv"));
    }

    #[test]
    fn series_roundtrip() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, connected_at)
               VALUES ('a', 'youtube', 'x', 'X', 't');
             INSERT INTO contents (id, account_id, external_id, kind, first_seen_at, last_fetched_at)
               VALUES ('c', 'a', 'v', 'video', 't', 't');",
        )
        .unwrap();
        save_series(&conn, "c", "retention", &[(0.01, 1.0)], "t1").unwrap();
        save_series(&conn, "c", "retention", &[(0.01, 0.9)], "t2").unwrap();
        let (p, at) = load_series(&conn, "c", "retention").unwrap().unwrap();
        assert_eq!((p, at.as_str()), (vec![(0.01, 0.9)], "t2"));
    }
}
