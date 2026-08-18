//! Raydium CLMM pool state (on-chain account layout).
//!
//! Ported from v2-crates reference. These are the raw on-chain account layouts
//! deserialized with bincode (after stripping the 8-byte Anchor discriminator).
//!
//! Unlike Raydium V4 where reserves are in separate vault accounts, CLMM pools
//! store the sqrt_price and liquidity directly in the pool state. However, token
//! mints and vault addresses are also stored here.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::OnchainState;

use super::constants::CLMM_POOL_STATE_DISCRIMINATOR;
use solana_program::pubkey::Pubkey;

/// Raydium CLMM pool state (on-chain account data).
///
/// Size: ~1544 bytes (excluding discriminator). Fields exactly match the
/// on-chain layout for correct bincode deserialization.
#[repr(C)]
#[derive(Deserialize, Serialize, Debug, Clone, Default, OnchainState)]
#[state(discriminator = CLMM_POOL_STATE_DISCRIMINATOR)]
#[state(fixtures("raydium_clmm/pool_account.json"))]
pub struct PoolState {
    /// PDA bump seed.
    pub bump: [u8; 1],
    /// AMM config account address.
    pub amm_config: Pubkey,
    /// Pool creator/owner.
    pub owner: Pubkey,
    /// Token mint 0 (usually base).
    pub token_mint_0: Pubkey,
    /// Token mint 1 (usually quote).
    pub token_mint_1: Pubkey,
    /// Token vault 0.
    pub token_vault_0: Pubkey,
    /// Token vault 1.
    pub token_vault_1: Pubkey,
    /// Observation state address.
    pub observation_key: Pubkey,
    /// Decimals for token 0.
    pub mint_decimals_0: u8,
    /// Decimals for token 1.
    pub mint_decimals_1: u8,
    /// Tick spacing for this pool.
    pub tick_spacing: u16,
    /// Current liquidity in the active tick range.
    pub liquidity: u128,
    /// Current sqrt price as Q64.64 fixed-point.
    pub sqrt_price_x64: u128,
    /// Current tick index.
    pub tick_current: i32,
    /// Padding.
    pub padding3: u16,
    /// Padding.
    pub padding4: u16,
    /// Global fee growth for token 0 (Q64.64).
    pub fee_growth_global_0_x64: u128,
    /// Global fee growth for token 1 (Q64.64).
    pub fee_growth_global_1_x64: u128,
    /// Accumulated protocol fees for token 0.
    pub protocol_fees_token_0: u64,
    /// Accumulated protocol fees for token 1.
    pub protocol_fees_token_1: u64,
    /// Cumulative swap-in amount token 0.
    pub swap_in_amount_token_0: u128,
    /// Cumulative swap-out amount token 1.
    pub swap_out_amount_token_1: u128,
    /// Cumulative swap-in amount token 1.
    pub swap_in_amount_token_1: u128,
    /// Cumulative swap-out amount token 0.
    pub swap_out_amount_token_0: u128,
    /// Pool status byte.
    pub status: u8,
    /// Padding for alignment.
    pub padding: [u8; 7],
    /// Reward configuration (up to 3 reward tokens).
    pub reward_infos: [RewardInfo; 3],
    /// Tick array bitmap (for quick bitmap lookups).
    pub tick_array_bitmap: [u64; 16],
    /// Total fees collected for token 0.
    pub total_fees_token_0: u64,
    /// Total fees claimed for token 0.
    pub total_fees_claimed_token_0: u64,
    /// Total fees collected for token 1.
    pub total_fees_token_1: u64,
    /// Total fees claimed for token 1.
    pub total_fees_claimed_token_1: u64,
    /// Fund fees for token 0.
    pub fund_fees_token_0: u64,
    /// Fund fees for token 1.
    pub fund_fees_token_1: u64,
    /// Pool open time (unix timestamp).
    pub open_time: u64,
    /// Recent epoch.
    pub recent_epoch: u64,
    /// Reserved padding.
    pub padding1: [u64; 24],
    /// Reserved padding.
    pub padding2: [u64; 32],
}

