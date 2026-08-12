//! Pull-style account source.
//!
//! The [`AccountFetcher`] trait covers any source that returns account
//! state on demand for a list of addresses — typically an RPC
//! `getMultipleAccounts` call, but also mock sources in tests.

use async_trait::async_trait;
use solana_pubkey::Pubkey;

use crate::AccountUpdate;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("timeout after {secs}s")]
    Timeout { secs: u64 },
}

/// Source that fetches account state on demand.
#[async_trait]
pub trait AccountFetcher: Send + Sync + 'static {
    /// Fetch account states for `pubkeys`. The returned `Vec` must be the
    /// same length and order as the input; `None` at index `i` means the
    /// account at `pubkeys[i]` does not exist at the observed slot.
    async fn fetch(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<AccountUpdate>>, FetchError>;
}
