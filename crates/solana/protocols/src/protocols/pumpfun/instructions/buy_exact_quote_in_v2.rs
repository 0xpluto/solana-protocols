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
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, OnchainInstruction};

use crate::parsing::accounts::Conditional;

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
    #[idl(
        undeclared = "senders emit a trailing track_volume that neither the vendored nor the live on-chain IDL declares; the bytes are in the fixtures"
    )]
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

/// Account list for pump.fun `buy_exact_quote_in_v2`.
///
/// # Why this exists after all
///
/// This file used to say "no account struct: the v2 layouts are variable". That
/// was the right observation and the wrong conclusion. The lists *are* variable
/// — 27 slots observed on mainnet — but every extra account is a **suffix**,
/// past the `event_authority` / `program` pair that terminates every Anchor
/// `emit_cpi!` instruction. Counted from the end nothing is safe; counted from
/// the start every named slot is exactly where the IDL puts it.
///
/// Settled from 63 recorded mainnet instructions (`fixtures/pumpfun/v2recs.json`):
/// every position that is constant in the shortest form holds the *same* value
/// at the *same* index in the longest, with zero disagreements.
///
/// # What `remaining` buys
///
/// Total accounting. The parser reads exactly the slots this instruction has —
/// no more, because the named fields stop at 27; no fewer, because everything
/// past them lands in `remaining` instead of being dropped on the floor.
///
/// The field names and their order are checked against the program's own IDL at
/// **compile time** by `#[idl(...)]`, so a mis-numbered slot is a build failure
/// rather than a wrong pubkey that looks like a right one.
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[onchain_ix(fixtures(
    "pumpfun/ix_buy_exact_quote_in_v2_n27.json",
    "pumpfun/ix_buy_exact_quote_in_v2_n28.json",
    "pumpfun/ix_buy_exact_quote_in_v2_n29.json"
))]
#[accounts(program_id = super::super::constants::PROGRAM_ID)]
#[idl(program = "pump", instruction = "buy_exact_quote_in_v2")]
pub struct BuyExactQuoteInV2Accounts {
    /// IDL slot 0.
    #[account]
    pub global: Pubkey,
    /// IDL slot 1.
    #[account]
    pub base_mint: Pubkey,
    /// IDL slot 2.
    #[account]
    pub quote_mint: Pubkey,
    /// IDL slot 3.
    #[account]
    pub base_token_program: Pubkey,
    /// IDL slot 4.
    #[account]
    pub quote_token_program: Pubkey,
    /// IDL slot 5.
    #[account]
    pub associated_token_program: Pubkey,
    /// IDL slot 6.
    #[account(writable)]
    pub fee_recipient: Pubkey,
    /// IDL slot 7.
    #[account(writable)]
    pub associated_quote_fee_recipient: Pubkey,
    /// IDL slot 8.
    #[account(writable)]
    pub buyback_fee_recipient: Pubkey,
    /// IDL slot 9.
    #[account(writable)]
    pub associated_quote_buyback_fee_recipient: Pubkey,
    /// IDL slot 10.
    #[account(writable)]
    pub bonding_curve: Pubkey,
    /// IDL slot 11.
    #[account(writable)]
    pub associated_base_bonding_curve: Pubkey,
    /// IDL slot 12.
    #[account(writable)]
    pub associated_quote_bonding_curve: Pubkey,
    /// IDL slot 13.
    #[account(writable, signer)]
    pub user: Pubkey,
    /// IDL slot 14.
    #[account(writable)]
    pub associated_base_user: Pubkey,
    /// IDL slot 15.
    #[account(writable)]
    pub associated_quote_user: Pubkey,
    /// IDL slot 16.
    #[account(writable)]
    pub creator_vault: Pubkey,
    /// IDL slot 17.
    #[account(writable)]
    pub associated_creator_vault: Pubkey,
    /// IDL slot 18.
    #[account]
    pub sharing_config: Pubkey,
    /// IDL slot 19.
    #[account]
    pub global_volume_accumulator: Pubkey,
    /// IDL slot 20.
    #[account(writable)]
    pub user_volume_accumulator: Pubkey,
    /// IDL slot 21.
    #[account(writable)]
    pub associated_user_volume_accumulator: Pubkey,
    /// IDL slot 22.
    #[account]
    pub fee_config: Pubkey,
    /// IDL slot 23.
    #[account]
    pub fee_program: Pubkey,
    /// IDL slot 24.
    #[account]
    pub system_program: Pubkey,
    /// IDL slot 25.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 26.
    #[account]
    pub program: Pubkey,
    /// A second copy of the cashback accumulator, appended past the declared
    /// list.
    ///
    /// `PDA(pumpfun, ["user_volume_accumulator", user])` — the *same* account
    /// this layout already declares as `user_volume_accumulator`. Some callers
    /// append it again, which is what pump's cashback docs ask for on the v1
    /// `sell` (which has no such slot) and which carries over here where the
    /// slot does exist. Resolved rather than ignored so it cannot be mistaken
    /// for a [`buyback_vaults`](Self::buyback_vaults) entry.
    #[account(resolved = super::super::accounts::derive_user_volume_accumulator_pda(&user))]
    pub appended_user_volume_accumulator: Conditional,
    /// The v2 bonding curve, appended to every buy and sell.
    ///
    /// `PDA(pumpfun, ["bonding-curve-v2", base_mint])`. Present on 33 of 33
    /// tailed mainnet instructions. Located by derivation because its index is
    /// not stable: first in 27, second in 6.
    #[account(resolved = super::super::accounts::derive_bonding_curve_v2_pda(&base_mint))]
    pub bonding_curve_v2: Conditional,
    /// Fee destinations from the `pump_fees` global roster.
    ///
    /// A `Vec` because this genuinely is a list and a caller picks a subset:
    /// eight `BuybackVault` accounts exist on chain, one direct call passed all
    /// eight, five routed `sell`s passed one. Not derivable from the
    /// instruction, so unlike the two above it cannot be resolved by name.
    #[account(
        remaining,
        reason = "the pump_fees BuybackVault roster: eight exist on chain and a \
                  caller credits any subset (one direct call passed all eight, five \
                  routed sells passed one), so which ones appear is caller policy \
                  rather than layout, and none is derivable from this instruction"
    )]
    pub buyback_vaults: Vec<Pubkey>,
}

impl crate::pairs::NamesPair for BuyExactQuoteInV2Accounts {
    fn pair(
        &self,
    ) -> (
        solana_program::pubkey::Pubkey,
        solana_program::pubkey::Pubkey,
    ) {
        (self.base_mint, self.quote_mint)
    }
}

impl crate::pairs::SwapAccounts for BuyExactQuoteInV2Accounts {
    fn pool(&self) -> solana_program::pubkey::Pubkey {
        self.bonding_curve
    }
}
