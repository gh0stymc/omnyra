//! Account lifecycle: connect (OAuth flow → profile → keyring → DB) and disconnect
//! (revoke → delete from keyring → mark disconnected). Generic over connectors: Twitch (2a)
//! reuses it unchanged.

use crate::auth::{loopback::LoopbackServer, oauth, pkce, AuthService};
use crate::clock::Clock;
use crate::connectors::http::{AccountTokens, ApiClient, NoToken, StaticToken};
use crate::connectors::{AccountCtx, Connector};
use crate::db::repo::accounts::{self, Account, AccountStatus, NewAccount};
use crate::db::repo::audit_log;
use crate::quota::QuotaGate;
use crate::{AppError, Result};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long we wait for the user to finish in the browser.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Internal)
}

/// Runs the whole OAuth flow for `connector` and returns the connected account.
///
/// `open_browser` receives the authorization URL. Dropping the returned future cancels the
/// flow (the loopback port is closed). Requests `base_scopes` plus `extra_scopes`
/// (incremental authorization, e.g. `youtube.upload` in 1d).
pub async fn connect(
    db: &Mutex<Connection>,
    auth: &AuthService,
    clock: &dyn Clock,
    connector: &dyn Connector,
    extra_scopes: &[String],
    timeout: Duration,
    open_browser: impl FnOnce(&str) -> Result<()> + Send,
) -> Result<Account> {
    let cfg = connector.oauth().ok_or(AppError::NotConfigured)?.clone();
    let platform = connector.info().id.clone();

    let mut scopes = cfg.base_scopes.clone();
    for s in extra_scopes {
        if !scopes.contains(s) {
            scopes.push(s.clone());
        }
    }

    let server = LoopbackServer::bind().await?;
    let redirect_uri = server.redirect_uri();
    let pkce = pkce::Pkce::new();
    let state = pkce::random_token();
    let url = oauth::authorization_url(&cfg, &redirect_uri, &scopes, &state, &pkce.challenge);
    open_browser(&url)?;

    let code = server.wait_for_code(&state, timeout).await?;
    let mut tokens = oauth::exchange_code(
        auth.http(),
        &cfg,
        &code,
        &pkce.verifier,
        &redirect_uri,
        &scopes,
        auth.now(),
    )
    .await?;

    // Google lets the user untick scopes on the consent screen.
    if !scopes.iter().all(|s| tokens.scopes.contains(s)) {
        return Err(AppError::ScopesDenied);
    }

    let ctx = AccountCtx::unmetered(
        "",
        "",
        ApiClient::new(
            auth.http().clone(),
            Arc::new(StaticToken(tokens.access_token.clone())),
        ),
    );
    let profile = match connector.fetch_profile(&ctx).await {
        Ok(p) => p,
        Err(AppError::NoChannel) => {
            // No account can be using this grant: don't leave it behind at the provider.
            let t = tokens
                .refresh_token
                .as_deref()
                .unwrap_or(&tokens.access_token);
            let _ = oauth::revoke(auth.http(), &cfg, t).await;
            return Err(AppError::NoChannel);
        }
        // Other failures: the tokens only live in memory and are dropped here.
        Err(e) => return Err(e),
    };

    let (id, existed) = {
        let conn = lock(db)?;
        let existed = accounts::find_by_external(&conn, &platform, &profile.external_id)?;
        let id = accounts::id_for(&conn, &platform, &profile.external_id)?;
        (id, existed.is_some())
    };

    if tokens.refresh_token.is_none() {
        if let Some(old) = auth.load(&id).await? {
            tokens.refresh_token = old.refresh_token;
        }
    }
    auth.save(&id, &tokens).await?;

    let now = clock.now_iso();
    let saved = (|| {
        let mut conn = lock(db)?;
        let tx = conn.transaction()?;
        let account = accounts::upsert(
            &tx,
            &id,
            &NewAccount {
                platform: &platform,
                external_id: &profile.external_id,
                handle: profile.handle.as_deref(),
                display_name: &profile.display_name,
                granted_scopes: &tokens.scopes,
            },
            &now,
        )?;
        audit_log::append(
            &tx,
            &now,
            "account.connect",
            Some(&id),
            &json!({ "platform": platform, "scopes": tokens.scopes }),
        )?;
        tx.commit()?;
        Ok::<_, AppError>(account)
    })();
    if saved.is_err() && !existed {
        let _ = auth.delete(&id).await;
    }
    saved
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectOutcome {
    /// False if the provider could not be reached to revoke the grant. The local tokens are
    /// deleted anyway; the UI then links to the provider's "third-party access" page.
    pub revoked: bool,
}

/// Revokes the grant (best effort), deletes the tokens from the keyring and marks the account
/// as disconnected. Historical data is kept.
pub async fn disconnect(
    db: &Mutex<Connection>,
    auth: &AuthService,
    clock: &dyn Clock,
    connector: Option<&dyn Connector>,
    account_id: &str,
) -> Result<DisconnectOutcome> {
    let account = accounts::get(&*lock(db)?, account_id)?;
    let mut revoked = true;
    if let Some(cfg) = connector.and_then(|c| c.oauth()) {
        if let Some(t) = auth.load(account_id).await? {
            let token = t.refresh_token.as_deref().unwrap_or(&t.access_token);
            revoked = oauth::revoke(auth.http(), cfg, token).await.is_ok();
        }
    }
    auth.delete(account_id).await?;

    let now = clock.now_iso();
    let mut conn = lock(db)?;
    let tx = conn.transaction()?;
    accounts::set_status(&tx, account_id, AccountStatus::Disconnected)?;
    audit_log::append(
        &tx,
        &now,
        "account.disconnect",
        Some(account_id),
        &json!({ "platform": account.platform, "revoked": revoked }),
    )?;
    tx.commit()?;
    Ok(DisconnectOutcome { revoked })
}

/// Context for API calls on behalf of a stored account, charging metered calls to `quota`.
pub fn account_ctx(
    auth: &Arc<AuthService>,
    connector: &dyn Connector,
    account: &Account,
    quota: Arc<dyn QuotaGate>,
) -> AccountCtx {
    let api = match connector.oauth() {
        Some(cfg) => ApiClient::new(
            auth.http().clone(),
            Arc::new(AccountTokens {
                auth: auth.clone(),
                account_id: account.id.clone(),
                config: cfg.clone(),
            }),
        ),
        None => ApiClient::new(auth.http().clone(), Arc::new(NoToken)),
    };
    AccountCtx {
        account_id: account.id.clone(),
        external_id: account.external_id.clone(),
        api,
        quota,
    }
}

/// If `err` means the account's credentials are no longer usable, mark it `needs_reauth`.
/// Returns true when the status changed (the caller then notifies the UI).
pub fn handle_auth_error(db: &Mutex<Connection>, account_id: &str, err: &AppError) -> bool {
    if !matches!(err, AppError::TokenExpired | AppError::NotConnected) {
        return false;
    }
    lock(db)
        .and_then(|c| accounts::set_status(&c, account_id, AccountStatus::NeedsReauth))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{MemoryStore, TokenStore};
    use crate::clock::FixedClock;
    use crate::connectors::youtube::{test_connector, SCOPE_ANALYTICS, SCOPE_READONLY};
    use crate::db::open_in_memory;
    use mockito::Matcher;
    use time::macros::datetime;

    struct Env {
        db: Mutex<Connection>,
        store: Arc<MemoryStore>,
        auth: Arc<AuthService>,
        clock: FixedClock,
    }

    fn env() -> Env {
        let store = Arc::new(MemoryStore::new());
        let clock = FixedClock(datetime!(2026-11-16 12:00:00 UTC));
        let auth = Arc::new(AuthService::new(
            store.clone(),
            reqwest::Client::new(),
            Arc::new(FixedClock(clock.0)),
        ));
        Env {
            db: Mutex::new(open_in_memory().unwrap()),
            store,
            auth,
            clock,
        }
    }

    /// Plays the browser: follows the authorization URL's redirect_uri with a code.
    fn browser(query: &'static str) -> impl FnOnce(&str) -> Result<()> + Send {
        move |url: &str| {
            let u = url::Url::parse(url).unwrap();
            let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
            let target = format!(
                "{}?state={}&{query}",
                q["redirect_uri"],
                if query.contains("state=") {
                    ""
                } else {
                    &q["state"]
                }
            );
            tokio::spawn(async move {
                let _ = reqwest::get(target).await;
            });
            Ok(())
        }
    }

    fn token_body(scope: &str) -> String {
        format!(
            r#"{{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600,"scope":"{scope}"}}"#
        )
    }

    async fn mock_channel(srv: &mut mockito::Server, body: &str) -> mockito::Mock {
        srv.mock("GET", "/youtube/v3/channels")
            .match_query(Matcher::Any)
            .with_body(body)
            .create_async()
            .await
    }

    const CHANNEL: &str =
        r#"{"items":[{"id":"UCgh0st","snippet":{"title":"gh0stymc","customUrl":"@gh0stymc"}}]}"#;

    #[tokio::test]
    async fn full_connect_flow_stores_tokens_only_in_keyring() {
        let mut srv = mockito::Server::new_async().await;
        let tok = srv
            .mock("POST", "/token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("code".into(), "the-code".into()),
                Matcher::Regex("code_verifier=".into()),
            ]))
            .with_body(token_body(&format!("{SCOPE_READONLY} {SCOPE_ANALYTICS}")))
            .create_async()
            .await;
        mock_channel(&mut srv, CHANNEL).await;
        let e = env();
        let yt = test_connector(&srv.url());

        let acc = connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(5),
            browser("code=the-code"),
        )
        .await
        .unwrap();
        tok.assert_async().await;

        assert_eq!(acc.platform, "youtube");
        assert_eq!(acc.external_id, "UCgh0st");
        assert_eq!(acc.handle.as_deref(), Some("@gh0stymc"));
        assert_eq!(acc.status, AccountStatus::Active);
        assert_eq!(acc.granted_scopes.len(), 2);

        let stored = e.store.get(&acc.id).unwrap().unwrap();
        assert_eq!(stored.access_token, "at-1");
        assert_eq!(stored.refresh_token.as_deref(), Some("rt-1"));

        // Nothing token-like in the database.
        let conn = e.db.lock().unwrap();
        let dump: String = conn
            .query_row(
                "SELECT group_concat(granted_scopes || settings) FROM accounts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!dump.contains("at-1") && !dump.contains("rt-1"));
        assert_eq!(
            audit_log::recent_actions(&conn, 1).unwrap()[0].0,
            "account.connect"
        );
        assert!(audit_log::verify_chain(&conn).unwrap());
    }

    #[tokio::test]
    async fn reconnect_keeps_id_and_old_refresh_token_if_none_returned() {
        let mut srv = mockito::Server::new_async().await;
        let scope = format!("{SCOPE_READONLY} {SCOPE_ANALYTICS}");
        let first = srv
            .mock("POST", "/token")
            .with_body(token_body(&scope))
            .create_async()
            .await;
        mock_channel(&mut srv, CHANNEL).await;
        let e = env();
        let yt = test_connector(&srv.url());
        let t = Duration::from_secs(5);
        let a = connect(&e.db, &e.auth, &e.clock, &yt, &[], t, browser("code=c1"))
            .await
            .unwrap();
        first.remove_async().await;
        srv.mock("POST", "/token")
            .with_body(format!(
                r#"{{"access_token":"at-2","expires_in":3600,"scope":"{scope}"}}"#
            ))
            .create_async()
            .await;
        let b = connect(&e.db, &e.auth, &e.clock, &yt, &[], t, browser("code=c2"))
            .await
            .unwrap();
        assert_eq!(a.id, b.id);
        let stored = e.store.get(&a.id).unwrap().unwrap();
        assert_eq!(stored.access_token, "at-2");
        assert_eq!(stored.refresh_token.as_deref(), Some("rt-1"));
    }

    #[tokio::test]
    async fn unticked_scope_is_rejected_and_nothing_is_saved() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_body(token_body(SCOPE_READONLY))
            .create_async()
            .await;
        let e = env();
        let yt = test_connector(&srv.url());
        let r = connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(5),
            browser("code=c"),
        )
        .await;
        assert!(matches!(r, Err(AppError::ScopesDenied)));
        assert!(accounts::list(&e.db.lock().unwrap()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_channel_revokes_the_grant() {
        let mut srv = mockito::Server::new_async().await;
        srv.mock("POST", "/token")
            .with_body(token_body(&format!("{SCOPE_READONLY} {SCOPE_ANALYTICS}")))
            .create_async()
            .await;
        mock_channel(&mut srv, r#"{"items":[]}"#).await;
        let rev = srv
            .mock("POST", "/revoke")
            .match_body(Matcher::UrlEncoded("token".into(), "rt-1".into()))
            .create_async()
            .await;
        let e = env();
        let yt = test_connector(&srv.url());
        let r = connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(5),
            browser("code=c"),
        )
        .await;
        assert!(matches!(r, Err(AppError::NoChannel)));
        rev.assert_async().await;
    }

    #[tokio::test]
    async fn user_denies_consent() {
        let e = env();
        let yt = test_connector("http://127.0.0.1:9");
        let r = connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(5),
            browser("error=access_denied"),
        )
        .await;
        assert!(matches!(r, Err(AppError::AuthCancelled)));
    }

    #[tokio::test]
    async fn unconfigured_platform() {
        let e = env();
        let yt = crate::connectors::youtube::YouTubeConnector::new(None);
        let r = connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(1),
            |_: &str| Ok(()),
        )
        .await;
        assert!(matches!(r, Err(AppError::NotConfigured)));
    }

    async fn connected(srv: &mut mockito::Server, e: &Env) -> Account {
        srv.mock("POST", "/token")
            .with_body(token_body(&format!("{SCOPE_READONLY} {SCOPE_ANALYTICS}")))
            .create_async()
            .await;
        mock_channel(srv, CHANNEL).await;
        let yt = test_connector(&srv.url());
        connect(
            &e.db,
            &e.auth,
            &e.clock,
            &yt,
            &[],
            Duration::from_secs(5),
            browser("code=c"),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn disconnect_revokes_and_deletes_tokens() {
        let mut srv = mockito::Server::new_async().await;
        let e = env();
        let acc = connected(&mut srv, &e).await;
        let rev = srv
            .mock("POST", "/revoke")
            .match_body(Matcher::UrlEncoded("token".into(), "rt-1".into()))
            .create_async()
            .await;
        let yt = test_connector(&srv.url());
        let out = disconnect(&e.db, &e.auth, &e.clock, Some(&yt), &acc.id)
            .await
            .unwrap();
        rev.assert_async().await;
        assert!(out.revoked);
        assert!(e.store.get(&acc.id).unwrap().is_none());
        let conn = e.db.lock().unwrap();
        assert_eq!(
            accounts::get(&conn, &acc.id).unwrap().status,
            AccountStatus::Disconnected
        );
        assert_eq!(
            audit_log::recent_actions(&conn, 1).unwrap()[0].0,
            "account.disconnect"
        );
    }

    #[tokio::test]
    async fn disconnect_offline_still_deletes_local_tokens() {
        let mut srv = mockito::Server::new_async().await;
        let e = env();
        let acc = connected(&mut srv, &e).await;
        // Revocation endpoint unreachable.
        let yt = crate::connectors::youtube::YouTubeConnector::with_endpoints(
            crate::auth::ClientCredentials::from_parts(Some("cid"), None),
            crate::connectors::youtube::Endpoints {
                revoke_url: "http://127.0.0.1:9/revoke".into(),
                ..Default::default()
            },
        );
        let out = disconnect(&e.db, &e.auth, &e.clock, Some(&yt), &acc.id)
            .await
            .unwrap();
        assert!(!out.revoked);
        assert!(e.store.get(&acc.id).unwrap().is_none());
    }

    #[tokio::test]
    async fn expired_grant_marks_needs_reauth() {
        let e = env();
        let id = {
            let conn = e.db.lock().unwrap();
            let id = accounts::id_for(&conn, "youtube", "UC1").unwrap();
            accounts::upsert(
                &conn,
                &id,
                &NewAccount {
                    platform: "youtube",
                    external_id: "UC1",
                    handle: None,
                    display_name: "x",
                    granted_scopes: &[],
                },
                "t",
            )
            .unwrap();
            id
        };
        assert!(!handle_auth_error(&e.db, &id, &AppError::Network));
        assert!(handle_auth_error(&e.db, &id, &AppError::TokenExpired));
        assert!(!handle_auth_error(&e.db, &id, &AppError::TokenExpired));
        assert_eq!(
            accounts::get(&e.db.lock().unwrap(), &id).unwrap().status,
            AccountStatus::NeedsReauth
        );
    }
}
