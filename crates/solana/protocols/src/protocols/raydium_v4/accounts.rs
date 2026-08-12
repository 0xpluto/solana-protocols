//! Raydium V4 account addresses and PDAs.

use solana_program::pubkey::Pubkey;

use super::constants::{AUTHORITY, PROGRAM_ID};

/// Keys for a Raydium V4 pool.
///
/// These are the addresses needed to interact with a specific pool.
#[derive(Debug, Clone)]
pub struct RaydiumV4Keys {
    /// AMM ID (pool account address).
    pub amm_id: Pubkey,

    /// AMM authority (PDA).
    pub amm_authority: Pubkey,

    /// AMM open orders (Serum).
    pub amm_open_orders: Pubkey,

    /// AMM target orders.
    pub amm_target_orders: Pubkey,

    /// Base token vault.
    pub base_vault: Pubkey,

    /// Quote token vault.
    pub quote_vault: Pubkey,

    /// Base token mint.
    pub base_mint: Pubkey,

    /// Quote token mint.
    pub quote_mint: Pubkey,

    /// Serum market address.
    pub serum_market: Pubkey,

    /// Serum program ID.
    pub serum_program_id: Pubkey,

    /// Serum bids.
    pub serum_bids: Pubkey,

    /// Serum asks.
    pub serum_asks: Pubkey,

    /// Serum event queue.
    pub serum_event_queue: Pubkey,

    /// Serum coin vault.
    pub serum_coin_vault: Pubkey,

    /// Serum PC vault.
    pub serum_pc_vault: Pubkey,

    /// Serum vault signer.
    pub serum_vault_signer: Pubkey,
}

impl RaydiumV4Keys {
    /// Create keys from a pool state.
    ///
    /// This extracts the necessary addresses from the on-chain pool state.
    /// Some Serum addresses need to be fetched separately from the market account.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_pool_state(
        amm_id: Pubkey,
        pool: &super::LiquidityPool,
        serum_bids: Pubkey,
        serum_asks: Pubkey,
        serum_event_queue: Pubkey,
        serum_coin_vault: Pubkey,
        serum_pc_vault: Pubkey,
        serum_vault_signer: Pubkey,
    ) -> Self {
        Self {
            amm_id,
            amm_authority: AUTHORITY,
            amm_open_orders: pool.open_orders,
            amm_target_orders: pool.target_orders,
            base_vault: pool.base_vault,
            quote_vault: pool.quote_vault,
            base_mint: pool.base_mint,
            quote_mint: pool.quote_mint,
            serum_market: pool.market_id,
            serum_program_id: pool.market_program_id,
            serum_bids,
            serum_asks,
            serum_event_queue,
            serum_coin_vault,
            serum_pc_vault,
            serum_vault_signer,
        }
    }

    /// Get the program ID.
    #[must_use]
    pub fn program_id() -> Pubkey {
        PROGRAM_ID
    }
}

/// Derive the AMM authority PDA.
///
/// The authority is a fixed PDA used by all Raydium V4 pools.
#[must_use]
pub fn derive_authority() -> Pubkey {
    // Raydium uses a fixed authority for all pools
    AUTHORITY
}

/// Derive associated pool account PDA from market ID.
///
/// # Arguments
///
/// * `market_id` - The Serum market address
/// * `nonce` - The pool nonce (from pool state)
pub fn derive_amm_id(market_id: &Pubkey, nonce: u8) -> Option<Pubkey> {
    let seeds: &[&[u8]] = &[
        &PROGRAM_ID.to_bytes(),
        &market_id.to_bytes(),
        b"amm_associated_seed",
        &[nonce],
    ];

    Pubkey::try_find_program_address(seeds, &PROGRAM_ID).map(|(addr, _)| addr)
}
