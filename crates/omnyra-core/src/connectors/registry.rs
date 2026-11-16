//! Registry of connectors. Adding a network = a new folder + one line here
//! (and one line in `src/platforms/registry.ts` on the frontend).

use super::{mock::MockConnector, youtube::YouTubeConnector, Connector};
use crate::auth::ClientCredentials;
use std::sync::Arc;

/// OAuth client credentials per platform (from the build-time `.env`).
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub google: Option<ClientCredentials>,
}

pub struct Registry {
    connectors: Vec<Arc<dyn Connector>>,
}

impl Registry {
    pub fn new(creds: Credentials) -> Self {
        Self::from_connectors(vec![
            Arc::new(YouTubeConnector::new(creds.google)),
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
        assert!(r.get("nope").is_none());
    }
}
