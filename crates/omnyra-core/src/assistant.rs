//! Optional LLM help for the text assistant (step 5b): titles, a description and hashtags
//! from what the user typed, through the Claude Messages API with the user's own API key.
//!
//! - The key lives in the OS keyring (`omnyra/llm/anthropic`, H12), never in SQLite.
//! - Only the fields of [`AssistRequest`] are sent; the UI shows them before sending.
//! - Raw HTTP (there is no official Rust SDK): `POST /v1/messages`, `anthropic-version:
//!   2026-10-21`, structured output (`output_config.format` JSON schema), low effort (a
//!   short, simple task), server-side refusal fallback (`fallbacks: "default"`).

use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

pub const KEY_SECRET: &str = "llm/anthropic";
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
const API_BASE: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_INPUT_CHARS: usize = 4000;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssistRequest {
    /// youtube | tiktok | instagram | twitch
    pub platform: String,
    /// What the video is about, as the user wrote it.
    pub topic: String,
    /// Extra notes (game, series, tone...). Optional.
    pub notes: String,
    /// `es` | `en`: language of the suggestions.
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistResult {
    pub titles: Vec<String>,
    pub description: String,
    pub hashtags: Vec<String>,
}

fn platform_rules(platform: &str) -> &'static str {
    match platform {
        "youtube" => "YouTube: titles up to 100 characters (best under 60), description up to 5000 bytes with the key info in the first two lines, 3-5 hashtags.",
        "tiktok" => "TikTok: no title, the caption is everything; keep it short and punchy, 3-5 hashtags.",
        "instagram" => "Instagram Reels: no title, caption up to 2200 characters with a hook in the first line, 5-10 hashtags (max 30).",
        "twitch" => "Twitch: stream title up to 140 characters, short and descriptive; hashtags are not used much (0-3 tags).",
        _ => "Generic social network.",
    }
}

fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "titles": { "type": "array", "items": { "type": "string" } },
            "description": { "type": "string" },
            "hashtags": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["titles", "description", "hashtags"],
        "additionalProperties": false
    })
}

/// Request body (pure, tested).
pub fn body(req: &AssistRequest, model: &str) -> Result<serde_json::Value> {
    let topic = req.topic.trim();
    if topic.is_empty() || topic.chars().count() > MAX_INPUT_CHARS {
        return Err(AppError::Validation {
            field: "topic".into(),
        });
    }
    if req.notes.chars().count() > MAX_INPUT_CHARS {
        return Err(AppError::Validation {
            field: "notes".into(),
        });
    }
    let language = if req.language == "en" {
        "English"
    } else {
        "Spanish (Spain)"
    };
    let system = format!(
        "You help a content creator write metadata for a video. Write in {language}. \
         Platform rules: {} Give 5 different title options (for platforms without titles, \
         5 options for the caption's first line), one description or caption, and hashtags \
         without the # sign. Do not invent facts that are not in the user's notes. No \
         clickbait that the video cannot deliver.",
        platform_rules(&req.platform)
    );
    let mut user = format!("Topic: {topic}");
    if !req.notes.trim().is_empty() {
        user.push_str(&format!("\nNotes: {}", req.notes.trim()));
    }
    Ok(json!({
        "model": model,
        "max_tokens": 16000,
        "system": system,
        "messages": [{ "role": "user", "content": user }],
        "output_config": {
            "effort": "low",
            "format": { "type": "json_schema", "schema": schema() }
        },
        "fallbacks": "default"
    }))
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    content: Vec<Block>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// Parses a Messages API response into suggestions (pure, tested).
pub fn parse(json_body: &str) -> Result<AssistResult> {
    let r: Response = serde_json::from_str(json_body).map_err(|_| AppError::Internal)?;
    match r.stop_reason.as_deref() {
        Some("refusal") => return Err(AppError::LlmRefused),
        // Truncated JSON would not parse: report it as a failure to retry.
        Some("max_tokens") => return Err(AppError::Network),
        _ => {}
    }
    let text = r
        .content
        .iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text.as_deref())
        .collect::<String>();
    let mut out: AssistResult = serde_json::from_str(&text).map_err(|_| AppError::Internal)?;
    out.titles.retain(|t| !t.trim().is_empty());
    out.hashtags = out
        .hashtags
        .iter()
        .map(|h| h.trim().trim_start_matches('#').replace(' ', ""))
        .filter(|h| !h.is_empty())
        .collect();
    out.hashtags.dedup();
    Ok(out)
}

