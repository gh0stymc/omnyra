//! Shared upload rules for short-form networks (Instagram Reels, TikTok): the caption is
//! the text that goes with the video, there is no separate title on the platform.

use super::{FileFacts, Issue, UploadDraft};

pub struct ShortFormRules {
    pub extensions: &'static [&'static str],
    pub max_size_bytes: u64,
    pub min_duration_s: f64,
    pub max_duration_s: f64,
    pub max_caption_chars: usize,
    pub max_hashtags: usize,
}

/// The caption sent to the platform: title, a blank line and the description (either may
/// be empty), plus the tags as hashtags when they are not already in the text.
pub fn caption(draft: &UploadDraft) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !draft.title.trim().is_empty() {
        parts.push(draft.title.trim().to_owned());
    }
    if !draft.description.trim().is_empty() {
        parts.push(draft.description.trim().to_owned());
    }
    let mut text = parts.join("\n\n");
    let tags: Vec<String> = draft
        .tags
        .iter()
        .map(|t| t.trim().trim_start_matches('#').replace(' ', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("#{t}"))
        .filter(|t| !text.to_lowercase().contains(&t.to_lowercase()))
        .collect();
    if !tags.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&tags.join(" "));
    }
    text
}

pub fn count_hashtags(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.len() > 1 && w.starts_with('#'))
        .count()
}

pub fn check(rules: &ShortFormRules, draft: &UploadDraft, f: &FileFacts) -> Vec<Issue> {
    let mut out = Vec::new();
    if !rules.extensions.contains(&f.extension.as_str()) {
        out.push(Issue::error("file", "format"));
    }
    if f.size_bytes == 0 {
        out.push(Issue::error("file", "empty"));
    } else if f.size_bytes > rules.max_size_bytes {
        out.push(Issue::error("file", "too_big").with_limit(rules.max_size_bytes as f64));
    }
    match &f.media {
        None => out.push(Issue::warning("file", "probe_unavailable")),
        Some(m) if !m.has_video() => out.push(Issue::error("file", "no_video")),
        Some(m) => {
            if let Some(d) = m.duration_s {
                if d > rules.max_duration_s {
                    out.push(Issue::error("file", "too_long").with_limit(rules.max_duration_s));
                } else if d < rules.min_duration_s {
                    out.push(Issue::error("file", "too_short").with_limit(rules.min_duration_s));
                }
            }
            if let (Some(w), Some(h)) = (m.width, m.height) {
                if w >= h {
                    // Accepted, but shown letterboxed: suggest the 9:16 rendition (3c).
                    out.push(Issue::warning("file", "not_vertical"));
                }
            }
        }
    }
    let text = caption(draft);
    if text.trim().is_empty() {
        out.push(Issue::error("title", "required"));
    }
    if text.chars().count() > rules.max_caption_chars {
        out.push(
            Issue::error("description", "caption_too_long")
                .with_limit(rules.max_caption_chars as f64),
        );
    }
    if count_hashtags(&text) > rules.max_hashtags {
        out.push(
            Issue::error("description", "too_many_hashtags").with_limit(rules.max_hashtags as f64),
        );
    }
    if draft.thumbnail_path.is_some() {
        out.push(Issue::warning("thumbnail", "ignored"));
    }
    if draft.playlist_id.is_some() {
        out.push(Issue::warning("playlist", "ignored"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaInfo;

    fn draft(title: &str, desc: &str, tags: &[&str]) -> UploadDraft {
        UploadDraft {
            file_path: "/v.mp4".into(),
            title: title.into(),
            description: desc.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            visibility: "public".into(),
            category_id: None,
            playlist_id: None,
            thumbnail_path: None,
            made_for_kids: false,
        }
    }

    const RULES: ShortFormRules = ShortFormRules {
        extensions: &["mp4", "mov"],
        max_size_bytes: 1000,
        min_duration_s: 3.0,
        max_duration_s: 60.0,
        max_caption_chars: 30,
        max_hashtags: 2,
    };

    fn facts(w: u32, h: u32, d: f64) -> FileFacts {
        FileFacts {
            size_bytes: 10,
            extension: "mp4".into(),
            media: Some(MediaInfo {
                duration_s: Some(d),
                width: Some(w),
                height: Some(h),
                video_codec: Some("h264".into()),
                audio_codec: None,
                container: None,
            }),
            thumbnail: None,
        }
    }

    #[test]
    fn caption_joins_text_and_adds_missing_hashtags() {
        assert_eq!(
            caption(&draft("Hola", "texto #mc", &["mc", "speed run"])),
            "Hola\n\ntexto #mc\n\n#speedrun"
        );
        assert_eq!(caption(&draft("", "", &["#a"])), "#a");
    }

    #[test]
    fn flags_long_caption_hashtags_duration_and_orientation() {
        let codes = |v: Vec<Issue>| v.into_iter().map(|i| i.code).collect::<Vec<_>>();
        assert!(codes(check(
            &RULES,
            &draft("ok", "", &[]),
            &facts(1080, 1920, 10.0)
        ))
        .is_empty());
        let c = codes(check(
            &RULES,
            &draft(
                "a very long caption that goes past the limit",
                "#a #b #c",
                &[],
            ),
            &facts(1920, 1080, 61.0),
        ));
        assert!(c.contains(&"caption_too_long".to_string()));
        assert!(c.contains(&"too_many_hashtags".to_string()));
        assert!(c.contains(&"too_long".to_string()));
        assert!(c.contains(&"not_vertical".to_string()));
        assert!(
            codes(check(&RULES, &draft("x", "", &[]), &facts(9, 16, 1.0)))
                .contains(&"too_short".to_string())
        );
    }
}
