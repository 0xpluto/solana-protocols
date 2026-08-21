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

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;

use crate::parsing::accounts::Conditional;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};

use super::super::constants::BUY_V2_DISCRIMINATOR;

/// Arguments for `buy_v2`.
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
#[instruction_data(discriminator = BUY_V2_DISCRIMINATOR, fixtures(
    "pumpfun/ix_buy_v2_n27.json",
    "pumpfun/ix_buy_v2_n28.json",
    "pumpfun/ix_buy_v2_n29.json"
), idl(program = "pump", instruction = "buy_v2"))]
pub struct BuyV2Params {
    /// Tokens to receive — the pinned side.
    pub amount: u64,
    /// Maximum SOL to spend (slippage bound).
    pub max_sol_cost: u64,
}

impl crate::pairs::NamesPair for BuyV2Accounts {
    fn pair(
        &self,
    ) -> (
        solana_program::pubkey::Pubkey,
        solana_program::pubkey::Pubkey,
    ) {
        (self.base_mint, self.quote_mint)
    }
}

/// v2 names both sides, so a curve quoted in something other than SOL is read
/// correctly instead of being labelled SOL.
impl crate::pairs::SwapAccounts for BuyV2Accounts {
    fn pool(&self) -> solana_program::pubkey::Pubkey {
        self.bonding_curve
    }
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

/// Accounts for `buy_v2` — 27 named slots, then any remaining.
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
    "pumpfun/ix_buy_v2_n27.json",
    "pumpfun/ix_buy_v2_n28.json",
    "pumpfun/ix_buy_v2_n29.json"
))]
#[accounts(program_id = super::super::constants::PROGRAM_ID)]
#[idl(program = "pump", instruction = "buy_v2")]
pub struct BuyV2Accounts {
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
