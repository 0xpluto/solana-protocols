//! Raydium CLMM instruction definitions.
//!
//! Uses 8-byte Anchor discriminators. The `ProtocolInstruction` derive macro
//! generates discriminator dispatch, accounts enum, and serialization.
//!
//! Currently implements swap instructions only. Liquidity operations
//! (OpenPosition, IncreaseLiquidity, DecreaseLiquidity) will be added later.

pub mod swap;
pub mod swap_v2;

pub use swap::{SwapAccounts, SwapParams};
pub use swap_v2::{
    sqrt_price_x64_from_tick, SwapV2Accounts, SwapV2Builder, SwapV2BuilderConfig, SwapV2Params,
};

use solana_protocols_macros::ProtocolInstruction;

use super::constants::{PROGRAM_ID, SWAP_DISCRIMINATOR, SWAP_V2_DISCRIMINATOR};
use crate::parsing::{ClassifiesAsSwap, SwapAmount, SwapClassification};
use crate::traits::SwapDirection;

/// Raydium CLMM instruction enum.
///
/// `#[derive(ProtocolInstruction)]` generates:
/// - `try_from_slice(data)` — discriminator dispatch + `FromInstructionData`
/// - `discriminator()` — get 8-byte discriminator
/// - `data()` — serialize back to bytes
/// - `from_accounts(keys)` — parse accounts via `FromAccountKeys`
/// - `RaydiumClmmInstructionAccounts` enum
/// - `RaydiumClmmInstructionEvent` struct
#[derive(Debug, Clone, ProtocolInstruction)]
#[protocol(program_id = PROGRAM_ID)]
pub enum RaydiumClmmInstruction {
    /// Swap (v1) — SPL Token only.
    #[instruction(discriminator = SWAP_DISCRIMINATOR, accounts = SwapAccounts)]
    Swap(SwapParams),
    /// SwapV2 — Token2022 support + memo + explicit mints.
    #[instruction(discriminator = SWAP_V2_DISCRIMINATOR, accounts = SwapV2Accounts)]
    SwapV2(SwapV2Params),
}

impl RaydiumClmmInstruction {
    /// Check if this is a swap (v1) instruction.
    #[must_use]
    pub fn is_swap(&self) -> bool {
        matches!(self, RaydiumClmmInstruction::Swap(_))
    }

    /// Check if this is a swap v2 instruction.
    #[must_use]
    pub fn is_swap_v2(&self) -> bool {
        matches!(self, RaydiumClmmInstruction::SwapV2(_))
    }

    /// Check if this is any kind of swap instruction.
    #[must_use]
    pub fn is_any_swap(&self) -> bool {
        matches!(
            self,
            RaydiumClmmInstruction::Swap(_) | RaydiumClmmInstruction::SwapV2(_)
        )
    }
}

impl ClassifiesAsSwap for RaydiumClmmInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            RaydiumClmmInstruction::Swap(params) => {
                if params.is_base_input {
                    Some(SwapClassification::new(
                        SwapDirection::Buy,
                        SwapAmount::Exact(params.amount),
                        SwapAmount::Minimum(params.other_amount_threshold),
                    ))
                } else {
                    Some(SwapClassification::new(
                        SwapDirection::Sell,
                        SwapAmount::Maximum(params.other_amount_threshold),
                        SwapAmount::Exact(params.amount),
                    ))
                }
            }
            RaydiumClmmInstruction::SwapV2(params) => {
                if params.is_base_input {
                    Some(SwapClassification::new(
                        SwapDirection::Buy,
                        SwapAmount::Exact(params.amount),
                        SwapAmount::Minimum(params.other_amount_threshold),
                    ))
                } else {
                    Some(SwapClassification::new(
                        SwapDirection::Sell,
                        SwapAmount::Maximum(params.other_amount_threshold),
                        SwapAmount::Exact(params.amount),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::constants::{SWAP_DISCRIMINATOR, SWAP_V2_DISCRIMINATOR};
    use super::*;

    fn make_swap_data(is_base_input: bool) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount
        data.extend_from_slice(&900_000u64.to_le_bytes()); // threshold
        data.extend_from_slice(&4_295_048_017u128.to_le_bytes()); // sqrt_price_limit
        data.push(u8::from(is_base_input));
        data
    }

    fn make_swap_v2_data(is_base_input: bool) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP_V2_DISCRIMINATOR);
        data.extend_from_slice(&630_000_000u64.to_le_bytes());
        data.extend_from_slice(&5_450_147_723_573u64.to_le_bytes());
        data.extend_from_slice(&4_295_048_017u128.to_le_bytes());
        data.push(u8::from(is_base_input));
        data
    }

    #[test]
    fn try_from_slice_swap() {
        let data = make_swap_data(true);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        assert!(ix.is_swap());
        assert!(!ix.is_swap_v2());
        assert!(ix.is_any_swap());

        match ix {
            RaydiumClmmInstruction::Swap(params) => {
                assert_eq!(params.amount, 1_000_000_000);
                assert_eq!(params.other_amount_threshold, 900_000);
                assert!(params.is_base_input);
            }
            RaydiumClmmInstruction::SwapV2(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn try_from_slice_swap_v2() {
        let data = make_swap_v2_data(true);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        assert!(!ix.is_swap());
        assert!(ix.is_swap_v2());
        assert!(ix.is_any_swap());

        match ix {
            RaydiumClmmInstruction::SwapV2(params) => {
                assert_eq!(params.amount, 630_000_000);
                assert_eq!(params.other_amount_threshold, 5_450_147_723_573u64);
                assert!(params.is_base_input);
            }
            RaydiumClmmInstruction::Swap(_) => panic!("expected SwapV2"),
        }
    }

    #[test]
    fn classifies_as_swap_base_input() {
        let data = make_swap_data(true);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Buy);
    }

    #[test]
    fn classifies_as_swap_base_output() {
        let data = make_swap_data(false);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Sell);
    }

    #[test]
    fn unknown_discriminator() {
        let data = vec![0xFF; 41];
        let result = RaydiumClmmInstruction::try_from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_swap() {
        let data = make_swap_data(true);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        let serialized = ix.data();
        assert_eq!(data, serialized);
    }

    #[test]
    fn roundtrip_swap_v2() {
        let data = make_swap_v2_data(false);
        let ix = RaydiumClmmInstruction::try_from_slice(&data).unwrap();
        let serialized = ix.data();
        assert_eq!(data, serialized);
    }
}
