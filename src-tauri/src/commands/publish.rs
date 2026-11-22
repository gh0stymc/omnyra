use crate::events::RenderProgress;
use crate::state::AppState;
use omnyra_core::{
    connectors::UploadDraft,
    db::repo::accounts,
    media::{
        self,
        ffmpeg::{self, RenditionSpec},
    },
    uploads::{self, EnqueueOptions, ValidationReport},
    AppError,
};
use serde::Deserialize;
use tauri::State;
use tauri_specta::Event;

/// One destination of a multi-network post.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublishTarget {
    pub account_id: String,
    pub draft: UploadDraft,
    pub render: Option<RenditionSpec>,
    /// RFC 3339; `None` = right away.
    pub scheduled_at: Option<String>,
}

/// Default adaptation (aspect, limits) for a network.
#[tauri::command]
#[specta::specta]
pub fn rendition_preset(platform: String) -> RenditionSpec {
    ffmpeg::preset(&platform)
}

/// Validates one target as it will be after rendering.
#[tauri::command]
#[specta::specta]
pub async fn validate_target(
    state: State<'_, AppState>,
    target: PublishTarget,
) -> Result<ValidationReport, AppError> {
    let platform = {
        let conn = state.db.lock().map_err(|_| AppError::Internal)?;
        accounts::get(&conn, &target.account_id)?.platform
    };
    let c = state
        .registry
        .get(&platform)
        .ok_or(AppError::NotSupported)?;
    Ok(uploads::validate_with(c.as_ref(), &target.draft, target.render.as_ref()).await)
}

/// Queues every target under one batch. All targets are validated first: nothing is
/// queued if one fails (`VALIDATION` with field `<accountId>:<field>`).
#[tauri::command]
#[specta::specta]
pub async fn publish_batch(
    state: State<'_, AppState>,
    targets: Vec<PublishTarget>,
) -> Result<Vec<String>, AppError> {
    if targets.is_empty() {
        return Err(AppError::Validation {
            field: "targets".into(),
        });
    }
    let mut resolved = Vec::new();
    for t in &targets {
        let account = {
            let conn = state.db.lock().map_err(|_| AppError::Internal)?;
            accounts::get(&conn, &t.account_id)?
        };
        let c = state
            .registry
            .get(&account.platform)
            .ok_or(AppError::NotSupported)?;
        let needed = c.upload_scopes(t.draft.playlist_id.is_some());
        if !needed.iter().all(|s| account.granted_scopes.contains(s)) {
            return Err(AppError::ScopesDenied);
        }
        let report = uploads::validate_with(c.as_ref(), &t.draft, t.render.as_ref()).await;
        if let Some(i) = report
            .issues
            .iter()
            .find(|i| i.severity == omnyra_core::connectors::Severity::Error)
        {
            return Err(AppError::Validation {
                field: format!("{}:{}", t.account_id, i.field),
            });
        }
        resolved.push(c);
    }
    let batch = uuid::Uuid::now_v7().to_string();
    let mut ids = Vec::new();
    for (t, c) in targets.iter().zip(resolved) {
        let opts = EnqueueOptions {
            batch_id: Some(batch.clone()),
            render: t.render.clone(),
            scheduled_at: t.scheduled_at.clone(),
        };
        ids.push(
            uploads::enqueue_with(
                &state.db,
                state.clock.as_ref(),
                c.as_ref(),
                &t.account_id,
                &t.draft,
                &opts,
            )
            .await?,
        );
    }
    state.render_wake.notify_one();
    state.upload_wake.notify_one();
    Ok(ids)
}

/// "Prepare and download": renders `spec` from `source` into `dest` (chosen by the user
/// in a save dialog) for networks without API upload. Progress comes as
/// `RenderProgress { key }`.
#[tauri::command]
#[specta::specta]
pub async fn export_rendition(
    app: tauri::AppHandle,
    source: String,
    spec: RenditionSpec,
    dest: String,
    key: String,
) -> Result<(), AppError> {
    let src = std::path::Path::new(&source);
    if !src.is_file() {
        return Err(AppError::FileUnavailable);
    }
    let info = media::probe(src).await.ok();
    let emitter = app.clone();
    ffmpeg::render(
        src,
        std::path::Path::new(&dest),
        &spec,
        info.as_ref(),
        move |p| {
            let _ = RenderProgress {
                key: key.clone(),
                progress: p,
            }
            .emit(&emitter);
        },
    )
    .await
}
