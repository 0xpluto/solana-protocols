//! Raydium CPMM pool state (on-chain account layout).
//!
//! Constant product AMM with Token2022 support. Pool account stores mints,
//! vaults, status, fee configuration, and decimals. Reserves are in vault
//! token accounts (not inline) — must be fetched separately.
//!
//! Deserialization: bincode after stripping 8-byte Anchor discriminator.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::OnchainState;

use super::constants::CPMM_POOL_STATE_DISCRIMINATOR;
use solana_program::pubkey::Pubkey;

use crate::error::{Error, Result};

/// Minimum account data size for CPMM pool (including 8-byte discriminator).
pub const CPMM_POOL_ACCOUNT_MIN_SIZE: usize = 637;

/// Pool status bit indices for checking enabled operations.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PoolStatusBitIndex {
    /// Deposit operations.
    Deposit = 0,
    /// Withdraw operations.
    Withdraw = 1,
    /// Swap operations.
    Swap = 2,
}

/// Raydium CPMM pool state (on-chain account data).
///
/// Reserves are NOT stored in the pool account — they live in vault token
/// accounts (token_0_vault, token_1_vault). Use `spot_price_with_vaults()`
/// when vault balances are available.
#[derive(Debug, Clone, Serialize, Deserialize, OnchainState)]
#[state(discriminator = CPMM_POOL_STATE_DISCRIMINATOR)]
#[state(fixtures("raydium_cpmm/pool_account.json"))]
pub struct CpmmPoolState {
    /// AMM config account.
    pub amm_config: Pubkey,
    /// Pool creator.
    pub pool_creator: Pubkey,
    /// Token 0 vault account.
    pub token_0_vault: Pubkey,
    /// Token 1 vault account.
    pub token_1_vault: Pubkey,
    /// LP token mint.
    pub lp_mint: Pubkey,
    /// Token 0 mint.
    pub token_0_mint: Pubkey,
    /// Token 1 mint.
    pub token_1_mint: Pubkey,
    /// Token 0 program (SPL Token or Token2022).
    pub token_0_program: Pubkey,
    /// Token 1 program.
    pub token_1_program: Pubkey,
    /// Observation/oracle state account.
    pub observation_key: Pubkey,
    /// Auth PDA bump seed.
    pub auth_bump: u8,
    /// Pool status bitfield (bit0=deposit, bit1=withdraw, bit2=swap).
    pub status: u8,
    /// LP mint decimals.
    pub lp_mint_decimals: u8,
    /// Token 0 mint decimals.
    pub mint_0_decimals: u8,
    /// Token 1 mint decimals.
    pub mint_1_decimals: u8,
    /// LP token supply.
    pub lp_supply: u64,
    /// Protocol fees accumulated for token 0.
    pub protocol_fees_token_0: u64,
    /// Protocol fees accumulated for token 1.
    pub protocol_fees_token_1: u64,
    /// Fund fees for token 0.
    pub fund_fees_token_0: u64,
    /// Fund fees for token 1.
    pub fund_fees_token_1: u64,
    /// Timestamp when swaps are enabled.
    pub open_time: u64,
    /// Recent epoch.
    pub recent_epoch: u64,
    /// Creator fee collection mode (0=BothToken, 1=OnlyToken0, 2=OnlyToken1).
    pub creator_fee_on: u8,
    /// Whether creator fee is enabled.
    pub enable_creator_fee: bool,
    /// Alignment padding.
    pub _padding1: [u8; 6],
    /// Creator fees accumulated for token 0.
    pub creator_fees_token_0: u64,
    /// Creator fees accumulated for token 1.
    pub creator_fees_token_1: u64,
    /// Reserved padding.
    pub _padding_reserved: [u64; 28],
}

