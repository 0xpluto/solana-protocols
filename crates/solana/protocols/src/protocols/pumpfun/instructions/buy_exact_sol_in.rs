//! Pump.fun `buy_exact_sol_in` — exact-in buy against the v1 account layout.
//!
//! The mirror image of [`buy`](super::buy): same 16 accounts, same two `u64`s
//! on the wire, opposite pinned side. `buy` pins the tokens it delivers; this
//! pins the SOL it spends. A shared struct would name the pinned side wrong for
//! half its uses, which is exactly the confusion that makes a quoter "close but
//! never exact".

use serde::{Deserialize, Serialize};
use solana_protocols_macros::InstructionData;

use super::super::constants::BUY_EXACT_SOL_IN_DISCRIMINATOR;
use crate::protocols::OptionBool;

/// Arguments for `buy_exact_sol_in`.
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
#[instruction_data(discriminator = BUY_EXACT_SOL_IN_DISCRIMINATOR, fixtures(
    "pumpfun/ix_buy_exact_sol_in_n18.json",
    "pumpfun/ix_buy_exact_sol_in_n19.json"
), idl(program = "pump", instruction = "buy_exact_sol_in"))]
pub struct BuyExactSolInParams {
    /// SOL the trader spends — the pinned side.
    pub spendable_sol_in: u64,
    /// Minimum tokens to accept (slippage bound).
    pub min_tokens_out: u64,
    /// Trailing `track_volume`, which the IDL declares for this instruction.
    ///
    /// Load-bearing rather than cosmetic: setting it changes which accounts the
    /// program expects (the volume accumulators), so a builder that drops it can
    /// emit an instruction whose accounts contradict its own arguments. See
    /// [`OptionBool`] for why the encoding is not `Option<bool>`, and why an
    /// unattributable trailer is kept rather than refused — this is the
    /// instruction that surfaced that case on mainnet.
    pub track_volume: OptionBool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    fn args(trailer: &[u8]) -> Vec<u8> {
        let mut d = 1_000_000u64.to_le_bytes().to_vec();
        d.extend_from_slice(&1u64.to_le_bytes());
        d.extend_from_slice(trailer);
        d
    }

    #[test]
    fn all_three_declared_forms_of_the_flag_decode() {
        for (trailer, want) in [
            (&[][..], OptionBool::None),
            (&[0][..], OptionBool::SomeFalse),
            (&[1][..], OptionBool::SomeTrue),
        ] {
            let p = BuyExactSolInParams::from_instruction_data(&args(trailer)).expect("decodes");
            assert_eq!(p.spendable_sol_in, 1_000_000);
            assert_eq!(p.min_tokens_out, 1);
            assert_eq!(p.track_volume, want, "trailer {trailer:?}");
        }
    }

    /// The exact bytes a mainnet sender emitted on 2026-08-15: eight zero bytes
    /// where the IDL declares one. The program accepted it, so the decoder must,
    /// and the flag stays unresolved rather than being read as `false`.
    #[test]
    fn the_observed_eight_byte_trailer_decodes_without_resolving_the_flag() {
        let p = BuyExactSolInParams::from_instruction_data(&args(&[0u8; 8])).expect("decodes");
        assert_eq!(p.track_volume.unattributed(), Some([0u8; 8].as_slice()));
        assert_eq!(p.track_volume.requested(), None);
    }
}
