//! `ffprobe` wrapper. FFmpeg is a package dependency on Linux; on Windows it is not bundled
//! yet (H9), so a missing ffprobe is a normal situation: callers degrade (no duration
//! check) instead of failing.

use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::time::Duration;

/// Overridable with the `OMNYRA_FFPROBE` environment variable (a settings entry later).
fn ffprobe_bin() -> String {
    std::env::var("OMNYRA_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub duration_s: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    /// Container as ffprobe names it (`mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`...).
    pub container: Option<String>,
}

impl MediaInfo {
    pub fn has_video(&self) -> bool {
        self.video_codec.is_some()
    }
}

#[derive(Deserialize)]
struct Output {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    /// Cover art in audio files shows up as a video stream with this disposition.
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
}

pub(crate) fn parse(json: &str) -> Result<MediaInfo> {
    let out: Output = serde_json::from_str(json).map_err(|_| AppError::Internal)?;
    let video = out
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0);
    let audio = out
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    Ok(MediaInfo {
        duration_s: out
            .format
            .as_ref()
            .and_then(|f| f.duration.as_deref())
            .and_then(|d| d.parse().ok()),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        video_codec: video.and_then(|v| v.codec_name.clone()),
        audio_codec: audio.and_then(|a| a.codec_name.clone()),
        container: out.format.and_then(|f| f.format_name),
    })
}

/// `Err(NotSupported)` if ffprobe is not installed; `Err(Validation{file})` if it cannot
/// read the file as media.
pub async fn probe(path: &Path) -> Result<MediaInfo> {
    let mut cmd = tokio::process::Command::new(ffprobe_bin());
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path)
    .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console flashing up on Windows.
        cmd.creation_flags(0x0800_0000);
    }
    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Err(_) => return Err(AppError::Network), // timed out; treated as transient
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::NotSupported)
        }
        Ok(Err(_)) => return Err(AppError::Internal),
        Ok(Ok(o)) => o,
    };
    if !out.status.success() {
        return Err(AppError::Validation {
            field: "file".into(),
        });
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_with_audio() {
        let info = parse(
            r#"{"streams":[
                {"codec_type":"video","codec_name":"h264","width":1080,"height":1920},
                {"codec_type":"audio","codec_name":"aac"}],
               "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"42.500000"}}"#,
        )
        .unwrap();
        assert_eq!(info.duration_s, Some(42.5));
        assert_eq!((info.width, info.height), (Some(1080), Some(1920)));
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert!(info.has_video());
    }

    #[test]
    fn cover_art_is_not_video() {
        let info = parse(
            r#"{"streams":[
                {"codec_type":"audio","codec_name":"mp3"},
                {"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}}],
               "format":{"format_name":"mp3","duration":"200"}}"#,
        )
        .unwrap();
        assert!(!info.has_video());
    }

    /// Uses the real ffprobe if installed: a generated 1 s clip.
    #[tokio::test]
    async fn probes_a_real_file_when_ffmpeg_is_installed() {
        let dir = std::env::temp_dir().join(format!("omnyra-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10",
            ])
            .args(["-t", "1", "-pix_fmt", "yuv420p"])
            .arg(&file)
            .status();
        if !matches!(made, Ok(s) if s.success()) {
            eprintln!("ffmpeg not available; skipping");
            return;
        }
        let info = probe(&file).await.unwrap();
        assert!(info.has_video());
        assert_eq!(info.width, Some(320));
        assert!((info.duration_s.unwrap() - 1.0).abs() < 0.2);
        let not_media = dir.join("note.txt");
        std::fs::write(&not_media, "hello").unwrap();
        assert!(matches!(
            probe(&not_media).await,
            Err(AppError::Validation { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
