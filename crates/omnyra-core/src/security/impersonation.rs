//! Impersonation suspects (6c, A9): accounts whose name imitates the creator's, among
//! comment authors (free, local) and, on demand, YouTube channel search results (100
//! quota units). Names only: the APIs give no reliable way to compare avatars without
//! downloading every image, so avatar similarity is not checked.

use super::phishing::name_similarity_max;
use crate::connectors::{AccountCtx, Connector};
use crate::{AppError, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;

pub const THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suspect {
    pub id: String,
    pub account_id: String,
    pub platform: String,
    pub suspect_external_id: String,
    pub suspect_name: Option<String>,
    pub name_score: f64,
    pub found_in: String,
    pub status: String,
    pub found_at: String,
}

fn creator_names(conn: &Connection, account_id: &str) -> Result<Vec<String>> {
    let (d, h): (String, Option<String>) = conn.query_row(
        "SELECT display_name, handle FROM accounts WHERE id = ?1",
        params![account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(std::iter::once(d).chain(h).collect())
}

#[allow(clippy::too_many_arguments)]
fn record(
    conn: &Connection,
    account_id: &str,
    platform: &str,
    ext: &str,
    name: &str,
    score: f64,
    found_in: &str,
    now: &str,
) -> Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO impersonation_hits (id, account_id, platform, suspect_external_id,
                                                   suspect_name, name_score, found_in, found_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uuid::Uuid::now_v7().to_string(),
            account_id,
            platform,
            ext,
            name,
            score,
            found_in,
            now
        ],
    )? > 0)
}

/// Scans the authors of the account's comments. Returns how many new suspects were found.
pub fn scan_comments(conn: &Connection, account_id: &str, now: &str) -> Result<u32> {
    let names = creator_names(conn, account_id)?;
    let (platform, own_ext): (String, String) = conn.query_row(
        "SELECT platform, external_id FROM accounts WHERE id = ?1",
        params![account_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let authors: Vec<(String, String)> = {
        let mut st = conn.prepare(
            "SELECT DISTINCT COALESCE(author_external_id, author_name), author_name FROM comments
             WHERE account_id = ?1 AND is_own = 0 AND author_name IS NOT NULL",
        )?;
        let r = st.query_map(params![account_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let mut n = 0;
    for (ext, name) in authors {
        if ext == own_ext {
            continue;
        }
        if let Some((score, exact)) = name_similarity_max(&name, &names) {
            // An exact name from someone else is the strongest sign of all.
            if (exact || score >= THRESHOLD)
                && record(
                    conn, account_id, &platform, &ext, &name, score, "comments", now,
                )?
            {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Searches the platform for channels named like the creator (YouTube: 100+ units).
pub async fn scan_search(
    db: &std::sync::Mutex<Connection>,
    connector: &dyn Connector,
    ctx: &AccountCtx,
    account_id: &str,
    now: &str,
) -> Result<u32> {
    let (names, platform) = {
        let c = db.lock().map_err(|_| AppError::Internal)?;
        let p: String = c.query_row(
            "SELECT platform FROM accounts WHERE id = ?1",
            params![account_id],
            |r| r.get(0),
        )?;
        (creator_names(&c, account_id)?, p)
    };
    let query = names.first().cloned().unwrap_or_default();
    let found = connector.search_channels(ctx, &query).await?;
    let c = db.lock().map_err(|_| AppError::Internal)?;
    let mut n = 0;
    for ch in found {
        if ch.external_id == ctx.external_id {
            continue;
        }
        let candidates = [Some(ch.display_name.clone()), ch.handle.clone()];
        let best = candidates
            .iter()
            .flatten()
            .filter_map(|nm| name_similarity_max(nm, &names).map(|s| (nm.clone(), s)))
            .max_by(|a, b| {
                a.1 .0
                    .partial_cmp(&b.1 .0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some((nm, (score, exact))) = best {
            if (exact || score >= THRESHOLD)
                && record(
                    &c,
                    account_id,
                    &platform,
                    &ch.external_id,
                    &nm,
                    score,
                    "search",
                    now,
                )?
            {
                n += 1;
            }
        }
    }
    Ok(n)
}

pub fn list(conn: &Connection) -> Result<Vec<Suspect>> {
    let mut st = conn.prepare(
        "SELECT id, account_id, platform, suspect_external_id, suspect_name, name_score, found_in,
                status, found_at
         FROM impersonation_hits ORDER BY status = 'new' DESC, name_score DESC LIMIT 200",
    )?;
    let r = st.query_map([], |r| {
        Ok(Suspect {
            id: r.get(0)?,
            account_id: r.get(1)?,
            platform: r.get(2)?,
            suspect_external_id: r.get(3)?,
            suspect_name: r.get(4)?,
            name_score: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
            found_in: r.get(6)?,
            status: r.get(7)?,
            found_at: r.get(8)?,
        })
    })?;
    Ok(r.collect::<std::result::Result<_, _>>()?)
}

pub fn set_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    if !["new", "dismissed", "reported"].contains(&status) {
        return Err(AppError::Validation {
            field: "status".into(),
        });
    }
    if conn.execute(
        "UPDATE impersonation_hits SET status = ?2 WHERE id = ?1",
        params![id, status],
    )? == 0
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Public profile URL of a suspect, for "report on the platform".
pub fn profile_url(platform: &str, ext: &str) -> Option<String> {
    match platform {
        "youtube" if ext.starts_with("UC") => {
            Some(format!("https://www.youtube.com/channel/{ext}"))
        }
        "instagram" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lookalike_authors_once() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, handle, connected_at)
               VALUES ('a', 'youtube', 'UCme', 'gh0stymc', '@gh0stymc', 't');
             INSERT INTO comments (id, account_id, external_id, text, author_name, author_external_id, fetched_at) VALUES
               ('1', 'a', 'e1', 'x', 'gh0styrnc', 'UCfake1', 't'),
               ('2', 'a', 'e2', 'x', 'Marta', 'UCm', 't'),
               ('3', 'a', 'e3', 'x', 'gh0stymc', 'UCfake2', 't'),
               ('4', 'a', 'e4', 'x', 'gh0stymc', 'UCme', 't');",
        )
        .unwrap();
        assert_eq!(scan_comments(&conn, "a", "t1").unwrap(), 2);
        assert_eq!(
            scan_comments(&conn, "a", "t2").unwrap(),
            0,
            "already recorded"
        );
        let l = list(&conn).unwrap();
        assert!(l
            .iter()
            .any(|s| s.suspect_external_id == "UCfake2" && s.name_score == 1.0));
        assert!(l.iter().all(|s| s.suspect_external_id != "UCme"));
        set_status(&conn, &l[0].id, "dismissed").unwrap();
        assert!(set_status(&conn, &l[0].id, "x").is_err());
        assert_eq!(
            profile_url("youtube", "UCfake1").as_deref(),
            Some("https://www.youtube.com/channel/UCfake1")
        );
    }
}
