//! Shorts detection. The Data API has no "is Short" flag, so we ask youtube.com:
//! `HEAD /shorts/<id>` answers 200 for a Short and redirects (303 to `/watch?v=<id>`) for a
//! regular video. The request carries no token, and is made once per video. Its only cookie
//! is `SOCS=CAI` ("reject all" on the EU consent screen): without it, requests from the EU
//! are redirected to consent.youtube.com and nothing could be classified.
//! It is not an official API: anything unexpected yields `None` ("unknown"), and the video
//! is checked again on the next sync.

use super::super::kind;
use std::time::Duration;
use tokio::task::JoinSet;

/// Parallel requests to youtube.com.
const CONCURRENCY: usize = 4;

/// "Reject all" answer to the EU cookie consent screen. Carries no identity.
const CONSENT_COOKIE: &str = "SOCS=CAI";

/// Client without redirects or cookie store, so the raw answer is visible.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Omnyra/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("HTTP client")
}

/// `Some(kind)` when youtube.com gave a clear answer.
pub async fn check(http: &reqwest::Client, base: &str, id: &str) -> Option<&'static str> {
    let req = http
        .head(format!("{base}/shorts/{id}"))
        .header(reqwest::header::COOKIE, CONSENT_COOKIE);
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "Shorts check failed");
            return None;
        }
    };
    let status = resp.status();
    if status == reqwest::StatusCode::OK {
        return Some(kind::SHORT);
    }
    if status.is_redirection() {
        let to_watch = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|l| l.to_str().ok())
            .is_some_and(|l| l.contains("/watch"));
        if to_watch {
            return Some(kind::VIDEO);
        }
    }
    // Consent pages, 404 for private videos, rate limiting...: unknown for now.
    tracing::debug!(%status, "Shorts check inconclusive");
    None
}

/// Checks `ids` with bounded concurrency; returns the settled ones.
pub async fn classify(http: &reqwest::Client, base: &str, ids: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for chunk in ids.chunks(CONCURRENCY) {
        let mut set = JoinSet::new();
        for id in chunk {
            let (http, base, id) = (http.clone(), base.to_owned(), id.clone());
            set.spawn(async move {
                let k = check(&http, &base, &id).await;
                (id, k)
            });
        }
        while let Some(res) = set.join_next().await {
            if let Ok((id, Some(k))) = res {
                out.push((id, k.to_owned()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classifies_by_status_and_location() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("HEAD", "/shorts/short1")
            .match_header("cookie", CONSENT_COOKIE)
            .with_status(200)
            .create_async()
            .await;
        srv.mock("HEAD", "/shorts/video1")
            .with_status(303)
            .with_header("location", "https://www.youtube.com/watch?v=video1")
            .create_async()
            .await;
        srv.mock("HEAD", "/shorts/consent")
            .with_status(302)
            .with_header("location", "https://consent.youtube.com/m?continue=x")
            .create_async()
            .await;
        srv.mock("HEAD", "/shorts/private")
            .with_status(404)
            .create_async()
            .await;
        let ids: Vec<String> = ["short1", "video1", "consent", "private"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut got = classify(&client(), &srv.url(), &ids).await;
        got.sort();
        assert_eq!(
            got,
            vec![
                ("short1".to_string(), "short".to_string()),
                ("video1".to_string(), "video".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn network_error_is_unknown() {
        // Nothing listens on port 9 (discard).
        assert_eq!(check(&client(), "http://127.0.0.1:9", "x").await, None);
    }
}
