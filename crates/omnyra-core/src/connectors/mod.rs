//! Connector system: every social network implements [`Connector`].
//! Authentication is NOT part of the trait (generic `auth` module, step 1a).

pub mod mock;
pub mod registry;
pub mod types;

pub use types::*;

use crate::{AppError, Result};
use async_trait::async_trait;

/// Context handed to a connector for one account. Later (1a/1b) it will carry an
/// authenticated HTTP client, the rate limiter and the quota ledger.
/// A connector never touches the keyring.
#[derive(Debug, Clone)]
pub struct AccountCtx {
    pub account_id: String,
    pub external_id: String,
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> &PlatformInfo;

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
