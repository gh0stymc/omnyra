//! Local media files: inspection with ffprobe (formats, duration, streams).

pub mod ffmpeg;
pub mod probe;

pub use probe::{probe, MediaInfo};
