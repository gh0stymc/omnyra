//! Rule-based comment classification (step 4b).

use crate::Result;
use rusqlite::Connection;

/// Classifies the given comments (filled in by step 4b).
pub fn apply(_conn: &Connection, _ids: &[String]) -> Result<()> {
    Ok(())
}
