use omnyra_core::rusqlite::Connection;
use omnyra_core::{clock::SystemClock, connectors::registry::Registry};
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub registry: Registry,
    pub clock: SystemClock,
}
