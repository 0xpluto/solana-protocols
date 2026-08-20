//! Meteora DBC (Dynamic Bonding Curve) pool state (on-chain account layout).
//!
//! VirtualPool account — bonding curve with configurable migration to AMM pools.
//! Account size: 424 bytes (8-byte discriminator + 416 bytes state).
//!
//! Deserialization: bincode after stripping 8-byte Anchor discriminator.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::OnchainState;

use super::constants::DBC_VIRTUAL_POOL_DISCRIMINATOR;
use solana_program::pubkey::Pubkey;

use crate::error::Result;

/// Minimum account data size (8-byte discriminator + 416 bytes state).
pub const DBC_VIRTUAL_POOL_ACCOUNT_SIZE: usize = 424;

/// Q64 fixed-point denominator (2^64) as f64.
const Q64_F64: f64 = 18_446_744_073_709_551_616.0;

/// Volatility tracker for dynamic fee calculation.
/// Size: 64 bytes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, borsh::BorshDeserialize, OnchainState)]
#[idl(program = "meteora_dbc", account = "VolatilityTracker")]
#[state(no_discriminator)]
pub struct VolatilityTracker {
    /// Last fee update timestamp.
    pub last_update_timestamp: u64,
    /// Alignment padding.
    /// The program calls this `padding`.
    pub padding: [u8; 8],
    /// Reference sqrt price for volatility tracking.
    pub sqrt_price_reference: u128,
    /// Current volatility accumulator.
    pub volatility_accumulator: u128,
    /// Reference volatility (decays over time).
    pub volatility_reference: u128,
}

/// Accumulated fee metrics.
/// Size: 32 bytes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, borsh::BorshDeserialize, OnchainState)]
#[idl(program = "meteora_dbc", account = "PoolMetrics")]
#[state(no_discriminator)]
pub struct PoolMetrics {
    /// Total protocol base token fees.
    pub total_protocol_base_fee: u64,
    /// Total protocol quote token fees.
    pub total_protocol_quote_fee: u64,
    /// Total trading base token fees.
    pub total_trading_base_fee: u64,
    /// Total trading quote token fees.
    pub total_trading_quote_fee: u64,
}

/// Migration progress stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MigrationProgress {
    /// Before bonding curve completion.
    PreBondingCurve = 0,
    /// After bonding curve, before migration.
    PostBondingCurve = 1,
    /// Vesting locked.
    LockedVesting = 2,
    /// AMM pool created.
    CreatedPool = 3,
}

impl MigrationProgress {
    /// Parse from u8 byte.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(MigrationProgress::PreBondingCurve),
            1 => Some(MigrationProgress::PostBondingCurve),
            2 => Some(MigrationProgress::LockedVesting),
            3 => Some(MigrationProgress::CreatedPool),
            _ => None,
        }
    }
}

/// Meteora DBC VirtualPool (main pool account state).
///
/// Size: 416 bytes (without discriminator).
#[derive(Debug, Clone, Serialize, Deserialize, borsh::BorshDeserialize, OnchainState)]
#[idl(program = "meteora_dbc", account = "VirtualPool")]
#[state(discriminator = DBC_VIRTUAL_POOL_DISCRIMINATOR)]
#[state(fixtures("meteora_dbc/pool_account.json"))]
pub struct VirtualPool {
    /// Volatility tracker for dynamic fees.
    pub volatility_tracker: VolatilityTracker,
    /// Config account pubkey.
    pub config: Pubkey,
    /// Pool creator.
    pub creator: Pubkey,
    /// Base token mint.
    pub base_mint: Pubkey,
    /// Base token vault.
    pub base_vault: Pubkey,
    /// Quote token vault.
    pub quote_vault: Pubkey,
    /// Base token reserve amount.
    pub base_reserve: u64,
    /// Quote token reserve amount.
    pub quote_reserve: u64,
    /// Protocol base fee accumulated.
    pub protocol_base_fee: u64,
    /// Protocol quote fee accumulated.
    pub protocol_quote_fee: u64,
    /// Partner base fee accumulated.
    pub partner_base_fee: u64,
    /// Partner quote fee accumulated.
    pub partner_quote_fee: u64,
    /// Current sqrt price (Q64 fixed-point).
    pub sqrt_price: u128,
    /// Activation point (slot or timestamp).
    pub activation_point: u64,
    /// Pool type: 0 = SplToken, 1 = Token2022.
    pub pool_type: u8,
    /// Whether pool has migrated to AMM.
    pub is_migrated: u8,
    /// Whether partner has withdrawn surplus.
    pub is_partner_withdraw_surplus: u8,
    /// Whether protocol has withdrawn surplus.
    pub is_protocol_withdraw_surplus: u8,
    /// Migration progress stage.
    pub migration_progress: u8,
    /// Whether leftover has been withdrawn.
    pub is_withdraw_leftover: u8,
    /// Whether creator has withdrawn surplus.
    pub is_creator_withdraw_surplus: u8,
    /// Migration fee withdraw status.
    pub migration_fee_withdraw_status: u8,
    /// Pool fee metrics.
    pub metrics: PoolMetrics,
    /// Timestamp when curve finished.
    pub finish_curve_timestamp: u64,
    /// Creator base fee accumulated.
    pub creator_base_fee: u64,
    /// Creator quote fee accumulated.
    pub creator_quote_fee: u64,
    /// Legacy creation fee bits (deprecated).
    pub legacy_creation_fee_bits: u8,
    /// Pool creation fee claim status.
    pub creation_fee_bits: u8,
    /// Alignment padding.
    pub _padding_0: [u8; 6],
    /// Reserved padding.
    pub _padding_1: [u64; 6],
}

