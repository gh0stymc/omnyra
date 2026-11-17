use crate::connectors::Content;
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;

/// What the sync needs to know about a stored content.
#[derive(Debug, Clone, PartialEq)]
pub struct Known {
    pub id: String,
    pub kind: String,
    /// The kind was settled (by the API or by the connector's `classify`).
    pub kind_checked: bool,
}

/// Stored contents of an account, by external ID.
pub fn known(conn: &Connection, account_id: &str) -> Result<HashMap<String, Known>> {
    let mut stmt = conn.prepare(
        "SELECT external_id, id, kind, COALESCE(json_extract(extra, '$.kind_checked'), 0)
         FROM contents WHERE account_id = ?1",
    )?;
    let rows = stmt.query_map(params![account_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Known {
                id: r.get(1)?,
                kind: r.get(2)?,
                kind_checked: r.get::<_, i64>(3)? != 0,
            },
        ))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Inserts or refreshes a content and returns its ID. `kind` and `kind_checked` are decided
/// by the caller (a settled kind must not be overwritten by a provisional one).
pub fn upsert(
    conn: &Connection,
    account_id: &str,
    c: &Content,
    kind: &str,
    kind_checked: bool,
    now_iso: &str,
) -> Result<String> {
    let new_id = uuid::Uuid::now_v7().to_string();
    let extra = serde_json::json!({
        "kind_checked": kind_checked,
        "thumbnail_url": c.thumbnail_url,
    })
    .to_string();
    conn.execute(
        "INSERT INTO contents (id, account_id, external_id, kind, title, description, url,
                               published_at, duration_s, visibility, extra,
                               first_seen_at, last_fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
         ON CONFLICT (account_id, external_id) DO UPDATE SET
           kind = excluded.kind, title = excluded.title, description = excluded.description,
           url = excluded.url, published_at = excluded.published_at,
           duration_s = excluded.duration_s, visibility = excluded.visibility,
           extra = json_patch(contents.extra, excluded.extra),
           last_fetched_at = excluded.last_fetched_at",
        params![
            new_id,
            account_id,
            c.external_id,
            kind,
            c.title,
            c.description,
            c.url,
            c.published_at,
            c.duration_s,
            c.visibility,
            extra,
            now_iso
        ],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM contents WHERE account_id = ?1 AND external_id = ?2",
        params![account_id, c.external_id],
        |r| r.get(0),
    )?)
}

/// Stores a settled kind.
pub fn set_kind(conn: &Connection, account_id: &str, external_id: &str, kind: &str) -> Result<()> {
    conn.execute(
        "UPDATE contents SET kind = ?3, extra = json_set(extra, '$.kind_checked', json('true'))
         WHERE account_id = ?1 AND external_id = ?2",
        params![account_id, external_id, kind],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContentRow {
    pub id: String,
    pub external_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub published_at: Option<String>,
    pub duration_s: Option<u32>,
    pub visibility: Option<String>,
    /// Latest snapshot values.
    pub views: Option<f64>,
    pub likes: Option<f64>,
    pub comments: Option<f64>,
}

fn latest(conn: &Connection, content_id: &str, metric: &str) -> Result<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT value FROM content_snapshots WHERE content_id = ?1 AND metric = ?2
             ORDER BY day DESC LIMIT 1",
            params![content_id, metric],
            |r| r.get(0),
        )
        .optional()?)
}

/// Contents of an account, newest first, with their latest stats.
pub fn list_with_stats(conn: &Connection, account_id: &str) -> Result<Vec<ContentRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, external_id, kind, title, url, json_extract(extra, '$.thumbnail_url'),
                published_at, duration_s, visibility
         FROM contents WHERE account_id = ?1 ORDER BY published_at DESC",
    )?;
    let rows = stmt
        .query_map(params![account_id], |r| {
            Ok(ContentRow {
                id: r.get(0)?,
                external_id: r.get(1)?,
                kind: r.get(2)?,
                title: r.get(3)?,
                url: r.get(4)?,
                thumbnail_url: r.get(5)?,
                published_at: r.get(6)?,
                duration_s: r.get(7)?,
                visibility: r.get(8)?,
                views: None,
                likes: None,
                comments: None,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|mut row| {
            row.views = latest(conn, &row.id, "views")?;
            row.likes = latest(conn, &row.id, "likes")?;
            row.comments = latest(conn, &row.id, "comments")?;
            Ok(row)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, repo::accounts};

    fn content(id: &str) -> Content {
        Content {
            external_id: id.into(),
            kind: "video".into(),
            title: Some("T".into()),
            description: None,
            url: None,
            thumbnail_url: Some("thumb".into()),
            published_at: Some("2026-11-01T00:00:00Z".into()),
            duration_s: Some(30),
            visibility: None,
        }
    }

    #[test]
    fn upsert_keeps_id_and_settled_kind() {
        let conn = open_in_memory().unwrap();
        let acc = accounts::upsert(
            &conn,
            "acc",
            &accounts::NewAccount {
                platform: "mock",
                external_id: "1",
                handle: None,
                display_name: "M",
                granted_scopes: &[],
            },
            "t0",
        )
        .unwrap();
        let id1 = upsert(&conn, &acc.id, &content("v1"), "video", false, "t1").unwrap();
        set_kind(&conn, &acc.id, "v1", "short").unwrap();
        let k = known(&conn, &acc.id).unwrap();
        assert_eq!(
            k["v1"],
            Known {
                id: id1.clone(),
                kind: "short".into(),
                kind_checked: true
            }
        );
        // A later refresh passes the settled kind back in.
        let id2 = upsert(&conn, &acc.id, &content("v1"), "short", true, "t2").unwrap();
        assert_eq!(id1, id2);
        let rows = list_with_stats(&conn, &acc.id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "short");
        assert_eq!(rows[0].thumbnail_url.as_deref(), Some("thumb"));
    }
}
