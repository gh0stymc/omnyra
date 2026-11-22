//! Append-only log of sensitive actions (account connections, publications...).
//! Each row stores `hash = sha256(prev_hash | ts | action | account_id | detail)`, so editing
//! or deleting rows outside the app (the table has triggers against it) breaks the chain.
//! `detail` must never contain secrets.

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

fn digest(prev: &str, ts: &str, action: &str, account_id: Option<&str>, detail: &str) -> String {
    let mut h = Sha256::new();
    for part in [prev, ts, action, account_id.unwrap_or(""), detail] {
        h.update((part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn append(
    conn: &Connection,
    ts: &str,
    action: &str,
    account_id: Option<&str>,
    detail: &serde_json::Value,
) -> Result<()> {
    let detail = detail.to_string();
    let prev: String = conn
        .query_row(
            "SELECT hash FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
        .unwrap_or_default();
    let hash = digest(&prev, ts, action, account_id, &detail);
    conn.execute(
        "INSERT INTO audit_log (ts, action, account_id, detail, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![ts, action, account_id, detail, prev, hash],
    )?;
    Ok(())
}

/// True if every row's hash matches its content and links to the previous row.
pub fn verify_chain(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT ts, action, account_id, detail, prev_hash, hash FROM audit_log ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut prev = String::new();
    while let Some(r) = rows.next()? {
        let (ts, action, account, detail): (String, String, Option<String>, String) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        let (row_prev, hash): (Option<String>, Option<String>) = (r.get(4)?, r.get(5)?);
        if row_prev.unwrap_or_default() != prev
            || hash.as_deref() != Some(&digest(&prev, &ts, &action, account.as_deref(), &detail))
        {
            return Ok(false);
        }
        prev = hash.unwrap_or_default();
    }
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: f64,
    pub ts: String,
    pub action: String,
    pub account_id: Option<String>,
    pub detail: String,
}

/// Last `limit` entries, newest first (security screen).
pub fn recent(conn: &Connection, limit: u32) -> Result<Vec<AuditEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, action, account_id, detail FROM audit_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(AuditEntry {
            id: r.get::<_, i64>(0)? as f64,
            ts: r.get(1)?,
            action: r.get(2)?,
            account_id: r.get(3)?,
            detail: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Last entries, newest first (for tests and, later, the security screen).
pub fn recent_actions(conn: &Connection, limit: u32) -> Result<Vec<(String, Option<String>)>> {
    let mut stmt =
        conn.prepare("SELECT action, account_id FROM audit_log ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use serde_json::json;

    #[test]
    fn chain_verifies_and_detects_tampering() {
        let conn = open_in_memory().unwrap();
        append(
            &conn,
            "t1",
            "account.connect",
            Some("a"),
            &json!({"platform": "youtube"}),
        )
        .unwrap();
        append(&conn, "t2", "account.disconnect", Some("a"), &json!({})).unwrap();
        assert!(verify_chain(&conn).unwrap());

        // Simulate an edit made outside the app (bypassing the trigger).
        conn.execute_batch(
            "DROP TRIGGER audit_log_no_update;
             UPDATE audit_log SET detail = '{\"platform\":\"evil\"}' WHERE id = 1;",
        )
        .unwrap();
        assert!(!verify_chain(&conn).unwrap());
    }

    #[test]
    fn recent_actions_newest_first() {
        let conn = open_in_memory().unwrap();
        append(&conn, "t1", "a1", None, &json!({})).unwrap();
        append(&conn, "t2", "a2", Some("x"), &json!({})).unwrap();
        let r = recent_actions(&conn, 10).unwrap();
        assert_eq!(r[0], ("a2".to_string(), Some("x".to_string())));
        assert_eq!(r.len(), 2);
    }
}
