//! Pump.fun `sell_v2` — sell against the v2 account layout.
//!
//! Same pinned side and argument bytes as [`sell`](super::sell), separate
//! discriminator, separate file. Neither declares a trailing `track_volume`,
//! and 631 mainnet `sell_v2` instructions in one window carried none — but
//! "they happen to agree today" is not a reason to share a type, which is what
//! the `buy`/`buy_v2` pair demonstrated by *not* agreeing.
//!
//! # Accounts
//!
//! No account struct: the v2 layouts are variable (26/27/28/29 slots observed),
//! so identity is recovered from the `TradeEvent` rather than by slot index.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::InstructionData;

use super::super::constants::SELL_V2_DISCRIMINATOR;

/// Arguments for `sell_v2`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = SELL_V2_DISCRIMINATOR)]
pub struct SellV2Params {
    /// Tokens to sell — the pinned side.
    pub amount: u64,
    /// Minimum SOL to accept (slippage bound).
    pub min_sol_output: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn the_two_arguments_decode_in_order() {
        let mut d = 11u64.to_le_bytes().to_vec();
        d.extend_from_slice(&22u64.to_le_bytes());
        let p = SellV2Params::from_instruction_data(&d).expect("16 bytes");
        assert_eq!(p.amount, 11);
        assert_eq!(p.min_sol_output, 22);
    }
}