/// Calls the API. `api_base` is injectable for tests.
pub async fn generate(
    http: &reqwest::Client,
    api_base: Option<&str>,
    api_key: &str,
    model: &str,
    req: &AssistRequest,
) -> Result<AssistResult> {
    let body = body(req, model)?;
    let resp = http
        .post(format!("{}/v1/messages", api_base.unwrap_or(API_BASE)))
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("anthropic-beta", FALLBACK_BETA)
        .header("content-type", "application/json")
        .timeout(std::time::Duration::from_secs(180))
        .json(&body)
        .send()
        .await
        .map_err(|_| AppError::Network)?;
    let status = resp.status().as_u16();
    let retry_after = crate::auth::oauth::retry_after(&resp);
    let text = resp.text().await.map_err(|_| AppError::Network)?;
    match status {
        200 => parse(&text),
        401 | 403 => Err(AppError::LlmKeyInvalid),
        429 => Err(AppError::RateLimited {
            retry_after_s: retry_after.unwrap_or(30),
        }),
        500..=599 => Err(AppError::Network),
        _ => {
            // The error type is safe to log; the body may echo the prompt.
            let kind = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"]["type"].as_str().map(str::to_owned));
            tracing::warn!(status, ?kind, "LLM request rejected");
            Err(AppError::Internal)
        }
    }
}

/// A key looks like one (cheap check before storing; the API has the last word).
pub fn plausible_key(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("sk-ant-") && k.len() > 20 && !k.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> AssistRequest {
        AssistRequest {
            platform: "youtube".into(),
            topic: "Speedrun de Minecraft en hardcore".into(),
            notes: "Serie: Hardcore #12".into(),
            language: "es".into(),
        }
    }

    #[test]
    fn body_has_schema_effort_fallback_and_only_user_fields() {
        let b = body(&req(), DEFAULT_MODEL).unwrap();
        assert_eq!(b["model"], "claude-opus-5-5");
        assert_eq!(b["output_config"]["effort"], "low");
        assert_eq!(b["output_config"]["format"]["type"], "json_schema");
        assert_eq!(b["fallbacks"], "default");
        assert!(b.get("temperature").is_none());
        let user = b["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("Speedrun") && user.contains("Hardcore #12"));
        assert!(b["system"].as_str().unwrap().contains("Spanish"));
        let mut empty = req();
        empty.topic = "  ".into();
        assert!(body(&empty, DEFAULT_MODEL).is_err());
    }

    #[test]
    fn parses_structured_output_and_cleans_hashtags() {
        let inner = r##"{"titles":["A","",  "B"],"description":"D","hashtags":["#mine craft","speedrun","speedrun"]}"##;
        let resp = json!({"content":[{"type":"thinking","thinking":""},{"type":"text","text":inner}],"stop_reason":"end_turn"});
        let r = parse(&resp.to_string()).unwrap();
        assert_eq!(r.titles, vec!["A", "B"]);
        assert_eq!(r.hashtags, vec!["minecraft", "speedrun"]);
    }

    #[test]
    fn refusal_and_truncation_are_errors() {
        let refused = json!({"content":[],"stop_reason":"refusal"});
        assert!(matches!(
            parse(&refused.to_string()),
            Err(AppError::LlmRefused)
        ));
        let cut = json!({"content":[{"type":"text","text":"{\"tit"}],"stop_reason":"max_tokens"});
        assert!(matches!(parse(&cut.to_string()), Err(AppError::Network)));
    }

    #[tokio::test]
    async fn sends_headers_and_maps_errors() {
        let mut srv = mockito::Server::new_async().await;
        let ok = srv
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "sk-ant-test-key-123456789")
            .match_header("anthropic-version", "2026-10-21")
            .match_header("anthropic-beta", "server-side-fallback-2026-07-01")
            .with_body(
                json!({"content":[{"type":"text","text":"{\"titles\":[\"T\"],\"description\":\"D\",\"hashtags\":[]}"}],"stop_reason":"end_turn"}).to_string(),
            )
            .create_async()
            .await;
        let http = reqwest::Client::new();
        let r = generate(
            &http,
            Some(&srv.url()),
            "sk-ant-test-key-123456789",
            DEFAULT_MODEL,
            &req(),
        )
        .await
        .unwrap();
        assert_eq!(r.titles, vec!["T"]);
        ok.assert_async().await;

        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"type":"error","error":{"type":"authentication_error"}}"#)
            .create_async()
            .await;
        assert!(matches!(
            generate(&http, Some(&srv.url()), "sk-ant-bad", DEFAULT_MODEL, &req()).await,
            Err(AppError::LlmKeyInvalid)
        ));
    }

    #[test]
    fn key_shape() {
        assert!(plausible_key("sk-ant-api03-abcdefghijklmnop"));
        assert!(!plausible_key("sk-ant-x"));
        assert!(!plausible_key("hello world key that is long"));
    }
}
