//! Raydium Launchpad pool state (on-chain account layout).
//!
//! Bonding curve platform with configurable curve types (constant product,
//! fixed price, linear price). Tokens graduate to AMM after hitting
//! a quote reserve threshold.
//!
//! Deserialization: bincode after stripping 8-byte Anchor discriminator.
//! Trailing 62-byte padding is omitted (exceeds serde array limit).

use serde::{Deserialize, Serialize};
use solana_protocols_macros::OnchainState;

use super::constants::LAUNCHPAD_POOL_STATE_DISCRIMINATOR;
use solana_program::pubkey::Pubkey;

use crate::error::Result;

/// Minimum account data size (8 disc + ~421 data).
pub const LAUNCHPAD_POOL_ACCOUNT_MIN_SIZE: usize = 429;

/// Pool status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PoolStatus {
    /// Funding phase — not yet trading.
    Fund = 0,
    /// Live — trading active.
    Live = 1,
    /// Ended — migrated to AMM.
    Ended = 2,
}

impl PoolStatus {
    /// Parse from u8 byte.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(PoolStatus::Fund),
            1 => Some(PoolStatus::Live),
            2 => Some(PoolStatus::Ended),
            _ => None,
        }
    }
}

/// Vesting schedule for token unlocks.
#[derive(Debug, Clone, Serialize, Deserialize, borsh::BorshDeserialize, OnchainState)]
#[state(unverified = "no vendored IDL for this program, so there is nothing to compare the field names against; the layout came from observation and the SDK")]
#[state(no_discriminator)]
pub struct VestingSchedule {
    /// Total locked token amount.
    pub total_locked_amount: u64,
    /// Cliff period (seconds).
    pub cliff_period: u64,
    /// Unlock period (seconds).
    pub unlock_period: u64,
    /// Start time (unix timestamp).
    pub start_time: u64,
    /// Allocated share amount.
    pub allocated_share_amount: u64,
}

/// Raydium Launchpad pool state (on-chain account data).
///
/// Contains virtual and real reserves for bonding curve pricing.
/// The curve type (constant product, fixed price, linear) determines
/// how reserves map to price.
///
/// NOTE: Trailing 62-byte padding omitted — bincode ignores extra bytes.
#[derive(Debug, Clone, Serialize, Deserialize, borsh::BorshDeserialize, OnchainState)]
#[state(unverified = "no vendored IDL for this program, so there is nothing to compare the field names against; the layout came from observation and the SDK")]
#[state(discriminator = LAUNCHPAD_POOL_STATE_DISCRIMINATOR)]
#[state(fixtures("raydium_launchpad/pool_account.json"))]
pub struct LaunchpadPoolState {
    /// Epoch counter.
    pub epoch: u64,
    /// Auth PDA bump seed.
    pub auth_bump: u8,
    /// Pool status (0=Fund, 1=Live, 2=Ended).
    pub status: u8,
    /// Base token decimals.
    pub base_decimals: u8,
    /// Quote token decimals.
    pub quote_decimals: u8,
    /// Migration type.
    pub migrate_type: u8,
    /// Total token supply.
    pub supply: u64,
    /// Total base tokens sold.
    pub total_base_sell: u64,
    /// Virtual base reserve (for pricing).
    pub virtual_base: u64,
    /// Virtual quote reserve (for pricing).
    pub virtual_quote: u64,
    /// Real base tokens accumulated.
    pub real_base: u64,
    /// Real quote tokens accumulated.
    pub real_quote: u64,
    /// Total quote raised for fundraising.
    pub total_quote_fund_raising: u64,
    /// Quote protocol fee accumulated.
    pub quote_protocol_fee: u64,
    /// Platform fee accumulated.
    pub platform_fee: u64,
    /// Migration fee accumulated.
    pub migrate_fee: u64,
    /// Token vesting schedule.
    pub vesting_schedule: VestingSchedule,
    /// Global config account.
    pub global_config: Pubkey,
    /// Platform config account.
    pub platform_config: Pubkey,
    /// Base token mint.
    pub base_mint: Pubkey,
    /// Quote token mint.
    pub quote_mint: Pubkey,
    /// Base token vault.
    pub base_vault: Pubkey,
    /// Quote token vault.
    pub quote_vault: Pubkey,
    /// Pool creator.
    pub creator: Pubkey,
    /// Token program flags (bit0=base, bit1=quote; 0=SPL, 1=Token2022).
    pub token_program_flag: u8,
    /// AMM creator fee mode (0=QuoteToken, 1=BothToken).
    pub amm_creator_fee_on: u8,
    // Trailing padding [u8; 62] omitted — exceeds serde array limit.
    // bincode ignores extra bytes after this point.
}

impl LaunchpadPoolState {
    /// Read this account, verifying identity first.
    ///
    /// Delegates to the `OnchainState` impl the derive generates, so the
    /// discriminator check, the derived length and the golden-fixture test all
    /// apply here rather than only to callers who happen to use the trait.
    ///
    /// # Errors
    ///
    /// The data is too short, or carries another account type's discriminator.
    pub fn from_account_data(data: &[u8]) -> Result<Self> {
        <Self as crate::parsing::state::OnchainState>::from_account_data(data).map_err(Into::into)
    }

