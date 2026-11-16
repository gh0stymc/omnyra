//! Registry of connectors. Adding a network = a new folder + one line here
//! (and one line in `src/platforms/registry.ts` on the frontend).

use super::{mock::MockConnector, Connector};
use std::sync::Arc;

pub struct Registry {
    connectors: Vec<Arc<dyn Connector>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            connectors: vec![Arc::new(MockConnector::new())],
        }
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
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_mock_and_rejects_unknown() {
        let r = Registry::new();
        assert!(r.get("mock").is_some());
        assert!(r.get("nope").is_none());
    }
}
