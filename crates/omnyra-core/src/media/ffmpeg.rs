//! FFmpeg renditions for multi-network publishing (step 3c): reframe to 16:9, 9:16 or
//! 1:1 (crop or pad), trim, and compress to fit a size limit.
//!
//! Argument building is pure (and tested); `render` runs the binary (`OMNYRA_FFMPEG` or
//! `ffmpeg` on the PATH) and reports progress from `-progress pipe:1`.

use super::MediaInfo;
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Aspect {
    /// Keep the source frame (only trim/compress).
    Original,
    Landscape,
    Vertical,
    Square,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// Fill the frame and cut what overflows (centre crop).
    Crop,
    /// Fit the whole frame and fill the rest with black bars.
    Pad,
    /// Fit the whole frame over a blurred, zoomed copy of itself.
    Blur,
}

/// How to adapt one video for one network.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenditionSpec {
    pub aspect: Aspect,
    pub fit: Fit,
    /// Start of the kept part, in seconds.
    pub start_s: Option<f64>,
    /// End of the kept part, in seconds (exclusive).
    pub end_s: Option<f64>,
    /// Longest duration the network accepts: the end is cut there.
    pub max_duration_s: Option<f64>,
    /// Size limit: the video bitrate is capped so the file fits (with a 5 % margin).
    pub max_size_mb: Option<f64>,
}

impl RenditionSpec {
    /// True when the source can be uploaded as-is.
    pub fn is_noop(&self) -> bool {
        self.aspect == Aspect::Original
            && self.start_s.unwrap_or(0.0) <= 0.0
            && self.end_s.is_none()
            && self.max_duration_s.is_none()
            && self.max_size_mb.is_none()
    }
}

/// Default adaptation per network: Instagram and TikTok want vertical video.
pub fn preset(platform: &str) -> RenditionSpec {
    let (aspect, max_duration_s, max_size_mb) = match platform {
        "instagram" => (Aspect::Vertical, Some(15.0 * 60.0), Some(300.0)),
        "tiktok" => (Aspect::Vertical, Some(10.0 * 60.0), Some(4096.0)),
        _ => (Aspect::Original, None, None),
    };
    RenditionSpec {
        aspect,
        fit: Fit::Crop,
        start_s: None,
        end_s: None,
        max_duration_s,
        max_size_mb,
    }
}

/// Output frame for an aspect, never upscaling past 1080 on the short side.
pub fn target_size(aspect: Aspect, src: Option<(u32, u32)>) -> Option<(u32, u32)> {
    let short = src.map(|(w, h)| w.min(h).min(1080)).unwrap_or(1080);
    let short = short - short % 2;
    match aspect {
        Aspect::Original => None,
        Aspect::Landscape => Some((even(short as f64 * 16.0 / 9.0), short)),
        Aspect::Vertical => Some((short, even(short as f64 * 16.0 / 9.0))),
        Aspect::Square => Some((short, short)),
    }
}

fn even(x: f64) -> u32 {
    let v = x.round() as u32;
    v - v % 2
}

/// `[start, duration)` actually kept, given the source duration.
pub fn kept_range(spec: &RenditionSpec, source_duration: Option<f64>) -> (f64, Option<f64>) {
    let start = spec.start_s.unwrap_or(0.0).max(0.0);
    let end = match (spec.end_s, source_duration) {
        (Some(e), Some(d)) => Some(e.min(d)),
        (Some(e), None) => Some(e),
        (None, d) => d,
    };
    let mut duration = end.map(|e| (e - start).max(0.0));
    if let Some(max) = spec.max_duration_s {
        duration = Some(duration.map_or(max, |d| d.min(max)));
    }
    (start, duration)
}

/// Audio bitrate of every rendition (kbit/s).
const AUDIO_KBPS: f64 = 128.0;

/// Video bitrate (kbit/s) that keeps the file under `max_mb` for `duration_s`, or `None`
/// when quality-based encoding (CRF) already fits comfortably.
pub fn video_kbps_cap(max_mb: f64, duration_s: f64) -> Option<f64> {
    if duration_s <= 0.0 {
        return None;
    }
    let total_kbps = max_mb * 8.0 * 1024.0 * 0.95 / duration_s;
    let video = total_kbps - AUDIO_KBPS;
    // CRF 23 at 1080p rarely exceeds ~8 Mbit/s: no cap needed above that.
    if video >= 8000.0 {
        None
    } else {
        Some(video.max(300.0))
    }
}

