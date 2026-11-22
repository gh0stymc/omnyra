//! Registry of connectors. Adding a network = a new folder + one line here
//! (and one line in `src/platforms/registry.ts` on the frontend).

use super::{
    instagram::InstagramConnector, mock::MockConnector, tiktok::TikTokConnector,
    twitch::TwitchConnector, youtube::YouTubeConnector, Connector,
};
use crate::auth::ClientCredentials;
use std::sync::Arc;

/// OAuth client credentials per platform (from the build-time `.env`).
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub google: Option<ClientCredentials>,
    pub twitch: Option<ClientCredentials>,
    pub instagram: Option<ClientCredentials>,
    pub tiktok: Option<ClientCredentials>,
    /// Token broker holding the Meta and TikTok secrets (docs/AUTH_STRATEGY.md).
    pub broker_url: Option<String>,
}

pub struct Registry {
    connectors: Vec<Arc<dyn Connector>>,
}

impl Registry {
    pub fn new(creds: Credentials) -> Self {
        Self::from_connectors(vec![
            Arc::new(YouTubeConnector::new(creds.google)),
            Arc::new(TwitchConnector::new(creds.twitch)),
            Arc::new(InstagramConnector::new(
                creds.instagram,
                creds.broker_url.as_deref(),
            )),
            Arc::new(TikTokConnector::new(
                creds.tiktok,
                creds.broker_url.as_deref(),
            )),
            Arc::new(MockConnector::new()),
        ])
    }

    pub fn from_connectors(connectors: Vec<Arc<dyn Connector>>) -> Self {
        Self { connectors }
    }

    pub fn all(&self) -> &[Arc<dyn Connector>] {
        &self.connectors
    }

    pub fn get(&self, platform: &str) -> Option<Arc<dyn Connector>> {
        self.connectors
            .iter()
            .find(|c| c.info().id == platform)
            .cloned()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(Credentials::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_mock_and_rejects_unknown() {
        let r = Registry::default();
        assert!(r.get("mock").is_some());
        assert!(r.get("youtube").is_some());
        assert!(r.get("twitch").is_some());
        assert!(r.get("instagram").is_some());
        assert!(r.get("tiktok").is_some());
        assert!(r.get("nope").is_none());
    }
}
