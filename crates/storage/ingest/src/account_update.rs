//! Normalized account-update shape.
//!
//! Every source (RPC batch, gRPC stream, tests) produces `AccountUpdate` so
//! the dispatch path has one input shape.

use solana_pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct AccountUpdate {
    /// Account address.
    pub pubkey: Pubkey,
    /// Program ID that owns the account.
    pub owner: Pubkey,
    /// Raw account bytes, including any discriminator prefix.
    pub data: Vec<u8>,
    /// Slot at which the account was observed.
    pub slot: u64,
}

impl AccountUpdate {
    pub fn new(pubkey: Pubkey, owner: Pubkey, data: Vec<u8>, slot: u64) -> Self {
        Self {
            pubkey,
            owner,
            data,
            slot,
        }
    }
}