fn filter(aspect: Aspect, fit: Fit, size: (u32, u32)) -> String {
    let (w, h) = size;
    match fit {
        Fit::Crop => format!(
            "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1"
        ),
        Fit::Pad => format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1"
        ),
        Fit::Blur => {
            let _ = aspect;
            format!(
                "split=2[bg][fg];[bg]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},boxblur=20:2[b];[fg]scale={w}:{h}:force_original_aspect_ratio=decrease[f];[b][f]overlay=(W-w)/2:(H-h)/2,setsar=1"
            )
        }
    }
}

/// FFmpeg arguments (without the binary) for `spec`.
pub fn build_args(
    input: &Path,
    output: &Path,
    spec: &RenditionSpec,
    media: Option<&MediaInfo>,
) -> Vec<String> {
    let src = media.and_then(|m| Some((m.width?, m.height?)));
    let (start, duration) = kept_range(spec, media.and_then(|m| m.duration_s));
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-nostats".into(), "-y".into()];
    if start > 0.0 {
        a.extend(["-ss".into(), format!("{start:.3}")]);
    }
    a.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    if let Some(d) = duration {
        a.extend(["-t".into(), format!("{d:.3}")]);
    }
    if let Some(size) = target_size(spec.aspect, src) {
        let f = filter(spec.aspect, spec.fit, size);
        if spec.fit == Fit::Blur {
            a.extend(["-filter_complex".into(), f]);
        } else {
            a.extend(["-vf".into(), f]);
        }
    }
    a.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
        ]
        .map(String::from),
    );
    match (spec.max_size_mb, duration) {
        (Some(mb), Some(d)) => match video_kbps_cap(mb, d) {
            Some(k) => a.extend([
                "-b:v".into(),
                format!("{}k", k.round()),
                "-maxrate".into(),
                format!("{}k", k.round()),
                "-bufsize".into(),
                format!("{}k", (k * 2.0).round()),
            ]),
            None => a.extend(["-crf".into(), "23".into()]),
        },
        _ => a.extend(["-crf".into(), "23".into()]),
    }
    a.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ar",
            "48000",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
        ]
        .map(String::from),
    );
    a.push(output.to_string_lossy().into_owned());
    a
}

/// Facts the rendition will have, to validate it against the network's rules before
/// spending minutes rendering it.
pub fn projected_media(spec: &RenditionSpec, media: &MediaInfo) -> MediaInfo {
    let mut m = media.clone();
    if let Some((w, h)) = target_size(spec.aspect, media.width.zip(media.height)) {
        m.width = Some(w);
        m.height = Some(h);
    }
    if let (_, Some(d)) = kept_range(spec, media.duration_s) {
        m.duration_s = Some(d);
    }
    m.video_codec = m.video_codec.map(|_| "h264".into());
    m
}

pub fn ffmpeg_bin() -> std::ffi::OsString {
    std::env::var_os("OMNYRA_FFMPEG").unwrap_or_else(|| "ffmpeg".into())
}

/// Parses `out_time_us=` / `out_time_ms=` (both microseconds) from `-progress` output.
pub fn progress_seconds(line: &str) -> Option<f64> {
    let v = line
        .strip_prefix("out_time_us=")
        .or_else(|| line.strip_prefix("out_time_ms="))?;
    v.trim().parse::<f64>().ok().map(|us| us / 1_000_000.0)
}

