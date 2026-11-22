use crate::state::AppState;
use omnyra_core::{
    creation::{
        self, Asset, AssetInput, TemplateInput, TextTemplate, TextTemplateInput, ThumbnailTemplate,
    },
    AppError,
};
use tauri::State;

fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, omnyra_core::rusqlite::Connection>, AppError> {
    state.db.lock().map_err(|_| AppError::Internal)
}

#[tauri::command]
#[specta::specta]
pub fn list_assets(state: State<'_, AppState>) -> Result<Vec<Asset>, AppError> {
    creation::list_assets(&*lock_db(&state)?)
}

/// Copies an image chosen by the user into the library.
#[tauri::command]
#[specta::specta]
pub fn import_asset(
    state: State<'_, AppState>,
    path: String,
    input: AssetInput,
) -> Result<Asset, AppError> {
    creation::import_asset(
        &*lock_db(&state)?,
        &state.data_dir.join("files"),
        std::path::Path::new(&path),
        &input,
        &state.clock.now_iso(),
    )
}

#[tauri::command]
#[specta::specta]
pub fn update_asset(
    state: State<'_, AppState>,
    id: String,
    input: AssetInput,
) -> Result<Asset, AppError> {
    creation::update_asset(&*lock_db(&state)?, &id, &input)
}

#[tauri::command]
#[specta::specta]
pub fn delete_asset(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    creation::delete_asset(&*lock_db(&state)?, &id)
}

#[tauri::command]
#[specta::specta]
pub fn asset_data_url(state: State<'_, AppState>, id: String) -> Result<String, AppError> {
    creation::asset_data_url(&*lock_db(&state)?, &id)
}

#[tauri::command]
#[specta::specta]
pub fn list_thumbnail_templates(
    state: State<'_, AppState>,
) -> Result<Vec<ThumbnailTemplate>, AppError> {
    creation::list_templates(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_thumbnail_template(
    state: State<'_, AppState>,
    input: TemplateInput,
) -> Result<ThumbnailTemplate, AppError> {
    creation::save_template(&*lock_db(&state)?, &input, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn delete_thumbnail_template(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    creation::delete_template(&*lock_db(&state)?, &id)
}

/// Saves an exported thumbnail (data URL) in `app_data_dir/thumbnails` and returns its
/// path, ready to be used as an upload's thumbnail.
#[tauri::command]
#[specta::specta]
pub fn save_thumbnail_export(
    state: State<'_, AppState>,
    data_url: String,
    name: String,
) -> Result<String, AppError> {
    creation::save_export(&state.data_dir.join("thumbnails"), &data_url, &name)
        .map(|p| p.to_string_lossy().into_owned())
}

/// Writes an exported thumbnail to a path the user chose in a save dialog.
#[tauri::command]
#[specta::specta]
pub fn write_thumbnail_file(data_url: String, dest: String) -> Result<(), AppError> {
    let dir = std::path::Path::new(&dest)
        .parent()
        .ok_or(AppError::FileUnavailable)?
        .to_path_buf();
    let tmp = creation::save_export(&dir, &data_url, "omnyra-tmp")?;
    std::fs::rename(&tmp, &dest).map_err(|_| AppError::FileUnavailable)
}

#[tauri::command]
#[specta::specta]
pub fn list_text_templates(state: State<'_, AppState>) -> Result<Vec<TextTemplate>, AppError> {
    creation::list_text_templates(&*lock_db(&state)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_text_template(
    state: State<'_, AppState>,
    input: TextTemplateInput,
) -> Result<TextTemplate, AppError> {
    creation::save_text_template(&*lock_db(&state)?, &input, &state.clock.now_iso())
}

#[tauri::command]
#[specta::specta]
pub fn delete_text_template(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    creation::delete_text_template(&*lock_db(&state)?, &id)
}
