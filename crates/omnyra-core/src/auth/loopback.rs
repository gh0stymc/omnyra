//! One-shot loopback HTTP server for the OAuth redirect (RFC 8252 §7.3).
//!
//! - Binds `127.0.0.1` on a random port (never `0.0.0.0`).
//! - Only a request to `/` carrying the expected `state` ends the wait. Anything else
//!   (favicon, wrong state from another local process...) gets an error page and the server
//!   keeps waiting, so a local process cannot abort the flow by sending junk.
//! - Requests are capped in size and time, and the response never echoes request input.

use super::pkce::ct_eq;
use crate::{AppError, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const PER_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LoopbackServer {
    listener: TcpListener,
    port: u16,
}

#[derive(Debug, PartialEq)]
enum Callback {
    Code(String),
    Denied,
    Error,
}

enum Request {
    /// Valid state: ends the wait.
    Callback(Callback),
    /// Wrong/missing state, wrong path or malformed: keep waiting.
    Ignored(u16),
}

impl LoopbackServer {
    pub async fn bind() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|_| AppError::Internal)?;
        let port = listener
            .local_addr()
            .map_err(|_| AppError::Internal)?
            .port();
        Ok(Self { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Redirect URI to register in the authorization request. Google accepts any port on
    /// the loopback IP for "Desktop app" clients.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    /// Waits until the browser is redirected back with the expected `state`, then returns the
    /// authorization code. The server is dropped (port closed) when this returns.
    pub async fn wait_for_code(self, expected_state: &str, timeout: Duration) -> Result<String> {
        let wait = async {
            loop {
                let Ok((stream, _)) = self.listener.accept().await else {
                    continue;
                };
                match tokio::time::timeout(PER_CONNECTION_TIMEOUT, handle(stream, expected_state))
                    .await
                {
                    Ok(Some(Callback::Code(code))) => return Ok(code),
                    Ok(Some(Callback::Denied)) => return Err(AppError::AuthCancelled),
                    Ok(Some(Callback::Error)) => return Err(AppError::AuthFailed),
                    Ok(None) | Err(_) => continue,
                }
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| AppError::AuthTimeout)?
    }
}

async fn handle(mut stream: TcpStream, expected_state: &str) -> Option<Callback> {
    let raw = read_head(&mut stream).await?;
    match parse_request(&raw, expected_state) {
        Request::Callback(cb) => {
            let ok = matches!(cb, Callback::Code(_));
            let _ = respond(&mut stream, 200, page(ok)).await;
            Some(cb)
        }
        Request::Ignored(status) => {
            let _ = respond(&mut stream, status, page(false)).await;
            None
        }
    }
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 || buf.len() + n > MAX_REQUEST_BYTES {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8(buf).ok()
}

fn parse_request(raw: &str, expected_state: &str) -> Request {
    let mut parts = raw.lines().next().unwrap_or("").split(' ');
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return Request::Ignored(405);
    };
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Request::Ignored(400);
    };
    if url.path() != "/" {
        return Request::Ignored(404);
    }
    let (mut code, mut state, mut error) = (None, None, None);
    for (k, v) in url.query_pairs() {
        match &*k {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    match state {
        Some(s) if ct_eq(&s, expected_state) => {}
        _ => return Request::Ignored(400),
    }
    Request::Callback(match (code, error.as_deref()) {
        (Some(c), None) if !c.is_empty() => Callback::Code(c),
        (_, Some("access_denied")) => Callback::Denied,
        _ => Callback::Error,
    })
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Bad Request",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

/// Static pages: nothing from the request is reflected.
fn page(ok: bool) -> &'static str {
    if ok {
        concat!(
            "<!doctype html><html><head><meta charset=utf-8><title>Omnyra</title>",
            "<style>body{font-family:system-ui,sans-serif;background:#0f1115;color:#e6e6e6;",
            "display:grid;place-items:center;height:100vh;margin:0}</style></head><body><div>",
            "<h1>Omnyra</h1><p>Cuenta conectada. Ya puedes cerrar esta pestaña y volver a la app.</p>",
            "<p>Account connected. You can close this tab and return to the app.</p>",
            "</div></body></html>"
        )
    } else {
        concat!(
            "<!doctype html><html><head><meta charset=utf-8><title>Omnyra</title>",
            "<style>body{font-family:system-ui,sans-serif;background:#0f1115;color:#e6e6e6;",
            "display:grid;place-items:center;height:100vh;margin:0}</style></head><body><div>",
            "<h1>Omnyra</h1><p>No se ha podido completar la conexión. Vuelve a la app.</p>",
            "<p>The connection could not be completed. Return to the app.</p>",
            "</div></body></html>"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get(port: u16, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    #[tokio::test]
    async fn returns_code_for_matching_state() {
        let srv = LoopbackServer::bind().await.unwrap();
        let port = srv.port();
        assert!(srv.redirect_uri().starts_with("http://127.0.0.1:"));
        let wait = tokio::spawn(srv.wait_for_code("st4te", Duration::from_secs(5)));
        let resp = get(port, "/?state=st4te&code=4%2Fabc&scope=x").await;
        assert!(resp.starts_with("HTTP/1.1 200"));
        assert_eq!(wait.await.unwrap().unwrap(), "4/abc");
    }

    #[tokio::test]
    async fn ignores_wrong_state_and_other_paths_then_accepts() {
        let srv = LoopbackServer::bind().await.unwrap();
        let port = srv.port();
        let wait = tokio::spawn(srv.wait_for_code("good", Duration::from_secs(5)));
        assert!(get(port, "/favicon.ico").await.starts_with("HTTP/1.1 404"));
        assert!(get(port, "/?state=evil&code=x")
            .await
            .starts_with("HTTP/1.1 400"));
        assert!(get(port, "/?code=x").await.starts_with("HTTP/1.1 400"));
        get(port, "/?state=good&code=real").await;
        assert_eq!(wait.await.unwrap().unwrap(), "real");
    }

    #[tokio::test]
    async fn access_denied_is_cancelled() {
        let srv = LoopbackServer::bind().await.unwrap();
        let port = srv.port();
        let wait = tokio::spawn(srv.wait_for_code("s", Duration::from_secs(5)));
        get(port, "/?state=s&error=access_denied").await;
        assert!(matches!(wait.await.unwrap(), Err(AppError::AuthCancelled)));
    }

    #[tokio::test]
    async fn other_errors_fail() {
        let srv = LoopbackServer::bind().await.unwrap();
        let port = srv.port();
        let wait = tokio::spawn(srv.wait_for_code("s", Duration::from_secs(5)));
        get(port, "/?state=s&error=invalid_scope").await;
        assert!(matches!(wait.await.unwrap(), Err(AppError::AuthFailed)));
    }

    #[tokio::test]
    async fn times_out() {
        let srv = LoopbackServer::bind().await.unwrap();
        let r = srv.wait_for_code("s", Duration::from_millis(50)).await;
        assert!(matches!(r, Err(AppError::AuthTimeout)));
    }

    #[tokio::test]
    async fn response_does_not_reflect_input() {
        let srv = LoopbackServer::bind().await.unwrap();
        let port = srv.port();
        let _wait = tokio::spawn(srv.wait_for_code("s", Duration::from_secs(5)));
        let resp = get(port, "/?state=%3Cscript%3E&code=x").await;
        assert!(!resp.contains("<script>"));
    }
}
