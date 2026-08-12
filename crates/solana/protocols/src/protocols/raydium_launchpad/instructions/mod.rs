//! Raydium Launchpad instruction definitions.
//!
//! Uses 8-byte Anchor discriminators. Handles 4 swap variants.

pub mod swap;

pub use swap::{
    BuyExactInBuilder, BuyExactInParams, BuyExactOutParams, SellExactInBuilder, SellExactInParams,
    SellExactOutParams, SwapAccounts,
};

use crate::parsing::{ClassifiesAsSwap, SwapAmount, SwapClassification};
use crate::traits::SwapDirection;
use solana_protocols_macros::ProtocolInstruction;

use super::constants::{
    BUY_EXACT_IN_DISCRIMINATOR, BUY_EXACT_OUT_DISCRIMINATOR, SELL_EXACT_IN_DISCRIMINATOR,
    SELL_EXACT_OUT_DISCRIMINATOR,
};

/// Raydium Launchpad instruction enum.
#[derive(Debug, Clone, ProtocolInstruction)]
pub enum RaydiumLaunchpadInstruction {
    /// Buy with exact input (quote → base).
    #[instruction(discriminator = BUY_EXACT_IN_DISCRIMINATOR)]
    BuyExactIn(BuyExactInParams),
    /// Buy with exact output (quote → base).
    #[instruction(discriminator = BUY_EXACT_OUT_DISCRIMINATOR)]
    BuyExactOut(BuyExactOutParams),
    /// Sell with exact input (base → quote).
    #[instruction(discriminator = SELL_EXACT_IN_DISCRIMINATOR)]
    SellExactIn(SellExactInParams),
    /// Sell with exact output (base → quote).
    #[instruction(discriminator = SELL_EXACT_OUT_DISCRIMINATOR)]
    SellExactOut(SellExactOutParams),
}

impl RaydiumLaunchpadInstruction {
    /// Check if this is a buy instruction.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        matches!(
            self,
            RaydiumLaunchpadInstruction::BuyExactIn(_)
                | RaydiumLaunchpadInstruction::BuyExactOut(_)
        )
    }

    /// Check if this is a sell instruction.
    #[must_use]
    pub fn is_sell(&self) -> bool {
        matches!(
            self,
            RaydiumLaunchpadInstruction::SellExactIn(_)
                | RaydiumLaunchpadInstruction::SellExactOut(_)
        )
    }
}

impl ClassifiesAsSwap for RaydiumLaunchpadInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            RaydiumLaunchpadInstruction::BuyExactIn(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Exact(params.amount_in),
                SwapAmount::Minimum(params.minimum_amount_out),
            )),
            RaydiumLaunchpadInstruction::BuyExactOut(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Maximum(params.maximum_amount_in),
                SwapAmount::Exact(params.amount_out),
            )),
            RaydiumLaunchpadInstruction::SellExactIn(params) => Some(SwapClassification::new(
                SwapDirection::Sell,
                SwapAmount::Exact(params.amount_in),
                SwapAmount::Minimum(params.minimum_amount_out),
            )),
            RaydiumLaunchpadInstruction::SellExactOut(params) => Some(SwapClassification::new(
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

    fn make_buy_exact_in_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&BUY_EXACT_IN_DISCRIMINATOR);
        data.extend_from_slice(&500_000_000u64.to_le_bytes());
        data.extend_from_slice(&900_000u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes()); // share_fee_rate
        data
    }

    fn make_sell_exact_in_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SELL_EXACT_IN_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes());
        data.extend_from_slice(&400_000_000u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes()); // share_fee_rate
        data
    }

    #[test]
    fn try_from_slice_buy_exact_in() {
        let data = make_buy_exact_in_data();
        let ix = RaydiumLaunchpadInstruction::try_from_slice(&data).unwrap();
        assert!(ix.is_buy());
        assert!(!ix.is_sell());

        match ix {
            RaydiumLaunchpadInstruction::BuyExactIn(params) => {
                assert_eq!(params.amount_in, 500_000_000);
                assert_eq!(params.minimum_amount_out, 900_000);
            }
            RaydiumLaunchpadInstruction::BuyExactOut(_)
            | RaydiumLaunchpadInstruction::SellExactIn(_)
            | RaydiumLaunchpadInstruction::SellExactOut(_) => panic!("expected BuyExactIn"),
        }
    }

    #[test]
    fn try_from_slice_sell_exact_in() {
        let data = make_sell_exact_in_data();
        let ix = RaydiumLaunchpadInstruction::try_from_slice(&data).unwrap();
        assert!(!ix.is_buy());
        assert!(ix.is_sell());

        match ix {
            RaydiumLaunchpadInstruction::SellExactIn(params) => {
                assert_eq!(params.amount_in, 1_000_000);
                assert_eq!(params.minimum_amount_out, 400_000_000);
            }
            RaydiumLaunchpadInstruction::BuyExactIn(_)
            | RaydiumLaunchpadInstruction::BuyExactOut(_)
            | RaydiumLaunchpadInstruction::SellExactOut(_) => panic!("expected SellExactIn"),
        }
    }

    #[test]
    fn classifies_buy() {
        let data = make_buy_exact_in_data();
        let ix = RaydiumLaunchpadInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Buy);
    }

    #[test]
    fn classifies_sell() {
        let data = make_sell_exact_in_data();
        let ix = RaydiumLaunchpadInstruction::try_from_slice(&data).unwrap();
        let classification = ix.as_swap().expect("should classify as swap");
        assert_eq!(classification.direction, SwapDirection::Sell);
    }

    #[test]
    fn unknown_discriminator() {
        let data = vec![0xFFu8; 32];
        let result = RaydiumLaunchpadInstruction::try_from_slice(&data);
        assert!(result.is_err());
    }
}
