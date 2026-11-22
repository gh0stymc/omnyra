use super::lock_db;
use crate::state::AppState;
use omnyra_core::{
    planning::{self, CalendarEvent, EventKind, Idea, IdeaInput, ReminderInput},
    AppError,
};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn list_ideas(state: State<'_, AppState>) -> Result<Vec<Idea>, AppError> {
    planning::list_ideas(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn create_idea(
    state: State<'_, AppState>,
    stage: String,
    input: IdeaInput,
) -> Result<Idea, AppError> {
    planning::create_idea(&*lock_db(&state)?, &stage, &input, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn update_idea(
    state: State<'_, AppState>,
    id: String,
    input: IdeaInput,
) -> Result<Idea, AppError> {
    planning::update_idea(&*lock_db(&state)?, &id, &input, &state.clock.now_iso())
}

/// Moves a card to `stage` at `index` (0 = top) among that column's other cards.
#[tauri::command]
#[specta::specta]
pub fn move_idea(
    state: State<'_, AppState>,
    id: String,
    stage: String,
    index: u32,
) -> Result<(), AppError> {
    planning::move_idea(
        &mut *lock_db(&state)?,
        &id,
        &stage,
        index as usize,
        &state.clock.now_iso(),
    )
}

#[tauri::command]
#[specta::specta]
pub fn delete_idea(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    planning::delete_idea(&*lock_db(&state)?, &id)
}

/// Attaches a local file (by reference: it is not copied).
#[tauri::command]
#[specta::specta]
pub fn attach_idea_file(
    state: State<'_, AppState>,
    idea_id: String,
    path: String,
) -> Result<Idea, AppError> {
    planning::attach_file(&*lock_db(&state)?, &idea_id, &path, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn detach_idea_file(
    state: State<'_, AppState>,
    idea_id: String,
    file_id: String,
) -> Result<Idea, AppError> {
    planning::detach_file(&*lock_db(&state)?, &idea_id, &file_id)
}

/// Opens an attached file with the system's default app. Only files attached to an idea
/// can be opened this way (the frontend never gets a generic "open any path").
#[tauri::command]
#[specta::specta]
pub fn open_idea_file(state: State<'_, AppState>, file_id: String) -> Result<(), AppError> {
    let path = planning::attached_path(&*lock_db(&state)?, &file_id)?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|_| AppError::FileUnavailable)
}

/// Events with `at` in `[from, to)` (RFC 3339).
#[tauri::command]
#[specta::specta]
pub fn get_calendar(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<Vec<CalendarEvent>, AppError> {
    planning::calendar(&*lock_db(&state)?, &from, &to)
}

#[tauri::command]
#[specta::specta]
pub fn move_calendar_event(
    state: State<'_, AppState>,
    kind: EventKind,
    id: String,
    at: String,
) -> Result<(), AppError> {
    planning::move_event(&*lock_db(&state)?, kind, &id, &at, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn create_reminder(
    state: State<'_, AppState>,
    input: ReminderInput,
) -> Result<String, AppError> {
    planning::create_reminder(&*lock_db(&state)?, &input)
}

#[tauri::command]
#[specta::specta]
pub fn delete_reminder(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    planning::delete_reminder(&*lock_db(&state)?, &id)
}
