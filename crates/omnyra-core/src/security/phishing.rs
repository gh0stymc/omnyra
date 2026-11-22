//! Phishing detection in comments (step 6c).

use crate::Result;
use rusqlite::Connection;

/// Scores the given comments (filled in by step 6c).
pub fn apply(_conn: &Connection, _ids: &[String]) -> Result<()> {
    Ok(())
}
