use omnyra_core::rusqlite::Connection;
use omnyra_core::{auth::AuthService, clock::Clock, connectors::registry::Registry};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: Registry,
    pub clock: Arc<dyn Clock>,
    pub auth: Arc<AuthService>,
    /// Cancels the OAuth flow currently waiting for the browser (only one at a time).
    pub connect_cancel: Mutex<Option<oneshot::Sender<()>>>,
}
