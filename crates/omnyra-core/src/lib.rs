//! Omnyra core: pure Rust logic (no Tauri/WebKit), so it can be tested fast.

pub mod accounts;
pub mod alerts;
pub mod assistant;
pub mod auth;
pub mod best_time;
pub mod clock;
pub mod community;
pub mod competitors;
pub mod connectors;
pub mod creation;
pub mod dashboard;
pub mod db;
pub mod error;
pub mod live;
pub mod media;
pub mod moments;
pub mod paths;
pub mod planning;
pub mod quota;
pub mod security;
pub mod sponsors;
pub mod sync;
pub mod uploads;

pub use error::{AppError, Result};

/// `n` days as a [`time::Duration`] (convenience for callers without a `time` dependency).
pub fn time_days(n: i64) -> time::Duration {
    time::Duration::days(n)
}

/// `YYYY-MM-DD` (UTC) for a timestamp.
pub fn format_day(t: time::OffsetDateTime) -> String {
    let d = t.date();
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}
pub use rusqlite;
