//! Background FFmpeg worker (3c): renders the adapted version of each publication waiting
//! in `rendering`, one at a time (renders are heavy), then hands it to the upload queue.

use crate::events::{RenderProgress, UploadChanged};
use crate::state::AppState;
use omnyra_core::{media::ffmpeg, uploads};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

const IDLE_CHECK: Duration = Duration::from_secs(60);

fn changed(app: &AppHandle, id: &str, status: &str) {
    let _ = UploadChanged {
        publication_id: id.to_owned(),
        status: status.to_owned(),
    }
    .emit(app);
}

pub fn spawn_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        loop {
            let job = state
                .db
                .lock()
                .ok()
                .and_then(|c| uploads::next_render(&c).ok().flatten());
            let Some(job) = job else {
                tokio::select! {
                    _ = state.render_wake.notified() => {}
                    _ = tokio::time::sleep(IDLE_CHECK) => {}
                }
                continue;
            };
            let dir = state.data_dir.join("renders");
            let _ = std::fs::create_dir_all(&dir);
            let out = dir.join(format!("{}.mp4", job.id));
            let media = match job.media.clone() {
                Some(m) => Some(m),
                None => omnyra_core::media::probe(std::path::Path::new(&job.source_path))
                    .await
                    .ok(),
            };
            let emitter = app.clone();
            let key = job.id.clone();
            let result = ffmpeg::render(
                std::path::Path::new(&job.source_path),
                &out,
                &job.spec,
                media.as_ref(),
                move |p| {
                    let _ = RenderProgress {
                        key: key.clone(),
                        progress: p,
                    }
                    .emit(&emitter);
                },
            )
            .await;
            let now = state.clock.now_iso();
            let Ok(mut conn) = state.db.lock() else {
                continue;
            };
            match result {
                Ok(()) => {
                    match uploads::finish_render(&mut conn, &job.id, &out.to_string_lossy(), &now) {
                        Ok(true) => {
                            drop(conn);
                            changed(&app, &job.id, "queued");
                            state.upload_wake.notify_one();
                        }
                        Ok(false) => {}
                        Err(e) => tracing::warn!(error = %e, "could not store the rendition"),
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "render failed");
                    let _ = uploads::fail_render(&conn, &job.id, &e, &now);
                    drop(conn);
                    changed(&app, &job.id, "failed");
                }
            }
        }
    });
}
