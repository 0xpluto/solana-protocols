//! Meteora DLMM constants.
//!
//! The codama-generated `meteora-dlmm-sdk` crate already exposes every
//! per-instruction / per-account discriminator. This module re-exports
//! the program ID + event-authority bookkeeping our extractor and
//! handlers need, plus a handful of bin-arithmetic constants the SDK
//! doesn't ship (it's layout-only).

use solana_program::pubkey::Pubkey;

/// Meteora DLMM program ID (`LBUZ…`). Hard-coded rather than re-exporting
/// the SDK's `LB_CLMM_ID` because the SDK's `Pubkey` is from a
/// different `solana-pubkey` major version (4.x vs our workspace's
/// 2.x); both encode to identical bytes but the types don't unify.
/// Validated at compile time by the test below.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

/// Anchor `event_authority` PDA for the DLMM program. Constant —
/// derived once from `seeds = [b"__event_authority"]` and the program
/// id; embedded literally for zero-cost lookup at parse time.
pub const EVENT_AUTHORITY: Pubkey =
    solana_program::pubkey!("D1ZN9Wj1fRSUQfCjhvnu1hqDMT7hzjzBBpi12nVniYD6");

// =====================================================================
// Account discriminators — re-exports from the SDK so handlers can
// reference them without taking a direct sdk dep at every call site.
// =====================================================================

/// `LbPair` account discriminator.
pub const LB_PAIR_DISCRIMINATOR: [u8; 8] = meteora_dlmm_sdk::accounts::LB_PAIR_DISCRIMINATOR;

/// `BinArray` account discriminator.
pub const BIN_ARRAY_DISCRIMINATOR: [u8; 8] = meteora_dlmm_sdk::accounts::BIN_ARRAY_DISCRIMINATOR;

/// `BinArrayBitmapExtension` account discriminator (sparse pools).
pub const BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR;

/// `PositionV2` account discriminator.
pub const POSITION_V2_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::POSITION_V2_DISCRIMINATOR;

/// Legacy `Position` account discriminator (pre-V2). Kept for completeness;
/// no on-chain program writes these any more.
pub const POSITION_DISCRIMINATOR: [u8; 8] = meteora_dlmm_sdk::accounts::POSITION_DISCRIMINATOR;

/// `Oracle` account discriminator.
pub const ORACLE_DISCRIMINATOR: [u8; 8] = meteora_dlmm_sdk::accounts::ORACLE_DISCRIMINATOR;

/// `PresetParameter` account discriminator.
pub const PRESET_PARAMETER_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::PRESET_PARAMETER_DISCRIMINATOR;

/// `PresetParameter2` account discriminator.
pub const PRESET_PARAMETER2_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::PRESET_PARAMETER2_DISCRIMINATOR;

/// `TokenBadge` account discriminator.
pub const TOKEN_BADGE_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::TOKEN_BADGE_DISCRIMINATOR;

/// `ClaimFeeOperator` account discriminator.
pub const CLAIM_FEE_OPERATOR_DISCRIMINATOR: [u8; 8] =
    meteora_dlmm_sdk::accounts::CLAIM_FEE_OPERATOR_DISCRIMINATOR;

// =====================================================================
// Bin / fee constants — not exposed by the SDK.
// =====================================================================

/// Bins per `BinArray` account.
pub const MAX_BIN_PER_ARRAY: usize = 70;

/// Lower bound of the bin id space.
pub const MIN_BIN_ID: i32 = -443_636;

/// Upper bound of the bin id space.
pub const MAX_BIN_ID: i32 = 443_636;

/// Fee precision divisor (1e9). Fees are stored as
/// `fee_rate / FEE_PRECISION`.
pub const FEE_PRECISION: u64 = 1_000_000_000;

/// Hard cap on the total fee rate (10% in `FEE_PRECISION` units).
pub const MAX_FEE_RATE: u64 = 100_000_000;

/// Basis-point denominator used by `bin_step` (1bp = 1e-4).
pub const BASIS_POINT_MAX: i32 = 10_000;

/// Number of bin arrays a single `LbPair` bitmap covers
/// (`bitmap.len() == 16` × 64 bits).
pub const BIN_ARRAY_BITMAP_SIZE: i32 = 512;

/// Reward token slots per pool.
pub const NUM_REWARDS: usize = 2;

/// Number of additional 512-bit bitmap pages a `BinArrayBitmapExtension`
/// account holds per direction (positive / negative). Together with
/// the inline 1024-bit bitmap on the `LbPair`, the extension covers
/// the full `[MIN_BIN_ID, MAX_BIN_ID]` range for sparse pools.
pub const EXTENSION_BIN_ARRAY_BITMAP_SIZE: usize = 12;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_matches_sdk() {
        // Bytes-equal — types differ across solana-pubkey versions.
        assert_eq!(
            PROGRAM_ID.to_bytes(),
            meteora_dlmm_sdk::programs::LB_CLMM_ID.to_bytes()
        );
    }

    #[test]
    fn account_discriminators_unique() {
        let all = [
            LB_PAIR_DISCRIMINATOR,
            BIN_ARRAY_DISCRIMINATOR,
            BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR,
            POSITION_V2_DISCRIMINATOR,
            POSITION_DISCRIMINATOR,
            ORACLE_DISCRIMINATOR,
            PRESET_PARAMETER_DISCRIMINATOR,
            PRESET_PARAMETER2_DISCRIMINATOR,
            TOKEN_BADGE_DISCRIMINATOR,
            CLAIM_FEE_OPERATOR_DISCRIMINATOR,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "duplicate discriminator");
            }
        }
    }
}
