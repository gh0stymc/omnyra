//! Generic authentication: OAuth 2.0 + PKCE with a loopback redirect, token storage in the
//! OS keyring, refresh and revocation. Connectors only describe their [`OAuthConfig`];
//! they never touch tokens or the keyring directly.

pub mod loopback;
pub mod oauth;
pub mod pkce;
pub mod service;
pub mod token_store;

pub use oauth::OAuthConfig;
pub use service::AuthService;
pub use token_store::{KeyringStore, MemoryStore, TokenSet, TokenStore};

/// OAuth client credentials baked in at build time from `.env` (see `src-tauri/build.rs`).
#[derive(Clone)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
}

impl ClientCredentials {
    /// `None` when the ID is missing or blank (the platform shows as "not configured").
    pub fn from_parts(id: Option<&str>, secret: Option<&str>) -> Option<Self> {
        let id = id.map(str::trim).filter(|s| !s.is_empty())?;
        Some(Self {
            client_id: id.to_owned(),
            client_secret: secret
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        })
    }
}

impl std::fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_client_id_means_not_configured() {
        assert!(ClientCredentials::from_parts(None, Some("s")).is_none());
        assert!(ClientCredentials::from_parts(Some("  "), Some("s")).is_none());
        let c = ClientCredentials::from_parts(Some("id"), Some("")).unwrap();
        assert_eq!(c.client_secret, None);
    }
}
