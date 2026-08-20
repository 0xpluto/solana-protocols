//! Pump.fun `buy_exact_quote_in_v2` — exact-in buy against the v2 layout.
//!
//! Exact-in like [`buy_exact_sol_in`](super::buy_exact_sol_in), but v2 supports
//! non-SOL quote mints, so the pinned quantity is named *quote* rather than
//! *sol*. Same role, different denomination assumption.
//!
//! # The undeclared flag
//!
//! **Neither the vendored nor the live on-chain IDL declares `track_volume` for
//! this instruction, and it is sent anyway.** Measured 2026-08-12 over 1,050 v2
//! instructions: 362 arrived at 24 bytes with 27 accounts, 113 at 25 bytes with
//! 28 accounts, and the correlation between the trailing byte and the extra
//! account was perfect. A later window put it at 150 of 621.
//!
//! So the field is here on evidence, not on the IDL. Rejecting the trailing byte
//! would drop ~11% of these instructions; ignoring it would lose the flag while
//! leaving the account list unexplained. Its sibling `buy_v2` genuinely does not
//! send one — which is why these are two files.

use serde::{Deserialize, Serialize};
use solana_protocols_macros::InstructionData;

use super::super::constants::BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR;
use crate::protocols::OptionBool;

/// Arguments for `buy_exact_quote_in_v2`.
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
#[instruction_data(discriminator = BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR, fixtures(
    "pumpfun/ix_buy_exact_quote_in_v2_n27.json",
    "pumpfun/ix_buy_exact_quote_in_v2_n28.json",
    "pumpfun/ix_buy_exact_quote_in_v2_n29.json"
), idl(program = "pump", instruction = "buy_exact_quote_in_v2"))]
pub struct BuyExactQuoteInV2Params {
    /// Quote the trader spends — the pinned side.
    pub spendable_quote_in: u64,
    /// Minimum tokens to accept (slippage bound).
    pub min_tokens_out: u64,
    /// Trailing `track_volume` the IDL does not declare — see the module docs.
    #[idl(undeclared = "senders emit a trailing track_volume that neither the vendored nor the live on-chain IDL declares; the bytes are in the fixtures")]
    pub track_volume: OptionBool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    /// Both observed sizes decode: 24 bytes with the flag absent, 25 with it
    /// set. Dropping the 25-byte form would lose about one in nine.
    #[test]
    fn both_observed_wire_sizes_decode() {
        let mut bare = 5u64.to_le_bytes().to_vec();
        bare.extend_from_slice(&6u64.to_le_bytes());
        assert_eq!(bare.len(), 16);
        let p = BuyExactQuoteInV2Params::from_instruction_data(&bare).expect("24-byte form");
        assert_eq!(p.track_volume, OptionBool::None);

        let mut flagged = bare.clone();
        flagged.push(1);
        let p = BuyExactQuoteInV2Params::from_instruction_data(&flagged).expect("25-byte form");
        assert_eq!(p.track_volume, OptionBool::SomeTrue);
        assert_eq!(p.spendable_quote_in, 5);
        assert_eq!(p.min_tokens_out, 6);
    }
}
