use crate::state::AppState;
use omnyra_core::connectors::PlatformInfo;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn list_platforms(state: State<'_, AppState>) -> Vec<PlatformInfo> {
    state
        .registry
        .all()
        .iter()
        .map(|c| c.info().clone())
        .collect()
}
