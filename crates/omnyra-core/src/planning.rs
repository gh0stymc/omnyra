//! Planning (step 2b): idea board (Kanban), reminders and the calendar.
//!
//! The calendar is a read model: the union of published contents, scheduled
//! publications, reminders and idea due dates. Published contents are history and cannot
//! be moved; the rest can be rescheduled by dragging.

use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

pub const STAGES: &[&str] = &["idea", "script", "recorded", "edited", "published"];

fn validation(field: &str) -> AppError {
    AppError::Validation {
        field: field.into(),
    }
}

// ---- ideas ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IdeaFile {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub stage: String,
    pub position: f64,
    pub target_platforms: Vec<String>,
    pub due_at: Option<String>,
    pub files: Vec<IdeaFile>,
    pub created_at: String,
    pub updated_at: String,
}

/// Editable fields of an idea (all of them at once: the form sends the whole card).
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IdeaInput {
    pub title: String,
    pub notes: Option<String>,
    pub target_platforms: Vec<String>,
    pub due_at: Option<String>,
}

fn check_stage(stage: &str) -> Result<()> {
    if STAGES.contains(&stage) {
        Ok(())
    } else {
        Err(validation("stage"))
    }
}

fn check_input(i: &IdeaInput) -> Result<()> {
    if i.title.trim().is_empty() || i.title.chars().count() > 200 {
        return Err(validation("title"));
    }
    if let Some(d) = &i.due_at {
        check_iso(d, "dueAt")?;
    }
    Ok(())
}

fn check_iso(s: &str, field: &str) -> Result<()> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|_| ())
        .map_err(|_| validation(field))
}