    /// Get the parsed pool status.
    #[must_use]
    pub fn pool_status(&self) -> Option<PoolStatus> {
        PoolStatus::from_u8(self.status)
    }

    /// Check if the pool is active for trading.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.pool_status() == Some(PoolStatus::Live)
    }

    /// Effective base reserve available for trading.
    ///
    /// For constant product: virtual_base minus tokens sold (real_base).
    #[must_use]
    pub fn base_reserve(&self) -> u64 {
        self.virtual_base.saturating_sub(self.real_base)
    }

    /// Effective quote reserve.
    ///
    /// For constant product: virtual_quote plus quote collected (real_quote).
    #[must_use]
    pub fn quote_reserve(&self) -> u64 {
        self.virtual_quote.saturating_add(self.real_quote)
    }

    /// Spot price (quote per base, adjusted for decimals).
    ///
    /// Uses constant product formula: price = quote_reserve / base_reserve.
    /// This is accurate for ConstantProduct curve type. For FixedPrice and
    /// LinearPrice curves, the formula differs but this gives an approximation.
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        let base = self.base_reserve();
        let quote = self.quote_reserve();
        if base == 0 || quote == 0 {
            return 0.0;
        }
        let decimal_adj = 10f64.powi(self.base_decimals as i32 - self.quote_decimals as i32);
        (quote as f64 / base as f64) * decimal_adj
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_pool() -> LaunchpadPoolState {
        LaunchpadPoolState {
            epoch: 1,
            auth_bump: 255,
            status: 1, // Live
            base_decimals: 6,
            quote_decimals: 9,
            migrate_type: 0,
            supply: 1_000_000_000_000,
            total_base_sell: 0,
            virtual_base: 1_000_000_000_000_000,
            virtual_quote: 30_000_000_000,
            real_base: 200_000_000_000_000,
            real_quote: 10_000_000_000,
            total_quote_fund_raising: 0,
            quote_protocol_fee: 0,
            platform_fee: 0,
            migrate_fee: 0,
            vesting_schedule: VestingSchedule {
                total_locked_amount: 0,
                cliff_period: 0,
                unlock_period: 0,
                start_time: 0,
                allocated_share_amount: 0,
            },
            global_config: Pubkey::new_unique(),
            platform_config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            token_program_flag: 0,
            amm_creator_fee_on: 0,
        }
    }

    fn make_account_data(pool: &LaunchpadPoolState) -> Vec<u8> {
        // The real discriminator, not eight zeros: identity is now checked
        // inside the decoder, and a synthetic account that skips it is testing
        // a path no real caller takes.
        let mut data = LAUNCHPAD_POOL_STATE_DISCRIMINATOR.to_vec();
        let serialized = bincode::serialize(pool).unwrap();
        data.extend_from_slice(&serialized);
        // Add trailing padding that would exist on-chain
        data.extend_from_slice(&[0u8; 62]);
        data
    }

    #[test]
    fn parse_launchpad_pool() {
        let pool = make_test_pool();
        let data = make_account_data(&pool);

        let parsed = LaunchpadPoolState::from_account_data(&data).unwrap();
        assert_eq!(parsed.auth_bump, 255);
        assert_eq!(parsed.base_decimals, 6);
        assert_eq!(parsed.quote_decimals, 9);
        assert_eq!(parsed.virtual_base, 1_000_000_000_000_000);
    }

    #[test]
    fn parse_rejects_short_data() {
        let data = vec![0u8; 100];
        assert!(LaunchpadPoolState::from_account_data(&data).is_err());
    }

    #[test]
    fn pool_status_live() {
        let pool = make_test_pool();
        assert_eq!(pool.pool_status(), Some(PoolStatus::Live));
        assert!(pool.is_active());
    }

    #[test]
    fn pool_status_fund() {
        let mut pool = make_test_pool();
        pool.status = 0;
        assert_eq!(pool.pool_status(), Some(PoolStatus::Fund));
        assert!(!pool.is_active());
    }

    #[test]
    fn pool_status_ended() {
        let mut pool = make_test_pool();
        pool.status = 2;
        assert_eq!(pool.pool_status(), Some(PoolStatus::Ended));
        assert!(!pool.is_active());
    }

    #[test]
    fn base_quote_reserves() {
        let pool = make_test_pool();
        // base_reserve = virtual_base - real_base = 1e15 - 2e14 = 8e14
        assert_eq!(pool.base_reserve(), 800_000_000_000_000);
        // quote_reserve = virtual_quote + real_quote = 3e10 + 1e10 = 4e10
        assert_eq!(pool.quote_reserve(), 40_000_000_000);
    }

    #[test]
    fn spot_price_calculation() {
        let pool = make_test_pool();
        let price = pool.spot_price();
        // price = (4e10 / 8e14) * 10^(6-9)
        //       = 5e-5 * 1e-3 = 5e-8
        assert!(price > 0.0);
        assert!((price - 5e-8).abs() < 1e-12);
    }

    #[test]
    fn spot_price_zero_reserves() {
        let mut pool = make_test_pool();
        pool.virtual_base = 0;
        pool.real_base = 0;
        assert_eq!(pool.spot_price(), 0.0);
    }
}
