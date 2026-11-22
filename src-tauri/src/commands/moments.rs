use crate::state::AppState;
use omnyra_core::{
    accounts::account_ctx,
    db::repo::accounts,
    media::ffmpeg::{self, Aspect, Fit, RenditionSpec},
    moments::{self, Moment},
    quota::Unmetered,
    AppError,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

/// Retention is re-fetched after this long (it keeps changing while the video gets views).
const RETENTION_MAX_AGE: time::Duration = time::Duration::days(7);

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MomentsReport {
    /// `retention` | `chat` | `None` when there is nothing to analyse.
    pub source: Option<String>,
    pub duration_s: Option<f64>,
    /// The analysed series: (seconds, value).
    pub series: Vec<(f64, f64)>,
    pub moments: Vec<Moment>,
}

/// Proposed fragments of a content: YouTube retention, or the chat Omnyra recorded during
/// a Twitch stream (for its VOD).
#[tauri::command]
#[specta::specta]
pub async fn get_moments(
    state: State<'_, AppState>,
    content_id: String,
) -> Result<MomentsReport, AppError> {
    let (account_id, platform, external_id, duration, extra): (
        String,
        String,
        String,
        Option<i64>,
        String,
    ) = {
        let c = state.db.lock().map_err(|_| AppError::Internal)?;
        c.query_row(
            "SELECT k.account_id, a.platform, k.external_id, k.duration_s, k.extra
             FROM contents k JOIN accounts a ON a.id = k.account_id WHERE k.id = ?1",
            [&content_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(|_| AppError::NotFound)?
    };
    let duration = duration.map(|d| d as f64);

    if platform == "twitch" {
        let stream_id = serde_json::from_str::<serde_json::Value>(&extra)
            .ok()
            .and_then(|v| v["stream_id"].as_str().map(str::to_owned));
        let session: Option<String> = match stream_id {
            Some(sid) => {
                let c = state.db.lock().map_err(|_| AppError::Internal)?;
                c.query_row(
                    "SELECT id FROM live_sessions WHERE account_id = ?1 AND external_id = ?2",
                    [&account_id, &sid],
                    |r| r.get(0),
                )
                .ok()
            }
            None => None,
        };
        let Some(session) = session else {
            return Ok(MomentsReport {
                source: None,
                duration_s: duration,
                series: vec![],
                moments: vec![],
            });
        };
        let series =
            moments::chat_series(&*state.db.lock().map_err(|_| AppError::Internal)?, &session)?;
        let found = moments::chat_peaks(&series, duration);
        return Ok(MomentsReport {
            source: Some("chat".into()),
            duration_s: duration,
            series,
            moments: found,
        });
    }

    let Some(d) = duration else {
        return Ok(MomentsReport {
            source: None,
            duration_s: None,
            series: vec![],
            moments: vec![],
        });
    };
    let cached = moments::load_series(
        &*state.db.lock().map_err(|_| AppError::Internal)?,
        &content_id,
        "retention",
    )?;
    let fresh = cached.as_ref().is_some_and(|(_, at)| {
        time::OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339)
            .is_ok_and(|t| state.clock.now() - t < RETENTION_MAX_AGE)
    });
    let points = if fresh {
        cached.map(|c| c.0).unwrap_or_default()
    } else {
        let account = accounts::get(
            &*state.db.lock().map_err(|_| AppError::Internal)?,
            &account_id,
        )?;
        let connector = state
            .registry
            .get(&platform)
            .ok_or(AppError::NotSupported)?;
        let ctx = account_ctx(
            &state.auth,
            connector.as_ref(),
            &account,
            Arc::new(Unmetered),
        );
        match connector.retention_curve(&ctx, &external_id).await {
            Ok(p) => {
                let c = state.db.lock().map_err(|_| AppError::Internal)?;
                moments::save_series(&c, &content_id, "retention", &p, &state.clock.now_iso())?;
                p
            }
            Err(AppError::NotSupported) => Vec::new(),
            Err(e) => match cached {
                Some((p, _)) => p,
                None => return Err(e),
            },
        }
    };
    let found = moments::retention_peaks(&points, d);
    Ok(MomentsReport {
        source: (!points.is_empty()).then(|| "retention".into()),
        duration_s: Some(d),
        series: points.iter().map(|(x, y)| (x * d, *y)).collect(),
        moments: found,
    })
}

/// Cuts `[start_s, end_s)` of a local file in vertical (9:16) into
/// `app_data_dir/clips/` and returns the path (then handed to multi-network publishing).
#[tauri::command]
#[specta::specta]
pub async fn cut_moment(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source: String,
    start_s: f64,
    end_s: f64,
    fit: Fit,
) -> Result<String, AppError> {
    let src = std::path::Path::new(&source);
    if !src.is_file() {
        return Err(AppError::FileUnavailable);
    }
    if !(end_s > start_s && end_s - start_s <= 180.0 && start_s >= 0.0) {
        return Err(AppError::Validation {
            field: "range".into(),
        });
    }
    let dir = state.data_dir.join("clips");
    std::fs::create_dir_all(&dir).map_err(|_| AppError::FileUnavailable)?;
    let out = dir.join(format!("clip-{}.mp4", uuid::Uuid::now_v7()));
    let spec = RenditionSpec {
        aspect: Aspect::Vertical,
        fit,
        start_s: Some(start_s),
        end_s: Some(end_s),
        max_duration_s: None,
        max_size_mb: None,
    };
    let info = omnyra_core::media::probe(src).await.ok();
    let key = format!("clip:{start_s}");
    let emitter = app.clone();
    ffmpeg::render(src, &out, &spec, info.as_ref(), move |p| {
        use tauri_specta::Event;
        let _ = crate::events::RenderProgress {
            key: key.clone(),
            progress: p,
        }
        .emit(&emitter);
    })
    .await?;
    Ok(out.to_string_lossy().into_owned())
}
