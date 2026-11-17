use omnyra_core::rusqlite::Connection;
use omnyra_core::{
    auth::AuthService, clock::Clock, connectors::registry::Registry, sync::SyncLocks,
};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub registry: Registry,
    pub clock: Arc<dyn Clock>,
    pub auth: Arc<AuthService>,
    /// Cancels the OAuth flow currently waiting for the browser (only one at a time).
    pub connect_cancel: Mutex<Option<oneshot::Sender<()>>>,
    pub sync_locks: SyncLocks,
    /// Wakes the upload worker when something is queued or retried.
    pub upload_wake: tokio::sync::Notify,
    /// The upload being sent, and how to abort it.
    pub upload_cancel: Mutex<Option<(String, oneshot::Sender<()>)>>,
}
