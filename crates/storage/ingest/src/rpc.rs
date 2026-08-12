//! Solana JSON-RPC-backed [`AccountFetcher`].
//!
//! Wraps `solana-client`'s nonblocking [`RpcClient`] and calls
//! `getMultipleAccounts` under the hood. Only compiled when the crate's
//! `rpc` feature is enabled.
//!
//! [`RpcClient`]: solana_client::nonblocking::rpc_client::RpcClient

use std::sync::Arc;

use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;

use crate::{AccountFetcher, AccountUpdate, FetchError};

/// `AccountFetcher` that resolves pubkeys against a Solana JSON-RPC node.
///
/// The caller is expected to keep batch sizes at or below the RPC's
/// `getMultipleAccounts` limit (currently 100 on mainnet endpoints).
/// Larger batches will either error out at the server or return a
/// truncated response — this fetcher surfaces either case as
/// [`FetchError::Backend`].
#[derive(Clone)]
pub struct RpcFetcher {
    client: Arc<RpcClient>,
    commitment: CommitmentConfig,
}

impl RpcFetcher {
    /// Construct with the default `Confirmed` commitment.
    pub fn new(client: Arc<RpcClient>) -> Self {
        Self {
            client,
            commitment: CommitmentConfig::confirmed(),
        }
    }

    /// Construct with an explicit commitment level.
    pub fn with_commitment(client: Arc<RpcClient>, commitment: CommitmentConfig) -> Self {
        Self { client, commitment }
    }

    pub fn commitment(&self) -> CommitmentConfig {
        self.commitment
    }
}

#[async_trait]
impl AccountFetcher for RpcFetcher {
    async fn fetch(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<AccountUpdate>>, FetchError> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .client
            .get_multiple_accounts_with_commitment(pubkeys, self.commitment)
            .await
            .map_err(|e| FetchError::Backend(e.to_string()))?;

        if response.value.len() != pubkeys.len() {
            return Err(FetchError::Backend(format!(
                "RPC returned {} accounts for {} pubkeys",
                response.value.len(),
                pubkeys.len(),
            )));
        }

        let slot = response.context.slot;
        Ok(pubkeys
            .iter()
            .zip(response.value)
            .map(|(pubkey, account_opt)| {
                account_opt.map(|account| AccountUpdate {
                    pubkey: *pubkey,
                    owner: account.owner,
                    data: account.data,
                    slot,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The only thing practical to check at this layer without a live RPC
    // is the type shape: that `RpcFetcher` is buildable and implements
    // `AccountFetcher`. Network-level behavior is covered by integration
    // tests against a real devnet/testnet endpoint.
    #[test]
    fn rpc_fetcher_implements_account_fetcher_trait() {
        fn assert_account_fetcher<F: AccountFetcher>() {}
        assert_account_fetcher::<RpcFetcher>();
    }

    #[test]
    fn rpc_fetcher_defaults_to_confirmed_commitment() {
        let client = Arc::new(RpcClient::new("https://example.invalid".to_string()));
        let fetcher = RpcFetcher::new(client);
        assert_eq!(fetcher.commitment(), CommitmentConfig::confirmed());
    }
}
