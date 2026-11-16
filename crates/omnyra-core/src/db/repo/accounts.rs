use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    /// The refresh token was rejected (expired, revoked from Google...): reconnect.
    NeedsReauth,
    /// Disconnected by the user. Tokens are gone; historical data is kept.
    Disconnected,
}

impl AccountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::NeedsReauth => "needs_reauth",
            Self::Disconnected => "disconnected",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "needs_reauth" => Self::NeedsReauth,
            _ => Self::Disconnected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub external_id: String,
    pub handle: Option<String>,
    pub display_name: String,
    /// Only for display ("Connected accounts"). The tokens themselves are in the keyring.
    pub granted_scopes: Vec<String>,
    pub status: AccountStatus,
    pub connected_at: String,
    pub last_sync_at: Option<String>,
}

pub struct NewAccount<'a> {
    pub platform: &'a str,
    pub external_id: &'a str,
    pub handle: Option<&'a str>,
    pub display_name: &'a str,
    pub granted_scopes: &'a [String],
}

const COLUMNS: &str = "id, platform, external_id, handle, display_name, granted_scopes, status,
                       connected_at, last_sync_at";

/// The existing ID for (platform, external_id), or a fresh UUID v7. Lets the caller store the
/// tokens under the final ID before writing the row.
pub fn id_for(conn: &Connection, platform: &str, external_id: &str) -> Result<String> {
    Ok(find_by_external(conn, platform, external_id)?
        .map(|a| a.id)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string()))
}

/// Inserts the account, or refreshes the existing one for the same (platform, external_id)
/// and marks it active again. `connected_at` is updated on every (re)connection.
pub fn upsert(conn: &Connection, id: &str, new: &NewAccount, now_iso: &str) -> Result<Account> {
    let scopes = serde_json::to_string(new.granted_scopes).map_err(|_| AppError::Internal)?;
    conn.execute(
        "INSERT INTO accounts (id, platform, external_id, handle, display_name, granted_scopes,
                               status, connected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)
         ON CONFLICT (platform, external_id) DO UPDATE SET
           handle = excluded.handle, display_name = excluded.display_name,
           granted_scopes = excluded.granted_scopes, status = 'active',
           connected_at = excluded.connected_at",
        params![
            id,
            new.platform,
            new.external_id,
            new.handle,
            new.display_name,
            scopes,
            now_iso
        ],
    )?;
    find_by_external(conn, new.platform, new.external_id)?.ok_or(AppError::Internal)
}

pub fn find_by_external(
    conn: &Connection,
    platform: &str,
    external_id: &str,
) -> Result<Option<Account>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM accounts WHERE platform = ?1 AND external_id = ?2"),
            params![platform, external_id],
            row_to_account,
        )
        .optional()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Account> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM accounts WHERE id = ?1"),
        params![id],
        row_to_account,
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

pub fn list(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM accounts ORDER BY platform, display_name"
    ))?;
    let rows = stmt.query_map([], row_to_account)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Returns whether the status actually changed.
pub fn set_status(conn: &Connection, id: &str, status: AccountStatus) -> Result<bool> {
    let n = conn.execute(
        "UPDATE accounts SET status = ?2 WHERE id = ?1 AND status <> ?2",
        params![id, status.as_str()],
    )?;
    Ok(n > 0)
}

fn row_to_account(r: &rusqlite::Row) -> rusqlite::Result<Account> {
    let scopes: String = r.get(5)?;
    let status: String = r.get(6)?;
    Ok(Account {
        id: r.get(0)?,
        platform: r.get(1)?,
        external_id: r.get(2)?,
        handle: r.get(3)?,
        display_name: r.get(4)?,
        granted_scopes: serde_json::from_str(&scopes).unwrap_or_default(),
        status: AccountStatus::parse(&status),
        connected_at: r.get(7)?,
        last_sync_at: r.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn new<'a>(scopes: &'a [String]) -> NewAccount<'a> {
        NewAccount {
            platform: "mock",
            external_id: "m1",
            handle: Some("@m"),
            display_name: "Mock",
            granted_scopes: scopes,
        }
    }

    #[test]
    fn upsert_is_idempotent() {
        let conn = open_in_memory().unwrap();
        let id = id_for(&conn, "mock", "m1").unwrap();
        let a = upsert(&conn, &id, &new(&[]), "2026-11-16T00:00:00Z").unwrap();
        let id2 = id_for(&conn, "mock", "m1").unwrap();
        assert_eq!(id, id2);
        let b = upsert(&conn, &id2, &new(&[]), "2026-11-17T00:00:00Z").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn reconnect_reactivates_and_updates_scopes() {
        let conn = open_in_memory().unwrap();
        let id = id_for(&conn, "mock", "m1").unwrap();
        upsert(&conn, &id, &new(&["a".into()]), "t0").unwrap();
        assert!(set_status(&conn, &id, AccountStatus::Disconnected).unwrap());
        assert!(!set_status(&conn, &id, AccountStatus::Disconnected).unwrap());
        assert_eq!(get(&conn, &id).unwrap().status, AccountStatus::Disconnected);
        let a = upsert(&conn, &id, &new(&["a".into(), "b".into()]), "t1").unwrap();
        assert_eq!(a.status, AccountStatus::Active);
        assert_eq!(a.granted_scopes, vec!["a", "b"]);
        assert_eq!(a.connected_at, "t1");
    }

    #[test]
    fn get_missing_is_not_found() {
        let conn = open_in_memory().unwrap();
        assert!(matches!(get(&conn, "nope"), Err(AppError::NotFound)));
    }
}
