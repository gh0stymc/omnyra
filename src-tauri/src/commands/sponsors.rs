use crate::state::AppState;
use omnyra_core::{
    sponsors::{self, MediaKitConfig, MoneyTotals, Sponsorship, SponsorshipInput},
    AppError,
};
use serde::Serialize;
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SponsorOverview {
    pub items: Vec<Sponsorship>,
    pub totals: Vec<MoneyTotals>,
}

#[tauri::command]
#[specta::specta]
pub fn list_sponsorships(state: State<'_, AppState>) -> Result<SponsorOverview, AppError> {
    let items = sponsors::list(&*lock_db(&state)?, &state.clock.now_iso())?;
    Ok(SponsorOverview {
        totals: sponsors::totals(&items),
        items,
    })
}

#[tauri::command]
#[specta::specta]
pub fn create_sponsorship(
    state: State<'_, AppState>,
    input: SponsorshipInput,
) -> Result<Sponsorship, AppError> {
    sponsors::create(&*lock_db(&state)?, &input, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn update_sponsorship(
    state: State<'_, AppState>,
    id: String,
    input: SponsorshipInput,
) -> Result<Sponsorship, AppError> {
    sponsors::update(&*lock_db(&state)?, &id, &input, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn delete_sponsorship(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    sponsors::delete(&*lock_db(&state)?, &id)
}

#[tauri::command]
#[specta::specta]
pub fn attach_sponsorship_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<Sponsorship, AppError> {
    sponsors::attach_file(&*lock_db(&state)?, &id, &path, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn detach_sponsorship_file(
    state: State<'_, AppState>,
    id: String,
    file_id: String,
) -> Result<Sponsorship, AppError> {
    sponsors::detach_file(&*lock_db(&state)?, &id, &file_id, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn open_sponsorship_file(state: State<'_, AppState>, file_id: String) -> Result<(), AppError> {
    let path = sponsors::attached_path(&*lock_db(&state)?, &file_id)?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|_| AppError::FileUnavailable)
}

/// Media kit as self-contained HTML (preview, print to PDF).
#[tauri::command]
#[specta::specta]
pub fn media_kit_html(
    state: State<'_, AppState>,
    config: MediaKitConfig,
) -> Result<String, AppError> {
    sponsors::media_kit_html(&*lock_db(&state)?, &config, &state.clock.now_iso())
}

/// Saves the media kit HTML where the user chose.
#[tauri::command]
#[specta::specta]
pub fn save_media_kit(
    state: State<'_, AppState>,
    config: MediaKitConfig,
    dest: String,
) -> Result<(), AppError> {
    let html = sponsors::media_kit_html(&*lock_db(&state)?, &config, &state.clock.now_iso())?;
    std::fs::write(dest, html).map_err(|_| AppError::FileUnavailable)
}
