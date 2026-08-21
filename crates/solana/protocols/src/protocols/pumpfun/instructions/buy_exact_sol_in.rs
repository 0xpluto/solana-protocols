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
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, BuildAccounts, OnchainInstruction};

use super::super::constants::{
    BONDING_CURVE_SEED, BONDING_CURVE_V2_SEED, EVENT_AUTHORITY_PDA, FEE_CONFIG_PDA, GLOBAL_PDA,
    GLOBAL_VOLUME_ACCUMULATOR_PDA, PROGRAM_ID, PUMP_FEES_PROGRAM_ID, USER_VOLUME_ACCUMULATOR_SEED,
};
use crate::parsing::accounts::Conditional;

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

/// Account list for pump.fun `buy_exact_sol_in`.
///
/// `#[derive(AccountMetas)]` generates `to_account_metas()`/`from_pubkeys()`;
/// `#[derive(BuildAccounts)]` generates `derive(mint, user, creator_vault)` from
/// the per-field `#[build(...)]` derivations and a replay test that rebuilds a
/// real on-chain buy from its own accounts (every PDA/ATA must match the chain).
/// `creator_vault` is an `input` rather than a `pda` because its seed (the coin
/// creator) is not present in the instruction — the self-contained replay can't
/// recover it, so we take it as given.
#[derive(Debug, Clone, AccountMetas, BuildAccounts, OnchainInstruction)]
#[idl(program = "pump", instruction = "buy_exact_sol_in")]
#[build(
    unreplayed = "both captured buy_exact_sol_in instructions pass a non-canonical associated_user -- a token account that is not the ATA of (user, mint), which the program accepts -- so the replay would derive that slot and report a builder defect where there is none. The slots are right: associated_bonding_curve, an ATA over the same mint and token program, does reproduce. Needs a capture whose buyer used a plain ATA"
)]
#[onchain_ix(fixtures(
    "pumpfun/ix_buy_exact_sol_in_n18.json",
    "pumpfun/ix_buy_exact_sol_in_n19.json"
))]
pub struct BuyExactSolInAccounts {
    /// Global state PDA.
    #[account]
    #[build(key = GLOBAL_PDA)]
    pub global: Pubkey,
    /// Fee collector: any recipient the Global config lists. Input, not a
    /// const — the config rotates, and real senders spread across the arrays
    /// (the fixture buy used `reserved_fee_recipient`, not `fee_recipients\[0\]`).
    #[account(writable)]
    #[build(input)]
    pub fee_recipient: Pubkey,
    /// Token mint.
    #[account]
    #[build(input)]
    pub mint: Pubkey,
    /// Bonding curve PDA.
    #[account(writable)]
    #[build(pda(program = PROGRAM_ID, seeds(BONDING_CURVE_SEED, mint)))]
    pub bonding_curve: Pubkey,
    /// Associated bonding curve token account (under the mint's token program).
    #[account(writable)]
    #[build(ata(owner = bonding_curve, mint = mint, program = token_program))]
    pub associated_bonding_curve: Pubkey,
    /// User's associated token account (under the mint's token program).
    #[account(writable)]
    #[build(ata(owner = user, mint = mint, program = token_program))]
    pub associated_user: Pubkey,
    /// User wallet (signer).
    #[account(writable, signer)]
    #[build(input)]
    pub user: Pubkey,
    /// System program.
    #[account]
    #[build(key = solana_program::system_program::id())]
    pub system_program: Pubkey,
    /// Token program owning the mint: classic SPL or Token-2022. Input — read
    /// the mint account's owner at runtime; it also changes both ATAs above.
    #[account]
    #[build(input)]
    pub token_program: Pubkey,
    /// Creator vault (seeded by the coin creator, which the instruction does
    /// not carry — so an input, from the parsed bonding curve).
    #[account(writable)]
    #[build(input)]
    pub creator_vault: Pubkey,
    /// Event authority PDA.
    #[account]
    #[build(key = EVENT_AUTHORITY_PDA)]
    pub event_authority: Pubkey,
    /// Pumpfun program ID.
    #[account]
    #[build(key = PROGRAM_ID)]
    pub program: Pubkey,
    /// Global volume accumulator PDA.
    #[account]
    #[build(key = GLOBAL_VOLUME_ACCUMULATOR_PDA)]
    pub global_volume_accumulator: Pubkey,
    /// User volume accumulator PDA (of the pumpfun program itself).
    #[account(writable)]
    #[build(pda(program = PROGRAM_ID, seeds(USER_VOLUME_ACCUMULATOR_SEED, user)))]
    pub user_volume_accumulator: Pubkey,
    /// Fee config PDA (owned by the pump_fees program).
    #[account]
    #[build(key = FEE_CONFIG_PDA)]
    pub fee_config: Pubkey,
    /// The pump_fees program.
    #[account]
    #[build(key = PUMP_FEES_PROGRAM_ID)]
    pub fee_program: Pubkey,

    /// The cashback volume accumulator, when the coin has cashback enabled.
    ///
    /// `PDA(pumpfun, ["user_volume_accumulator", user])`. Pump's cashback docs
    /// name it as the 0th appended account on `sell`, and mainnet agrees — it is
    /// present on the 17/18/19-account sells and absent below.
    #[account(resolved = super::super::accounts::derive_user_volume_accumulator_pda(&user))]
    #[build(optional, pda(program = PROGRAM_ID, seeds(USER_VOLUME_ACCUMULATOR_SEED, user)))]
    pub appended_user_volume_accumulator: Conditional,
    /// The v2 bonding curve, appended to every buy and sell.
    ///
    /// `PDA(pumpfun, ["bonding-curve-v2", mint])`. Located by derivation, not by
    /// index: it sits at tail 0 when no accumulator precedes it and tail 1 when
    /// one does.
    #[account(resolved = super::super::accounts::derive_bonding_curve_v2_pda(&mint))]
    #[build(pda(program = PROGRAM_ID, seeds(BONDING_CURVE_V2_SEED, mint)))]
    pub bonding_curve_v2: Conditional,
    /// Fee destinations from the `pump_fees` global roster.
    #[account(
        remaining,
        reason = "the pump_fees BuybackVault roster: eight exist on chain and a \
                  caller credits any subset, so which ones appear is caller policy \
                  rather than layout, and none is derivable from this instruction"
    )]
    pub buyback_vaults: Vec<Pubkey>,
}

impl crate::pairs::NamesPair for BuyExactSolInAccounts {
    fn pair(
        &self,
    ) -> (
        solana_program::pubkey::Pubkey,
        solana_program::pubkey::Pubkey,
    ) {
        (self.mint, crate::tokens::WSOL)
    }
}

impl crate::pairs::SwapAccounts for BuyExactSolInAccounts {
    fn pool(&self) -> solana_program::pubkey::Pubkey {
        self.bonding_curve
    }
}
