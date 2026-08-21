//! Meteora DBC instruction definitions.
//!
//! Uses 8-byte Anchor discriminators. Handles swap (legacy) and swap2.

pub mod swap;

pub use swap::{
    Swap2Accounts, Swap2Params, SwapAccounts, SwapBuilder, SwapBuilderConfig, SwapMode, SwapParams,
};

use crate::parsing::{ClassifiesAsSwap, SwapAmount, SwapClassification};
use crate::traits::SwapDirection;
use solana_protocols_macros::ProtocolInstruction;

use super::constants::{SWAP2_DISCRIMINATOR, SWAP_DISCRIMINATOR};

/// Meteora DBC instruction enum.
#[derive(Debug, Clone, ProtocolInstruction)]
pub enum MeteoraDbcInstruction {
    /// Legacy swap (exact-in only).
    #[instruction(discriminator = SWAP_DISCRIMINATOR)]
    Swap(SwapParams),
    /// Swap2 (supports ExactIn, PartialFill, ExactOut).
    #[instruction(discriminator = SWAP2_DISCRIMINATOR)]
    Swap2(Swap2Params),
}

impl ClassifiesAsSwap for MeteoraDbcInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            MeteoraDbcInstruction::Swap(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Exact(params.amount_in),
                SwapAmount::Minimum(params.minimum_amount_out),
            )),
            MeteoraDbcInstruction::Swap2(params) => {
                match SwapMode::from_u8(params.swap_mode) {
                    Some(SwapMode::ExactIn) | Some(SwapMode::PartialFill) => {
                        Some(SwapClassification::new(
                            SwapDirection::Buy,
                            SwapAmount::Exact(params.amount_0),
                            SwapAmount::Minimum(params.amount_1),
                        ))
                    }
                    Some(SwapMode::ExactOut) => Some(SwapClassification::new(
                        SwapDirection::Sell,
                        SwapAmount::Maximum(params.amount_1),
                        SwapAmount::Exact(params.amount_0),
                    )),
                    None => {
                        // Unknown swap mode — still a swap, use amount_0 as input
                        Some(SwapClassification::new(
                            SwapDirection::Buy,
                            SwapAmount::Unknown,
                            SwapAmount::Unknown,
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_swap_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&900_000u64.to_le_bytes());
        data
    }

    fn make_swap2_exact_in_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP2_DISCRIMINATOR);
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // amount_0
        data.extend_from_slice(&400_000u64.to_le_bytes()); // amount_1
        data.push(0u8); // ExactIn
        data
    }

    fn make_swap2_exact_out_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP2_DISCRIMINATOR);
        data.extend_from_slice(&400_000u64.to_le_bytes()); // amount_0 (desired output)
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes()); // amount_1 (max input)
        data.push(2u8); // ExactOut
        data
    }

    #[test]
    fn try_from_slice_swap() {
        let data = make_swap_data();
        let ix = MeteoraDbcInstruction::try_from_slice(&data).unwrap();

        match ix {
            MeteoraDbcInstruction::Swap(params) => {
                assert_eq!(params.amount_in, 1_000_000_000);
                assert_eq!(params.minimum_amount_out, 900_000);
            }
            MeteoraDbcInstruction::Swap2(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn try_from_slice_swap2_exact_in() {
        let data = make_swap2_exact_in_data();
        let ix = MeteoraDbcInstruction::try_from_slice(&data).unwrap();

        match ix {
            MeteoraDbcInstruction::Swap2(params) => {
                assert_eq!(params.amount_0, 500_000_000);
                assert_eq!(params.amount_1, 400_000);
                assert_eq!(params.mode(), Some(SwapMode::ExactIn));
            }
            MeteoraDbcInstruction::Swap(_) => panic!("expected Swap2"),
        }
    }

    #[test]
    fn try_from_slice_swap2_exact_out() {
        let data = make_swap2_exact_out_data();
        let ix = MeteoraDbcInstruction::try_from_slice(&data).unwrap();

        match ix {
            MeteoraDbcInstruction::Swap2(params) => {
                assert_eq!(params.amount_0, 400_000);
                assert_eq!(params.amount_1, 1_100_000_000);
                assert_eq!(params.mode(), Some(SwapMode::ExactOut));
            }
            MeteoraDbcInstruction::Swap(_) => panic!("expected Swap2"),
        }
    }

    #[test]
    fn classifies_swap() {
        let data = make_swap_data();
        let ix = MeteoraDbcInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Buy);
    }

    #[test]
    fn classifies_swap2_exact_out() {
        let data = make_swap2_exact_out_data();
        let ix = MeteoraDbcInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Sell);
    }

    #[test]
    fn unknown_discriminator() {
        let data = vec![0xFFu8; 25];
        let result = MeteoraDbcInstruction::try_from_slice(&data);
        assert!(result.is_err());
    }
}