fn files_of(conn: &Connection, idea_id: &str) -> Result<Vec<IdeaFile>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, COALESCE(f.original_name, f.path), f.path FROM idea_files i
         JOIN files f ON f.id = i.file_id WHERE i.idea_id = ?1 ORDER BY f.created_at",
    )?;
    let rows = stmt.query_map(params![idea_id], |r| {
        Ok(IdeaFile {
            id: r.get(0)?,
            name: r.get(1)?,
            path: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

const IDEA_COLUMNS: &str =
    "id, title, notes, stage, position, target_platforms, due_at, created_at, updated_at";

fn row_to_idea(r: &rusqlite::Row) -> rusqlite::Result<Idea> {
    let platforms: String = r.get(5)?;
    Ok(Idea {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        stage: r.get(3)?,
        position: r.get(4)?,
        target_platforms: serde_json::from_str(&platforms).unwrap_or_default(),
        due_at: r.get(6)?,
        files: Vec::new(),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

/// All ideas, by stage and position.
pub fn list_ideas(conn: &Connection) -> Result<Vec<Idea>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {IDEA_COLUMNS} FROM ideas ORDER BY stage, position"
    ))?;
    let ideas = stmt
        .query_map([], row_to_idea)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ideas
        .into_iter()
        .map(|mut i| {
            i.files = files_of(conn, &i.id)?;
            Ok(i)
        })
        .collect()
}

pub fn get_idea(conn: &Connection, id: &str) -> Result<Idea> {
    let mut idea = conn
        .query_row(
            &format!("SELECT {IDEA_COLUMNS} FROM ideas WHERE id = ?1"),
            params![id],
            row_to_idea,
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    idea.files = files_of(conn, id)?;
    Ok(idea)
}

/// New idea at the end of `stage`.
pub fn create_idea(conn: &Connection, stage: &str, input: &IdeaInput, now: &str) -> Result<Idea> {
    check_stage(stage)?;
    check_input(input)?;
    let id = uuid::Uuid::now_v7().to_string();
    let last: Option<f64> = conn.query_row(
        "SELECT MAX(position) FROM ideas WHERE stage = ?1",
        params![stage],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO ideas (id, title, notes, stage, position, target_platforms, due_at,
                            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            input.title.trim(),
            input.notes,
            stage,
            last.unwrap_or(0.0) + 1.0,
            serde_json::to_string(&input.target_platforms).map_err(|_| AppError::Internal)?,
            input.due_at,
            now
        ],
    )?;
    get_idea(conn, &id)
}

pub fn update_idea(conn: &Connection, id: &str, input: &IdeaInput, now: &str) -> Result<Idea> {
    check_input(input)?;
    let n = conn.execute(
        "UPDATE ideas SET title = ?2, notes = ?3, target_platforms = ?4, due_at = ?5,
                          updated_at = ?6 WHERE id = ?1",
        params![
            id,
            input.title.trim(),
            input.notes,
            serde_json::to_string(&input.target_platforms).map_err(|_| AppError::Internal)?,
            input.due_at,
            now
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    get_idea(conn, id)
}

/// Moves an idea to `stage`, at `index` among that column's other cards (clamped).
/// Positions are fractional, so only the moved card is written — unless the gap became
/// too small, in which case the column is renumbered.
pub fn move_idea(
    conn: &mut Connection,
    id: &str,
    stage: &str,
    index: usize,
    now: &str,
) -> Result<()> {
    check_stage(stage)?;
    let tx = conn.transaction()?;
    let exists: bool = tx
        .query_row("SELECT 1 FROM ideas WHERE id = ?1", params![id], |_| Ok(()))
        .optional()?
        .is_some();
    if !exists {
        return Err(AppError::NotFound);
    }
    let others: Vec<(String, f64)> = {
        let mut stmt = tx.prepare(
            "SELECT id, position FROM ideas WHERE stage = ?1 AND id <> ?2 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![stage, id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let index = index.min(others.len());
    let before = index.checked_sub(1).map(|i| others[i].1);
    let after = others.get(index).map(|o| o.1);
    let mut position = match (before, after) {
        (None, None) => 1.0,
        (Some(b), None) => b + 1.0,
        (None, Some(a)) => a - 1.0,
        (Some(b), Some(a)) => (a + b) / 2.0,
    };
    if let (Some(b), Some(a)) = (before, after) {
        if (a - b).abs() < 1e-9 {
            // Renumber the column with the moved card in place.
            for (i, (oid, _)) in others.iter().enumerate() {
                let p = if i < index { i + 1 } else { i + 2 } as f64;
                tx.execute(
                    "UPDATE ideas SET position = ?2 WHERE id = ?1",
                    params![oid, p],
                )?;
            }
            position = (index + 1) as f64;
        }
    }
    tx.execute(
        "UPDATE ideas SET stage = ?2, position = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, stage, position, now],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn delete_idea(conn: &Connection, id: &str) -> Result<()> {
    // Attached files are only references to the user's files: the rows go, the files stay.
    let files: Vec<String> = {
        let mut st = conn.prepare("SELECT file_id FROM idea_files WHERE idea_id = ?1")?;
        let rows = st.query_map(params![id], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    if conn.execute("DELETE FROM ideas WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    for f in files {
        forget_file_if_unused(conn, &f)?;
    }
    Ok(())
}

/// Removes the `files` row of a referenced (unmanaged) file nobody uses any more.
fn forget_file_if_unused(conn: &Connection, file_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM files WHERE id = ?1 AND managed = 0
           AND NOT EXISTS (SELECT 1 FROM idea_files WHERE file_id = ?1)
           AND NOT EXISTS (SELECT 1 FROM publications
                           WHERE source_file_id = ?1 OR thumbnail_file_id = ?1
                              OR rendition_file_id = ?1)",
        params![file_id],
    )?;
    Ok(())
}

/// Attaches a file by reference (it is not copied).
pub fn attach_file(conn: &Connection, idea_id: &str, path: &str, now: &str) -> Result<Idea> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::FileUnavailable)?;
    if !meta.is_file() {
        return Err(AppError::FileUnavailable);
    }
    get_idea(conn, idea_id)?;
    let file_id = uuid::Uuid::now_v7().to_string();
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    conn.execute(
        "INSERT INTO files (id, path, managed, original_name, size_bytes, created_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5)",
        params![file_id, path, name, meta.len() as i64, now],
    )?;
    conn.execute(
        "INSERT INTO idea_files (idea_id, file_id) VALUES (?1, ?2)",
        params![idea_id, file_id],
    )?;
    get_idea(conn, idea_id)
}

pub fn detach_file(conn: &Connection, idea_id: &str, file_id: &str) -> Result<Idea> {
    conn.execute(
        "DELETE FROM idea_files WHERE idea_id = ?1 AND file_id = ?2",
        params![idea_id, file_id],
    )?;
    forget_file_if_unused(conn, file_id)?;
    get_idea(conn, idea_id)
}

/// Path of a file attached to an idea (so the app only ever opens attached files).
pub fn attached_path(conn: &Connection, file_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT f.path FROM files f JOIN idea_files i ON i.file_id = f.id WHERE f.id = ?1",
        params![file_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

// ---- reminders -----------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReminderInput {
    pub title: String,
    pub at: String,
    pub platform: Option<String>,
}

pub fn create_reminder(conn: &Connection, input: &ReminderInput) -> Result<String> {
    if input.title.trim().is_empty() {
        return Err(validation("title"));
    }
    check_iso(&input.at, "at")?;
    let id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO reminders (id, title, at, platform) VALUES (?1, ?2, ?3, ?4)",
        params![id, input.title.trim(), input.at, input.platform],
    )?;
    Ok(id)
}

pub fn delete_reminder(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM reminders WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

// ---- calendar ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    /// Already published (read-only).
    Content,
    /// Scheduled publication.
    Publication,
    Reminder,
    /// An idea's due date.
    Idea,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub kind: EventKind,
    /// ID of the content, publication, reminder or idea.
    pub id: String,
    pub title: String,
    /// RFC 3339, UTC.
    pub at: String,
    pub platform: Option<String>,
    pub account_id: Option<String>,
    /// Content kind (video, short...), publication status or idea stage.
    pub detail: Option<String>,
    pub url: Option<String>,
    pub movable: bool,
}

/// Events with `at` in `[from, to)` (RFC 3339 instants; the UI sends local day bounds).
pub fn calendar(conn: &Connection, from: &str, to: &str) -> Result<Vec<CalendarEvent>> {
    let mut out = Vec::new();
    let mut push = |sql: &str, kind: EventKind, movable: bool| -> Result<()> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok(CalendarEvent {
                kind,
                id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                at: r.get(2)?,
                platform: r.get(3)?,
                account_id: r.get(4)?,
                detail: r.get(5)?,
                url: r.get(6)?,
                movable,
            })
        })?;
        for e in rows {
            out.push(e?);
        }
        Ok(())
    };
    // Timestamps from different sources differ in precision; compare as instants.
    push(
        "SELECT c.id, c.title, c.published_at, a.platform, a.id, c.kind, c.url
         FROM contents c JOIN accounts a ON a.id = c.account_id
         WHERE a.status <> 'disconnected' AND c.published_at IS NOT NULL
           AND julianday(c.published_at) >= julianday(?1)
           AND julianday(c.published_at) < julianday(?2)",
        EventKind::Content,
        false,
    )?;
    push(
        "SELECT p.id, p.title, p.scheduled_at, a.platform, a.id, p.status, NULL
         FROM publications p JOIN accounts a ON a.id = p.account_id
         WHERE p.scheduled_at IS NOT NULL AND p.status NOT IN ('published', 'cancelled')
           AND julianday(p.scheduled_at) >= julianday(?1)
           AND julianday(p.scheduled_at) < julianday(?2)",
        EventKind::Publication,
        true,
    )?;
    push(
        "SELECT id, title, at, platform, NULL, NULL, NULL FROM reminders
         WHERE julianday(at) >= julianday(?1) AND julianday(at) < julianday(?2)",
        EventKind::Reminder,
        true,
    )?;
    push(
        "SELECT id, title, due_at, NULL, NULL, stage, NULL FROM ideas
         WHERE due_at IS NOT NULL
           AND julianday(due_at) >= julianday(?1) AND julianday(due_at) < julianday(?2)",
        EventKind::Idea,
        true,
    )?;
    out.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

/// Reschedules a movable event.
pub fn move_event(conn: &Connection, kind: EventKind, id: &str, at: &str, now: &str) -> Result<()> {
    check_iso(at, "at")?;
    let n = match kind {
        EventKind::Content => return Err(validation("kind")),
        EventKind::Publication => conn.execute(
            "UPDATE publications SET scheduled_at = ?2, updated_at = ?3
             WHERE id = ?1 AND status NOT IN ('published', 'uploading', 'cancelled')",
            params![id, at, now],
        )?,
        EventKind::Reminder => conn.execute(
            "UPDATE reminders SET at = ?2, notified_at = NULL WHERE id = ?1",
            params![id, at],
        )?,
        EventKind::Idea => conn.execute(
            "UPDATE ideas SET due_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, at, now],
        )?,
    };
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn input(title: &str) -> IdeaInput {
        IdeaInput {
            title: title.into(),
            notes: None,
            target_platforms: vec!["youtube".into()],
            due_at: None,
        }
    }

    fn column(conn: &Connection, stage: &str) -> Vec<String> {
        list_ideas(conn)
            .unwrap()
            .into_iter()
            .filter(|i| i.stage == stage)
            .map(|i| i.title)
            .collect()
    }

    #[test]
    fn ideas_are_created_at_the_end_and_moved_by_index() {
        let mut conn = open_in_memory().unwrap();
        let a = create_idea(&conn, "idea", &input("A"), "t").unwrap();
        create_idea(&conn, "idea", &input("B"), "t").unwrap();
        let c = create_idea(&conn, "idea", &input("C"), "t").unwrap();
        assert_eq!(column(&conn, "idea"), vec!["A", "B", "C"]);
        move_idea(&mut conn, &c.id, "idea", 0, "t").unwrap();
        assert_eq!(column(&conn, "idea"), vec!["C", "A", "B"]);
        move_idea(&mut conn, &a.id, "script", 5, "t").unwrap();
        assert_eq!(column(&conn, "idea"), vec!["C", "B"]);
        assert_eq!(column(&conn, "script"), vec!["A"]);
        assert_eq!(
            get_idea(&conn, &a.id).unwrap().target_platforms,
            vec!["youtube"]
        );
    }

    #[test]
    fn repeated_moves_between_two_cards_renumber_when_needed() {
        let mut conn = open_in_memory().unwrap();
        create_idea(&conn, "idea", &input("A"), "t").unwrap();
        create_idea(&conn, "idea", &input("B"), "t").unwrap();
        // Keep inserting between A and B: halving the gap eventually forces a renumber.
        for i in 0..80 {
            let x = create_idea(&conn, "script", &input(&format!("x{i}")), "t").unwrap();
            move_idea(&mut conn, &x.id, "idea", 1, "t").unwrap();
        }
        let col = column(&conn, "idea");
        assert_eq!(col.len(), 82);
        assert_eq!(col.first().unwrap(), "A");
        assert_eq!(col.last().unwrap(), "B");
        assert_eq!(col[1], "x79");
    }

    #[test]
    fn invalid_input_is_rejected() {
        let mut conn = open_in_memory().unwrap();
        assert!(matches!(
            create_idea(&conn, "nope", &input("A"), "t"),
            Err(AppError::Validation { .. })
        ));
        assert!(matches!(
            create_idea(&conn, "idea", &input("  "), "t"),
            Err(AppError::Validation { .. })
        ));
        let mut bad = input("A");
        bad.due_at = Some("tomorrow".into());
        assert!(create_idea(&conn, "idea", &bad, "t").is_err());
        assert!(matches!(
            move_idea(&mut conn, "missing", "idea", 0, "t"),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn files_are_attached_by_reference_and_never_deleted_from_disk() {
        let conn = open_in_memory().unwrap();
        let idea = create_idea(&conn, "idea", &input("A"), "t").unwrap();
        let path = std::env::temp_dir().join(format!("omnyra-idea-{}.txt", uuid::Uuid::now_v7()));
        std::fs::write(&path, "guion").unwrap();
        let p = path.to_string_lossy().to_string();
        let with = attach_file(&conn, &idea.id, &p, "t").unwrap();
        assert_eq!(with.files.len(), 1);
        assert_eq!(attached_path(&conn, &with.files[0].id).unwrap(), p);
        let without = detach_file(&conn, &idea.id, &with.files[0].id).unwrap();
        assert!(without.files.is_empty());
        assert!(path.exists());
        assert!(matches!(
            attach_file(&conn, &idea.id, "/no/such/file", "t"),
            Err(AppError::FileUnavailable)
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn calendar_unites_sources_and_moves_only_movable_events() {
        let conn = open_in_memory().unwrap();
        let acc = crate::db::repo::accounts::upsert(
            &conn,
            "acc",
            &crate::db::repo::accounts::NewAccount {
                platform: "youtube",
                external_id: "UC1",
                handle: None,
                display_name: "Y",
                granted_scopes: &[],
            },
            "t",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contents (id, account_id, external_id, kind, title, published_at,
                                   first_seen_at, last_fetched_at)
             VALUES ('c1', ?1, 'v1', 'short', 'Mi short', '2026-11-10T18:00:00Z', 't', 't')",
            params![acc.id],
        )
        .unwrap();
        let r = create_reminder(
            &conn,
            &ReminderInput {
                title: "Grabar".into(),
                at: "2026-11-12T09:00:00.000Z".into(),
                platform: None,
            },
        )
        .unwrap();
        let mut i = input("Idea con fecha");
        i.due_at = Some("2026-11-20T10:00:00Z".into());
        let idea = create_idea(&conn, "script", &i, "t").unwrap();
        let ev = calendar(&conn, "2026-11-01T00:00:00Z", "2026-12-01T00:00:00Z").unwrap();
        let kinds: Vec<_> = ev.iter().map(|e| (e.kind, e.title.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                (EventKind::Content, "Mi short"),
                (EventKind::Reminder, "Grabar"),
                (EventKind::Idea, "Idea con fecha"),
            ]
        );
        assert_eq!(ev[0].platform.as_deref(), Some("youtube"));
        assert!(!ev[0].movable);

        move_event(&conn, EventKind::Reminder, &r, "2026-12-05T09:00:00Z", "t").unwrap();
        move_event(
            &conn,
            EventKind::Idea,
            &idea.id,
            "2026-11-02T10:00:00Z",
            "t",
        )
        .unwrap();
        assert!(move_event(&conn, EventKind::Content, "c1", "2026-11-02T10:00:00Z", "t").is_err());
        let ev = calendar(&conn, "2026-11-01T00:00:00Z", "2026-12-01T00:00:00Z").unwrap();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].kind, EventKind::Idea);
        delete_reminder(&conn, &r).unwrap();
        assert!(matches!(
            delete_reminder(&conn, &r),
            Err(AppError::NotFound)
        ));
    }
}
