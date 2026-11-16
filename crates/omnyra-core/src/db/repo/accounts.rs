use crate::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub external_id: String,
    pub handle: Option<String>,
    pub display_name: String,
    pub status: String,
    pub connected_at: String,
    pub last_sync_at: Option<String>,
}

pub struct NewAccount<'a> {
    pub platform: &'a str,
    pub external_id: &'a str,
    pub handle: Option<&'a str>,
    pub display_name: &'a str,
}

/// Inserts the account, or returns the existing one for the same (platform, external_id).
pub fn upsert(conn: &Connection, new: &NewAccount, now_iso: &str) -> Result<Account> {
    let id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO accounts (id, platform, external_id, handle, display_name, connected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (platform, external_id) DO UPDATE SET
           handle = excluded.handle, display_name = excluded.display_name",
        params![
            id,
            new.platform,
            new.external_id,
            new.handle,
            new.display_name,
            now_iso
        ],
    )?;
    get_by_external(conn, new.platform, new.external_id)
}

pub fn get_by_external(conn: &Connection, platform: &str, external_id: &str) -> Result<Account> {
    Ok(conn.query_row(
        "SELECT id, platform, external_id, handle, display_name, status, connected_at, last_sync_at
         FROM accounts WHERE platform = ?1 AND external_id = ?2",
        params![platform, external_id],
        row_to_account,
    )?)
}

pub fn list(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, platform, external_id, handle, display_name, status, connected_at, last_sync_at
         FROM accounts ORDER BY platform, display_name",
    )?;
    let rows = stmt.query_map([], row_to_account)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn row_to_account(r: &rusqlite::Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        platform: r.get(1)?,
        external_id: r.get(2)?,
        handle: r.get(3)?,
        display_name: r.get(4)?,
        status: r.get(5)?,
        connected_at: r.get(6)?,
        last_sync_at: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn upsert_is_idempotent() {
        let conn = open_in_memory().unwrap();
        let new = NewAccount {
            platform: "mock",
            external_id: "m1",
            handle: Some("@m"),
            display_name: "Mock",
        };
        let a = upsert(&conn, &new, "2026-11-16T00:00:00Z").unwrap();
        let b = upsert(&conn, &new, "2026-11-17T00:00:00Z").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }
}
