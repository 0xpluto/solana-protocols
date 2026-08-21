//! Raydium CPMM instruction definitions.
//!
//! Uses 8-byte Anchor discriminators. Currently handles swap instructions.

pub mod swap;

pub use swap::{
    SwapAccounts, SwapBaseInputAccounts, SwapBaseInputBuilder, SwapBaseInputBuilderConfig,
    SwapBaseInputParams, SwapBaseOutputAccounts, SwapBaseOutputParams,
};

use crate::parsing::{ClassifiesAsSwap, SwapAmount, SwapClassification};
use crate::traits::SwapDirection;
use solana_protocols_macros::ProtocolInstruction;

use super::constants::{SWAP_BASE_INPUT_DISCRIMINATOR, SWAP_BASE_OUTPUT_DISCRIMINATOR};

/// Raydium CPMM instruction enum.
#[derive(Debug, Clone, ProtocolInstruction)]
pub enum RaydiumCpmmInstruction {
    /// Swap with exact input amount.
    #[instruction(discriminator = SWAP_BASE_INPUT_DISCRIMINATOR)]
    SwapBaseInput(SwapBaseInputParams),
    /// Swap with exact output amount.
    #[instruction(discriminator = SWAP_BASE_OUTPUT_DISCRIMINATOR)]
    SwapBaseOutput(SwapBaseOutputParams),
}

impl ClassifiesAsSwap for RaydiumCpmmInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            RaydiumCpmmInstruction::SwapBaseInput(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Exact(params.amount_in),
                SwapAmount::Minimum(params.minimum_amount_out),
            )),
            RaydiumCpmmInstruction::SwapBaseOutput(params) => Some(SwapClassification::new(
                SwapDirection::Sell,
                SwapAmount::Maximum(params.maximum_amount_in),
                SwapAmount::Exact(params.amount_out),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_swap_base_input_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP_BASE_INPUT_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&900_000u64.to_le_bytes());
        data
    }

    fn make_swap_base_output_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SWAP_BASE_OUTPUT_DISCRIMINATOR);
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes());
        data.extend_from_slice(&1_000_000u64.to_le_bytes());
        data
    }

    #[test]
    fn try_from_slice_swap_base_input() {
        let data = make_swap_base_input_data();
        let ix = RaydiumCpmmInstruction::try_from_slice(&data).unwrap();

        match ix {
            RaydiumCpmmInstruction::SwapBaseInput(params) => {
                assert_eq!(params.amount_in, 1_000_000_000);
                assert_eq!(params.minimum_amount_out, 900_000);
            }
            RaydiumCpmmInstruction::SwapBaseOutput(_) => panic!("expected SwapBaseInput"),
        }
    }

    #[test]
    fn try_from_slice_swap_base_output() {
        let data = make_swap_base_output_data();
        let ix = RaydiumCpmmInstruction::try_from_slice(&data).unwrap();

        match ix {
            RaydiumCpmmInstruction::SwapBaseOutput(params) => {
                assert_eq!(params.maximum_amount_in, 1_100_000_000);
                assert_eq!(params.amount_out, 1_000_000);
            }
            RaydiumCpmmInstruction::SwapBaseInput(_) => panic!("expected SwapBaseOutput"),
        }
    }

    #[test]
    fn classifies_swap_base_input() {
        let data = make_swap_base_input_data();
        let ix = RaydiumCpmmInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Buy);
    }

    #[test]
    fn classifies_swap_base_output() {
        let data = make_swap_base_output_data();
        let ix = RaydiumCpmmInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Sell);
    }

    #[test]
    fn unknown_discriminator() {
        let data = vec![0xFFu8; 24];
        let result = RaydiumCpmmInstruction::try_from_slice(&data);
        assert!(result.is_err());
    }
}
