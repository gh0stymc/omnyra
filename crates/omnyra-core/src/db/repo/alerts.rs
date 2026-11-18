//! Alerts: performance outliers, follower milestones, and other things worth telling the
//! user about. Rows live in the `alerts` table; new ones are turned into system
//! notifications by the app layer (step 2c). The computation is in `crate::alerts`.

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Alert kinds (stored in `alerts.kind`).
pub mod kind {
    pub const MILESTONE: &str = "milestone";
    pub const OVERPERFORMING: &str = "overperforming";
    pub const UNDERPERFORMING: &str = "underperforming";
}

/// Severity (stored in `alerts.severity`); drives the icon/colour in the UI.
pub mod severity {
    pub const POSITIVE: &str = "positive";
    pub const INFO: &str = "info";
    pub const WARNING: &str = "warning";
}

/// Typed payload. `serde_json::Value` is not exposed because `specta` has no JSON feature
/// here; optional fields keep it flexible and still typed on the frontend.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AlertPayload {
    /// Milestone: the metric that crossed a threshold (`"followers"`).
    pub metric: Option<String>,
    /// Milestone: the threshold crossed. Performance: the content's early views.
    pub value: Option<f64>,
    /// Performance: title of the content, for the message.
    pub content_title: Option<String>,
    /// Performance: z-score against the same-kind baseline.
    pub z: Option<f64>,
    /// Performance: `"video" | "short" | ...`.
    pub content_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub account_id: Option<String>,
    pub content_id: Option<String>,
    pub payload: AlertPayload,
    pub created_at: String,
    pub read: bool,
}

/// An alert to be inserted.
#[derive(Debug, Clone)]
pub struct NewAlert {
    pub kind: String,
    pub severity: String,
    pub account_id: Option<String>,
    pub content_id: Option<String>,
    pub payload: AlertPayload,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Alert> {
    let payload: String = r.get(5)?;
    Ok(Alert {
        id: r.get(0)?,
        kind: r.get(1)?,
        severity: r.get(2)?,
        account_id: r.get(3)?,
        content_id: r.get(4)?,
        payload: serde_json::from_str(&payload).unwrap_or_default(),
        created_at: r.get(6)?,
        read: r.get::<_, Option<String>>(7)?.is_some(),
    })
}

const COLS: &str = "id, kind, severity, account_id, content_id, payload, created_at, read_at";

/// Inserts an alert and returns it (UUID v7 id, `created_at = now`).
pub fn insert(conn: &Connection, now_iso: &str, a: &NewAlert) -> Result<Alert> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload = serde_json::to_string(&a.payload).unwrap_or_else(|_| "{}".into());
    conn.execute(
        "INSERT INTO alerts (id, kind, severity, account_id, content_id, payload, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            a.kind,
            a.severity,
            a.account_id,
            a.content_id,
            payload,
            now_iso
        ],
    )?;
    Ok(Alert {
        id,
        kind: a.kind.clone(),
        severity: a.severity.clone(),
        account_id: a.account_id.clone(),
        content_id: a.content_id.clone(),
        payload: a.payload.clone(),
        created_at: now_iso.to_owned(),
        read: false,
    })
}

/// True if a milestone alert for this account and threshold already exists (dedup).
pub fn milestone_exists(conn: &Connection, account_id: &str, value: f64) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM alerts
             WHERE kind = 'milestone' AND account_id = ?1
               AND json_extract(payload, '$.value') = ?2
             LIMIT 1",
            params![account_id, value],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// True if any performance alert already exists for this content (dedup; one per content).
pub fn performance_exists(conn: &Connection, content_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM alerts
             WHERE content_id = ?1 AND kind IN ('overperforming', 'underperforming') LIMIT 1",
            params![content_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Most recent alerts first (for the UI).
pub fn list(conn: &Connection, limit: u32) -> Result<Vec<Alert>> {
    let sql = format!("SELECT {COLS} FROM alerts ORDER BY created_at DESC LIMIT ?1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Alerts not yet turned into a system notification, oldest first.
pub fn list_unnotified(conn: &Connection) -> Result<Vec<Alert>> {
    let sql =
        format!("SELECT {COLS} FROM alerts WHERE notified_at IS NULL ORDER BY created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn mark_notified(conn: &Connection, id: &str, now_iso: &str) -> Result<()> {
    conn.execute(
        "UPDATE alerts SET notified_at = ?2 WHERE id = ?1",
        params![id, now_iso],
    )?;
    Ok(())
}

pub fn unread_count(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM alerts WHERE read_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

pub fn mark_read(conn: &Connection, id: &str, now_iso: &str) -> Result<()> {
    conn.execute(
        "UPDATE alerts SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL",
        params![id, now_iso],
    )?;
    Ok(())
}

pub fn mark_all_read(conn: &Connection, now_iso: &str) -> Result<()> {
    conn.execute(
        "UPDATE alerts SET read_at = ?1 WHERE read_at IS NULL",
        params![now_iso],
    )?;
    Ok(())
}
