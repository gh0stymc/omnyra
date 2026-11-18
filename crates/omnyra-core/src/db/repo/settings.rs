//! App settings: a tiny key/value store (`settings` table). Values are plain strings;
//! booleans are stored as `"true"`/`"false"`. Never store secrets here (tokens and the
//! upload session URI live in the OS keyring, decision D4/H6).

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// Raw value for `key`, or `None` if unset.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?)
}

/// Upsert `key = value`.
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Boolean value for `key`, falling back to `default` when unset or unparseable.
pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool> {
    Ok(get(conn, key)?.map(|v| v == "true").unwrap_or(default))
}

pub fn set_bool(conn: &Connection, key: &str, value: bool) -> Result<()> {
    set(conn, key, if value { "true" } else { "false" })
}

/// Setting keys used across the app, so a typo is a compile error.
pub mod keys {
    /// Keep running in the system tray when the window is closed.
    pub const CLOSE_TO_TRAY: &str = "close_to_tray";
    /// Launch Omnyra automatically when the user signs in.
    pub const AUTOSTART: &str = "autostart";
    /// Run periodic background syncs (snapshots) while the app is open.
    pub const BACKGROUND_SYNC: &str = "background_sync";
    /// Show system notifications (reminders, performance alerts, milestones).
    pub const NOTIFICATIONS: &str = "notifications";
    /// UI language (`"es"` | `"en"`), mirrored from the frontend so Rust can localise
    /// system notifications, which are rendered outside the webview.
    pub const LOCALE: &str = "locale";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn get_set_and_defaults() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get(&conn, keys::CLOSE_TO_TRAY).unwrap(), None);
        // Default is returned when unset.
        assert!(get_bool(&conn, keys::BACKGROUND_SYNC, true).unwrap());
        assert!(!get_bool(&conn, keys::CLOSE_TO_TRAY, false).unwrap());

        set_bool(&conn, keys::CLOSE_TO_TRAY, true).unwrap();
        assert!(get_bool(&conn, keys::CLOSE_TO_TRAY, false).unwrap());
        // Upsert overwrites.
        set_bool(&conn, keys::CLOSE_TO_TRAY, false).unwrap();
        assert!(!get_bool(&conn, keys::CLOSE_TO_TRAY, true).unwrap());

        set(&conn, "greeting", "hola").unwrap();
        assert_eq!(get(&conn, "greeting").unwrap().as_deref(), Some("hola"));
    }
}
