//! Checks for file paths that come from the frontend (REVIEW S1/S2). They normally come
//! from a native file dialog, but the webview is not trusted blindly: an injected script
//! must not be able to write arbitrary files or overwrite Omnyra's own data.

use crate::{AppError, Result};
use std::path::{Path, PathBuf};

fn invalid() -> AppError {
    AppError::Validation {
        field: "path".into(),
    }
}

/// A destination for an export: absolute, with one of `exts` (lower case, no dot), in an
/// existing directory, and outside `protected` (the app data dir).
pub fn check_output(dest: &str, exts: &[&str], protected: &Path) -> Result<PathBuf> {
    let p = Path::new(dest);
    if !p.is_absolute() {
        return Err(invalid());
    }
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .ok_or_else(invalid)?;
    if !exts.contains(&ext.as_str()) {
        return Err(invalid());
    }
    let parent = p.parent().ok_or_else(invalid)?;
    let parent = parent
        .canonicalize()
        .map_err(|_| AppError::FileUnavailable)?;
    if let Ok(prot) = protected.canonicalize() {
        if parent.starts_with(&prot) {
            return Err(invalid());
        }
    }
    let name = p.file_name().ok_or_else(invalid)?;
    let out = parent.join(name);
    if out.is_dir() {
        return Err(invalid());
    }
    Ok(out)
}

/// Image type from its first bytes: `png`, `jpeg`, `gif` or `webp`.
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths() {
        let base = std::env::temp_dir().join(format!("omnyra-paths-{}", uuid::Uuid::now_v7()));
        let data = base.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let ok = base.join("kit.HTML");
        assert_eq!(
            check_output(&ok.to_string_lossy(), &["html"], &data).unwrap(),
            base.canonicalize().unwrap().join("kit.HTML")
        );
        assert!(check_output("relative.html", &["html"], &data).is_err());
        assert!(check_output(&base.join("x.sh").to_string_lossy(), &["html"], &data).is_err());
        assert!(
            check_output(
                &data.join("omnyra.html").to_string_lossy(),
                &["html"],
                &data
            )
            .is_err(),
            "inside app data"
        );
        assert!(
            check_output(
                &data.join("../data/x.html").to_string_lossy(),
                &["html"],
                &data
            )
            .is_err(),
            "dot-dot into app data"
        );
        assert!(matches!(
            check_output(
                &base.join("missing/x.html").to_string_lossy(),
                &["html"],
                &data
            ),
            Err(AppError::FileUnavailable)
        ));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn magic_bytes() {
        assert_eq!(image_kind(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(image_kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpeg"));
        assert_eq!(image_kind(b"GIF89a..."), Some("gif"));
        assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_kind(b"#!/bin/sh"), None);
        assert_eq!(image_kind(b""), None);
    }
}
