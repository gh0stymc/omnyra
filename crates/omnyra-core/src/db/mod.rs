//! SQLite access. The connection is opened in a single place (`open`/`open_in_memory`)
//! so that encryption (SQLCipher, decision D7) can be added later.

pub mod repo;

use crate::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::OnceLock;

fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: OnceLock<Migrations<'static>> = OnceLock::new();
    MIGRATIONS.get_or_init(|| {
        Migrations::new(vec![
            M::up(include_str!("migrations/0001_init.sql")),
            M::up(include_str!("migrations/0002_uploads.sql")),
            M::up(include_str!("migrations/0003_live.sql")),
            M::up(include_str!("migrations/0004_planning.sql")),
        ])
    })
}

fn configure(conn: &mut Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations().to_latest(conn)?;
    Ok(())
}

pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&mut conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        migrations().validate().expect("valid migrations");
    }

    #[test]
    fn audit_log_is_append_only() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO audit_log (ts, action) VALUES ('2026-01-01T00:00:00Z', 'account.connect')",
            [],
        )
        .unwrap();
        assert!(conn
            .execute("UPDATE audit_log SET action = 'x'", [])
            .is_err());
        assert!(conn.execute("DELETE FROM audit_log", []).is_err());
    }
}
