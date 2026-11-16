//! Connector system: every social network implements [`Connector`].
//! Authentication is NOT part of the trait: the generic `auth` module runs the flow from
//! the connector's [`OAuthConfig`].

pub mod http;
pub mod mock;
pub mod registry;
pub mod types;
pub mod youtube;

pub use types::*;

use crate::auth::OAuthConfig;
use crate::{AppError, Result};
use async_trait::async_trait;
use http::ApiClient;
use std::sync::Arc;

/// Context handed to a connector for one account: an HTTP client that already carries
/// (and refreshes) the account's token. Rate limiter and quota ledger arrive in 1b.
/// A connector never touches the keyring.
#[derive(Debug, Clone)]
pub struct AccountCtx {
    pub account_id: String,
    pub external_id: String,
    pub api: ApiClient,
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> &PlatformInfo;

    /// OAuth description, if the platform uses OAuth and its client ID was configured.
    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        None
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile>;
    async fn fetch_channel_metrics(
        &self,
        ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>>;
    async fn list_contents(&self, ctx: &AccountCtx, page: Option<String>) -> Result<Page<Content>>;

    // Optional capabilities: default is NotSupported.
    async fn upload(&self, _ctx: &AccountCtx, _req: UploadRequest) -> Result<UploadOutcome> {
        Err(AppError::NotSupported)
    }
    async fn list_comments(
        &self,
        _ctx: &AccountCtx,
        _content: &str,
        _page: Option<String>,
    ) -> Result<Page<Comment>> {
        Err(AppError::NotSupported)
    }
    async fn reply_comment(
        &self,
        _ctx: &AccountCtx,
        _parent: &str,
        _text: &str,
    ) -> Result<Comment> {
        Err(AppError::NotSupported)
    }
}
