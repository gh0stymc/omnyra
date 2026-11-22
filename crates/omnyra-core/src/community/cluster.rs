//! Repeated questions (step 4b).

use crate::Result;
use rusqlite::Connection;

/// Regroups similar questions (filled in by step 4b).
pub fn rebuild(_conn: &Connection, _now: &str) -> Result<()> {
    Ok(())
}