impl CpmmPoolState {
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
        <Self as crate::parsing::state::OnchainState>::from_account_data(data).map_err(
            |e| match e {
                crate::parsing::state::AccountParseError::TooShort { len, need } => {
                    Error::AccountDataTooShort {
                        expected: need,
                        actual: len,
                    }
                }
                other => Error::invalid_account_data(other.to_string()),
            },
        )
    }

    /// Check if swap operations are enabled (status bit 2 not set).
    #[must_use]
    pub fn is_swap_enabled(&self) -> bool {
        self.status & (1 << PoolStatusBitIndex::Swap as u8) == 0
    }

    /// Check if the pool is active for swaps.
    ///
    /// Only checks the on-chain status flag. Does NOT verify vault reserves
    /// (which are in separate accounts).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_swap_enabled()
    }

    /// Compute vault amounts minus accumulated fees.
    pub fn vault_amount_without_fee(&self, vault_0: u64, vault_1: u64) -> Result<(u64, u64)> {
        let fees_0 = self
            .protocol_fees_token_0
            .checked_add(self.fund_fees_token_0)
            .and_then(|v| v.checked_add(self.creator_fees_token_0))
            .ok_or(Error::ArithmeticOverflow)?;
        let fees_1 = self
            .protocol_fees_token_1
            .checked_add(self.fund_fees_token_1)
            .and_then(|v| v.checked_add(self.creator_fees_token_1))
            .ok_or(Error::ArithmeticOverflow)?;

        let net_0 = vault_0
            .checked_sub(fees_0)
            .ok_or_else(|| Error::parse_error("vault_0 less than accumulated fees"))?;
        let net_1 = vault_1
            .checked_sub(fees_1)
            .ok_or_else(|| Error::parse_error("vault_1 less than accumulated fees"))?;

        Ok((net_0, net_1))
    }

    /// Compute spot price with external vault amounts.
    ///
    /// Returns price of token_0 in terms of token_1, adjusted for decimals.
    #[must_use]
    pub fn spot_price_with_vaults(&self, vault_0: u64, vault_1: u64) -> f64 {
        if vault_0 == 0 || vault_1 == 0 {
            return 0.0;
        }
        let decimal_adj = 10f64.powi(self.mint_0_decimals as i32 - self.mint_1_decimals as i32);
        (vault_1 as f64 / vault_0 as f64) * decimal_adj
    }

    /// Spot price from pool state alone.
    ///
    /// Returns 0.0 because CPMM reserves are in vault accounts.
    /// Use `spot_price_with_vaults()` when vault balances are available.
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_pool() -> CpmmPoolState {
        CpmmPoolState {
            amm_config: Pubkey::new_unique(),
            pool_creator: Pubkey::new_unique(),
            token_0_vault: Pubkey::new_unique(),
            token_1_vault: Pubkey::new_unique(),
            lp_mint: Pubkey::new_unique(),
            token_0_mint: Pubkey::new_unique(),
            token_1_mint: Pubkey::new_unique(),
            token_0_program: Pubkey::new_unique(),
            token_1_program: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            auth_bump: 254,
            status: 0, // all enabled
            lp_mint_decimals: 9,
            mint_0_decimals: 6,
            mint_1_decimals: 9,
            lp_supply: 1_000_000_000,
            protocol_fees_token_0: 100,
            protocol_fees_token_1: 200,
            fund_fees_token_0: 50,
            fund_fees_token_1: 75,
            open_time: 0,
            recent_epoch: 100,
            creator_fee_on: 0,
            enable_creator_fee: false,
            _padding1: [0u8; 6],
            creator_fees_token_0: 0,
            creator_fees_token_1: 0,
            _padding_reserved: [0u64; 28],
        }
    }

    fn make_account_data(pool: &CpmmPoolState) -> Vec<u8> {
        // The real discriminator, not eight zeros: identity is now checked
        // inside the decoder, and a synthetic account that skips it is testing
        // a path no real caller takes.
        let mut data = CPMM_POOL_STATE_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&bincode::serialize(pool).unwrap());
        data
    }

    #[test]
    fn parse_cpmm_pool() {
        let pool = make_test_pool();
        let data = make_account_data(&pool);

        let parsed = CpmmPoolState::from_account_data(&data).unwrap();
        assert_eq!(parsed.auth_bump, 254);
        assert_eq!(parsed.mint_0_decimals, 6);
        assert_eq!(parsed.mint_1_decimals, 9);
        assert_eq!(parsed.lp_supply, 1_000_000_000);
    }

    #[test]
    fn parse_rejects_short_data() {
        let data = vec![0u8; 100];
        assert!(CpmmPoolState::from_account_data(&data).is_err());
    }

    #[test]
    fn swap_enabled_by_default() {
        let pool = make_test_pool();
        assert!(pool.is_swap_enabled());
        assert!(pool.is_active());
    }

    #[test]
    fn swap_disabled_by_status_bit() {
        let mut pool = make_test_pool();
        pool.status = 1 << PoolStatusBitIndex::Swap as u8; // bit 2 = 4
        assert!(!pool.is_swap_enabled());
        assert!(!pool.is_active());
    }

    #[test]
    fn spot_price_with_vaults() {
        let pool = make_test_pool();
        // token_0 = 6 decimals, token_1 = 9 decimals
        // vault_0 = 1M tokens (1e12 raw), vault_1 = 85 SOL (85e9 raw)
        let price = pool.spot_price_with_vaults(1_000_000_000_000, 85_000_000_000);
        // price = (85e9 / 1e12) * 10^(6-9) = 0.085 * 0.001 = 0.000085
        assert!((price - 0.000085).abs() < 1e-10);
    }

    #[test]
    fn spot_price_without_vaults_returns_zero() {
        let pool = make_test_pool();
        assert_eq!(pool.spot_price(), 0.0);
    }

    #[test]
    fn vault_amount_without_fee() {
        let pool = make_test_pool();
        let (net_0, net_1) = pool.vault_amount_without_fee(10_000, 20_000).unwrap();
        assert_eq!(net_0, 10_000 - 100 - 50); // fees: protocol + fund + creator(0)
        assert_eq!(net_1, 20_000 - 200 - 75);
    }
}
