//! Pump.fun `buy_v2` — exact-out buy against the v2 account layout.
//!
//! Same pinned side and same argument bytes as [`buy`](super::buy), and a
//! separate type anyway: they are separate discriminators, and the IDL declares
//! them differently. `buy` takes a trailing `track_volume`; this does not.
//!
//! That difference is real, not pedantic. Measured over 208 mainnet `buy_v2`
//! instructions in one window: **none** carried a trailing byte, while its
//! sibling `buy_exact_quote_in_v2` carried one on 150 of 621. A single shared
//! params struct answered "does this instruction take the flag" the same way
//! for both, which is wrong for one of them.
//!
//! # Accounts
//!
//! Deliberately no account struct. The v2 layouts are *variable* — 26/27/28/29
//! slots observed on mainnet 2026-08-12 — so no fixed index is safe. Identity
//! comes from the `TradeEvent` (mint and user) with the bonding curve derived
//! as a PDA of the mint, which cannot be broken by the program adding a slot.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::InstructionData;

use super::super::constants::BUY_V2_DISCRIMINATOR;

/// Arguments for `buy_v2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, InstructionData)]
#[instruction_data(discriminator = BUY_V2_DISCRIMINATOR)]
pub struct BuyV2Params {
    /// Tokens to receive — the pinned side.
    pub amount: u64,
    /// Maximum SOL to spend (slippage bound).
    pub max_sol_cost: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn the_two_arguments_decode_in_order() {
        let mut data = 7u64.to_le_bytes().to_vec();
        data.extend_from_slice(&9u64.to_le_bytes());
        let p = BuyV2Params::from_instruction_data(&data).expect("16 bytes");
        assert_eq!(p.amount, 7);
        assert_eq!(p.max_sol_cost, 9);
    }

    /// `buy_v2` pins the tokens delivered, not the SOL spent. Reading it as an
    /// exact-in buy inverts every quote built from it.
    #[test]
    fn short_data_is_refused() {
        assert!(BuyV2Params::from_instruction_data(&[0u8; 15]).is_err());
    }
}
