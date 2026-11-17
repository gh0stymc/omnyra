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
use crate::quota::{QuotaGate, Unmetered};
use crate::{AppError, Result};
use async_trait::async_trait;
use http::ApiClient;
use std::sync::Arc;

/// Context handed to a connector for one account: an HTTP client that already carries
/// (and refreshes) the account's token, and the quota gate that metered calls are charged
/// to. A connector never touches the keyring.
#[derive(Clone)]
pub struct AccountCtx {
    pub account_id: String,
    pub external_id: String,
    pub api: ApiClient,
    pub quota: Arc<dyn QuotaGate>,
}

impl AccountCtx {
    /// Context without quota metering (connect flow, mock, tests).
    pub fn unmetered(account_id: &str, external_id: &str, api: ApiClient) -> Self {
        Self {
            account_id: account_id.into(),
            external_id: external_id.into(),
            api,
            quota: Arc::new(Unmetered),
        }
    }
}

impl std::fmt::Debug for AccountCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountCtx")
            .field("account_id", &self.account_id)
            .field("external_id", &self.external_id)
            .finish_non_exhaustive()
    }
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> &PlatformInfo;

    /// OAuth description, if the platform uses OAuth and its client ID was configured.
    fn oauth(&self) -> Option<&Arc<OAuthConfig>> {
        None
    }

    /// Identifies the account right after connecting.
    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile>;

    /// Profile plus current cumulative totals.
    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot>;

    /// Daily historic metrics for `range` (inclusive days). Only meaningful when
    /// `capabilities.historic_analytics` is set.
    async fn fetch_channel_metrics(
        &self,
        ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>>;

    /// One page of the account's contents, newest first.
    async fn list_contents(
        &self,
        ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>>;

    /// Metadata and current stats for `ids` (any number; the connector batches).
    async fn fetch_content_details(
        &self,
        ctx: &AccountCtx,
        ids: &[String],
    ) -> Result<Vec<ContentDetails>>;

    /// Refines the provisional kind of contents whose `kind_final` was false. Returns
    /// `(id, kind)` for the ones it could settle; the rest are retried on the next sync.
    async fn classify(&self, _ctx: &AccountCtx, _ids: &[String]) -> Vec<(String, String)> {
        Vec::new()
    }

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