impl VirtualPool {
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

    /// Check if the pool is active (not migrated).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_migrated == 0
    }

    /// Get the migration progress stage.
    #[must_use]
    pub fn migration_state(&self) -> Option<MigrationProgress> {
        MigrationProgress::from_u8(self.migration_progress)
    }

    /// Spot price from sqrt_price (Q64 fixed-point).
    ///
    /// Formula: price = (sqrt_price / 2^64)^2.
    /// Returns raw price (not decimal-adjusted — caller must adjust for
    /// base/quote decimal differences).
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        if self.sqrt_price == 0 {
            return 0.0;
        }
        let sqrt = self.sqrt_price as f64 / Q64_F64;
        sqrt * sqrt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes() {
        assert_eq!(std::mem::size_of::<VolatilityTracker>(), 64);
        assert_eq!(std::mem::size_of::<PoolMetrics>(), 32);
    }

    fn make_test_pool() -> VirtualPool {
        VirtualPool {
            volatility_tracker: VolatilityTracker::default(),
            config: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            base_reserve: 800_000_000_000_000,
            quote_reserve: 30_000_000_000,
            protocol_base_fee: 0,
            protocol_quote_fee: 0,
            partner_base_fee: 0,
            partner_quote_fee: 0,
            // sqrt_price = sqrt(0.00003) * 2^64 ≈ 1.01e17
            sqrt_price: 101_000_000_000_000_000,
            activation_point: 0,
            pool_type: 0,
            is_migrated: 0,
            is_partner_withdraw_surplus: 0,
            is_protocol_withdraw_surplus: 0,
            migration_progress: 0,
            is_withdraw_leftover: 0,
            is_creator_withdraw_surplus: 0,
            migration_fee_withdraw_status: 0,
            metrics: PoolMetrics::default(),
            finish_curve_timestamp: 0,
            creator_base_fee: 0,
            creator_quote_fee: 0,
            legacy_creation_fee_bits: 0,
            creation_fee_bits: 0,
            _padding_0: [0u8; 6],
            _padding_1: [0u64; 6],
        }
    }

    fn make_account_data(pool: &VirtualPool) -> Vec<u8> {
        // The real discriminator, not eight zeros: identity is now checked
        // inside the decoder, and a synthetic account that skips it is testing
        // a path no real caller takes.
        let mut data = DBC_VIRTUAL_POOL_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&bincode::serialize(pool).unwrap());
        data
    }

    #[test]
    fn parse_virtual_pool() {
        let pool = make_test_pool();
        let data = make_account_data(&pool);

        let parsed = VirtualPool::from_account_data(&data).unwrap();
        assert_eq!(parsed.base_reserve, 800_000_000_000_000);
        assert_eq!(parsed.quote_reserve, 30_000_000_000);
        assert_eq!(parsed.pool_type, 0);
    }

    #[test]
    fn parse_rejects_short_data() {
        let data = vec![0u8; 100];
        assert!(VirtualPool::from_account_data(&data).is_err());
    }

    #[test]
    fn is_active_when_not_migrated() {
        let pool = make_test_pool();
        assert!(pool.is_active());
    }

    #[test]
    fn is_inactive_when_migrated() {
        let mut pool = make_test_pool();
        pool.is_migrated = 1;
        assert!(!pool.is_active());
    }

    #[test]
    fn spot_price_from_sqrt() {
        let mut pool = make_test_pool();
        // Set sqrt_price = 2^64 (so price = 1.0)
        pool.sqrt_price = 1u128 << 64;
        assert!((pool.spot_price() - 1.0).abs() < 0.001);

        // sqrt_price = 0 → price = 0
        pool.sqrt_price = 0;
        assert_eq!(pool.spot_price(), 0.0);
    }

    #[test]
    fn migration_state() {
        let mut pool = make_test_pool();
        assert_eq!(
            pool.migration_state(),
            Some(MigrationProgress::PreBondingCurve)
        );

        pool.migration_progress = 3;
        assert_eq!(pool.migration_state(), Some(MigrationProgress::CreatedPool));

        pool.migration_progress = 99;
        assert_eq!(pool.migration_state(), None);
    }
}