/// Renders `spec` from `input` to `output`, calling `progress` with 0..=1.
/// `NotSupported` without FFmpeg; `Validation{file}` when FFmpeg fails on the file.
pub async fn render(
    input: &Path,
    output: &Path,
    spec: &RenditionSpec,
    media: Option<&MediaInfo>,
    progress: impl Fn(f64),
) -> Result<()> {
    let args = build_args(input, output, spec, media);
    let total = kept_range(spec, media.and_then(|m| m.duration_s)).1;
    let mut cmd = tokio::process::Command::new(ffmpeg_bin());
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| {
        tracing::warn!(error = %e, "ffmpeg not available");
        AppError::NotSupported
    })?;
    let stdout = child.stdout.take().ok_or(AppError::Internal)?;
    let mut stderr = child.stderr.take().ok_or(AppError::Internal)?;
    // Drain stderr so FFmpeg never blocks on a full pipe; keep the tail for the log.
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut buf).await;
        let s = String::from_utf8_lossy(&buf).into_owned();
        s.chars()
            .rev()
            .take(600)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    });
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let (Some(t), Some(total)) = (progress_seconds(&line), total) {
            if total > 0.0 {
                progress((t / total).clamp(0.0, 1.0));
            }
        }
    }
    let status = child.wait().await.map_err(|_| AppError::Internal)?;
    let tail = err_task.await.unwrap_or_default();
    if !status.success() {
        tracing::warn!(stderr = %tail, "ffmpeg failed");
        let _ = tokio::fs::remove_file(output).await;
        return Err(AppError::Validation {
            field: "file".into(),
        });
    }
    progress(1.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(w: u32, h: u32, d: f64) -> MediaInfo {
        MediaInfo {
            duration_s: Some(d),
            width: Some(w),
            height: Some(h),
            video_codec: Some("hevc".into()),
            audio_codec: Some("opus".into()),
            container: None,
        }
    }

    fn spec(aspect: Aspect, fit: Fit) -> RenditionSpec {
        RenditionSpec {
            aspect,
            fit,
            start_s: None,
            end_s: None,
            max_duration_s: None,
            max_size_mb: None,
        }
    }

    fn arg_after<'a>(a: &'a [String], flag: &str) -> Option<&'a str> {
        a.iter()
            .position(|x| x == flag)
            .and_then(|i| a.get(i + 1))
            .map(String::as_str)
    }

    #[test]
    fn target_sizes_never_upscale_and_stay_even() {
        assert_eq!(
            target_size(Aspect::Vertical, Some((1920, 1080))),
            Some((1080, 1920))
        );
        assert_eq!(
            target_size(Aspect::Landscape, Some((1080, 1920))),
            Some((1920, 1080))
        );
        assert_eq!(
            target_size(Aspect::Square, Some((3840, 2160))),
            Some((1080, 1080))
        );
        // 720p source: 720 short side, no upscale to 1080.
        assert_eq!(
            target_size(Aspect::Vertical, Some((1280, 720))),
            Some((720, 1280))
        );
        // Odd sizes are rounded down to even numbers (yuv420p needs them).
        assert_eq!(
            target_size(Aspect::Square, Some((641, 481))),
            Some((480, 480))
        );
        assert_eq!(target_size(Aspect::Original, Some((10, 10))), None);
        assert_eq!(target_size(Aspect::Vertical, None), Some((1080, 1920)));
    }

    #[test]
    fn crop_pad_and_blur_filters() {
        let m = media(1920, 1080, 60.0);
        let out = Path::new("/o.mp4");
        let a = build_args(
            Path::new("/i.mp4"),
            out,
            &spec(Aspect::Vertical, Fit::Crop),
            Some(&m),
        );
        assert_eq!(
            arg_after(&a, "-vf"),
            Some("scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,setsar=1")
        );
        let a = build_args(
            Path::new("/i.mp4"),
            out,
            &spec(Aspect::Square, Fit::Pad),
            Some(&m),
        );
        assert!(arg_after(&a, "-vf").unwrap().contains("pad=1080:1080"));
        let a = build_args(
            Path::new("/i.mp4"),
            out,
            &spec(Aspect::Vertical, Fit::Blur),
            Some(&m),
        );
        assert!(arg_after(&a, "-filter_complex")
            .unwrap()
            .contains("boxblur"));
        assert!(arg_after(&a, "-vf").is_none());
        let a = build_args(
            Path::new("/i.mp4"),
            out,
            &spec(Aspect::Original, Fit::Crop),
            Some(&m),
        );
        assert!(arg_after(&a, "-vf").is_none());
        assert_eq!(a.last().map(String::as_str), Some("/o.mp4"));
        assert_eq!(arg_after(&a, "-c:v"), Some("libx264"));
        assert_eq!(arg_after(&a, "-progress"), Some("pipe:1"));
    }

    #[test]
    fn trimming_respects_start_end_and_network_limit() {
        let mut s = spec(Aspect::Original, Fit::Crop);
        s.start_s = Some(10.0);
        s.end_s = Some(100.0);
        assert_eq!(kept_range(&s, Some(60.0)), (10.0, Some(50.0)));
        s.max_duration_s = Some(30.0);
        assert_eq!(kept_range(&s, Some(600.0)), (10.0, Some(30.0)));
        let a = build_args(
            Path::new("/i"),
            Path::new("/o"),
            &s,
            Some(&media(1920, 1080, 600.0)),
        );
        assert_eq!(arg_after(&a, "-ss"), Some("10.000"));
        assert_eq!(arg_after(&a, "-t"), Some("30.000"));
        // -ss before -i: fast seek.
        assert!(a.iter().position(|x| x == "-ss") < a.iter().position(|x| x == "-i"));
        let s = spec(Aspect::Original, Fit::Crop);
        assert_eq!(kept_range(&s, Some(42.0)), (0.0, Some(42.0)));
        assert_eq!(kept_range(&s, None), (0.0, None));
    }

    #[test]
    fn bitrate_cap_fits_the_size_limit() {
        // 300 MB over 15 min: ~2.6 Mbit/s total.
        let k = video_kbps_cap(300.0, 900.0).unwrap();
        let total_mb = (k + AUDIO_KBPS) * 900.0 / 8.0 / 1024.0;
        assert!(total_mb < 300.0 && total_mb > 270.0, "{total_mb}");
        // Plenty of room: quality mode.
        assert_eq!(video_kbps_cap(4096.0, 60.0), None);
        // Absurd limits still give a watchable floor.
        assert_eq!(video_kbps_cap(1.0, 3600.0), Some(300.0));
        let mut s = preset("instagram");
        s.aspect = Aspect::Original;
        let a = build_args(
            Path::new("/i"),
            Path::new("/o"),
            &s,
            Some(&media(1920, 1080, 900.0)),
        );
        assert!(arg_after(&a, "-b:v").is_some());
        assert!(arg_after(&a, "-crf").is_none());
    }

    #[test]
    fn presets_and_noop() {
        assert_eq!(preset("tiktok").aspect, Aspect::Vertical);
        assert!(preset("youtube").is_noop());
        assert!(!preset("instagram").is_noop());
    }

    #[test]
    fn projected_media_reflects_the_rendition() {
        let mut s = preset("instagram");
        s.max_duration_s = Some(30.0);
        let p = projected_media(&s, &media(1920, 1080, 120.0));
        assert_eq!(
            (p.width, p.height, p.duration_s),
            (Some(1080), Some(1920), Some(30.0))
        );
        assert_eq!(p.video_codec.as_deref(), Some("h264"));
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(progress_seconds("out_time_us=1500000"), Some(1.5));
        assert_eq!(progress_seconds("out_time_ms=2000000"), Some(2.0));
        assert_eq!(progress_seconds("frame=10"), None);
        assert_eq!(progress_seconds("out_time_us=N/A"), None);
    }

    /// Real FFmpeg: renders a 2 s test pattern to vertical.
    #[tokio::test]
    #[ignore = "needs ffmpeg installed"]
    async fn renders_with_real_ffmpeg() {
        let dir = std::env::temp_dir().join(format!("omnyra-ff-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4");
        let ok = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=640x360:rate=25:duration=2",
            ])
            .args(["-f", "lavfi", "-i", "sine=duration=2", "-shortest", "-y"])
            .arg(&src)
            .status()
            .unwrap();
        assert!(ok.success());
        let out = dir.join("out.mp4");
        let m = crate::media::probe(&src).await.unwrap();
        let last = std::sync::Mutex::new(0.0);
        render(
            &src,
            &out,
            &spec(Aspect::Vertical, Fit::Blur),
            Some(&m),
            |p| *last.lock().unwrap() = p,
        )
        .await
        .unwrap();
        assert_eq!(*last.lock().unwrap(), 1.0);
        let o = crate::media::probe(&out).await.unwrap();
        assert_eq!((o.width, o.height), (Some(360), Some(640)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