/// Reward info for a single reward token.
#[repr(C)]
#[derive(Deserialize, Serialize, Debug, Clone, Default, OnchainState)]
#[state(no_discriminator)]
pub struct RewardInfo {
    /// Reward state (0=uninitialized, 1=initialized, 2=opening, 3=ended).
    pub reward_state: u8,
    /// Reward distribution start time.
    pub open_time: u64,
    /// Reward distribution end time.
    pub end_time: u64,
    /// Last update timestamp.
    pub last_update_time: u64,
    /// Emissions per second as Q64.64.
    pub emissions_per_second: u128,
    /// Total rewards emitted.
    pub reward_total_emissioned: u64,
    /// Total rewards claimed.
    pub reward_claimed: u64,
    /// Reward token mint.
    pub token_mint: Pubkey,
    /// Reward token vault.
    pub token_vault: Pubkey,
    /// Reward authority.
    pub authority: Pubkey,
    /// Global reward growth (Q64.64).
    pub reward_growth_global_x64: u128,
}

/// Tick array state (on-chain account data).
///
/// Each tick array holds `TICK_ARRAY_SIZE` (60) ticks.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TickArrayState {
    /// Pool this tick array belongs to.
    pub pool_id: Pubkey,
    /// Starting tick index for this array.
    pub start_tick_index: i32,
    /// Individual tick states.
    pub ticks: [TickState; 60],
    /// Number of initialized ticks in this array.
    pub initialized_tick_count: u8,
    /// Recent epoch.
    pub recent_epoch: u64,
}

/// Individual tick state within a tick array.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TickState {
    /// Tick index.
    pub tick: i32,
    /// Net liquidity change when crossing this tick.
    pub liquidity_net: i128,
    /// Gross liquidity referencing this tick.
    pub liquidity_gross: u128,
    /// Fee growth outside this tick for token 0 (Q64.64).
    pub fee_growth_outside_0_x64: u128,
    /// Fee growth outside this tick for token 1 (Q64.64).
    pub fee_growth_outside_1_x64: u128,
    /// Reward growth outside per reward token (Q64.64).
    pub reward_growths_outside_x64: [u128; 3],
    /// Padding for alignment.
    pub padding: [u32; 13],
}

impl PoolState {
    /// Read this account, verifying identity first.
    ///
    /// Delegates to the `OnchainState` impl the derive generates. It used to
    /// strip eight bytes and hand the rest to bincode without ever looking at
    /// the discriminator — so any account at least this long decoded, including
    /// other programs' accounts, silently producing plausible garbage.
    ///
    /// # Errors
    ///
    /// The data is too short, or carries another account type's discriminator.
    pub fn from_account_data(data: &[u8]) -> Result<Self, String> {
        <Self as crate::parsing::state::OnchainState>::from_account_data(data)
            .map_err(|e| e.to_string())
    }

    /// Check if the pool is active (status == 0 or status-specific check).
    #[must_use]
    pub fn is_active(&self) -> bool {
        // Status 0 = pool open and functioning
        self.status == 0
    }

    /// Get the current price as f64.
    ///
    /// Converts sqrt_price_x64 (Q64.64) to a regular price.
    #[must_use]
    pub fn current_price(&self) -> f64 {
        let sqrt_price = self.sqrt_price_x64 as f64 / (1u128 << 64) as f64;
        let price = sqrt_price * sqrt_price;
        // Adjust for decimal difference
        let decimal_adj = 10f64.powi(self.mint_decimals_0 as i32 - self.mint_decimals_1 as i32);
        price * decimal_adj
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_state_size() {
        // repr(C) alignment causes padding:
        // i32(4) + pad(12) + i128(16) + u128(16) + u128(16) + u128(16) +
        // [u128;3](48) + [u32;13](52) + pad(12) = 192
        assert_eq!(std::mem::size_of::<TickState>(), 192,);
    }

    #[test]
    fn pool_state_has_expected_fields() {
        let pool = PoolState::default();
        assert_eq!(pool.tick_spacing, 0);
        assert_eq!(pool.liquidity, 0);
        assert_eq!(pool.sqrt_price_x64, 0);
        assert_eq!(pool.tick_current, 0);
        // status 0 = active
        assert!(pool.is_active());
    }

    #[test]
    fn pool_is_active_when_status_zero() {
        let mut pool = PoolState {
            status: 0,
            ..Default::default()
        };
        assert!(pool.is_active());

        pool.status = 1;
        assert!(!pool.is_active());
    }
}
