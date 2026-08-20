//! Pump.fun sell instruction builder.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;

use crate::protocols::OptionBool;

use crate::parsing::accounts::Conditional;
use solana_protocols_macros::{AccountMetas, BuildAccounts, InstructionData, OnchainInstruction};
use solana_sdk::instruction::Instruction;

use crate::error::Result;
use crate::traits::InstructionBuilder;

use super::super::accounts::PumpfunKeys;
use super::super::constants::{
    BONDING_CURVE_SEED, EVENT_AUTHORITY_PDA, FEE_COLLECTOR, FEE_CONFIG_PDA, GLOBAL_PDA, PROGRAM_ID,
    PUMP_FEES_PROGRAM_ID, SELL_DISCRIMINATOR,
    USER_VOLUME_ACCUMULATOR_SEED,
    BONDING_CURVE_V2_SEED,
};

/// Account list for pump.fun sell instruction.
///
/// Same derive stack as [`BuyAccounts`](super::buy::BuyAccounts): `AccountMetas`
/// (metas + `from_pubkeys`), `BuildAccounts` (`derive()` + build replay against a
/// real landed sell), `OnchainInstruction` (parse-side account-order round-trip).
/// Note sell has no volume-accumulator accounts, and `creator_vault` precedes
/// `token_program` (opposite of buy) — pinned by the fixture.
#[derive(Debug, Clone, AccountMetas, BuildAccounts, OnchainInstruction)]
#[idl(program = "pump", instruction = "sell")]
#[build(fixture = "pumpfun/ix_sell.json")]
#[onchain_ix(fixtures(
    "pumpfun/ix_sell.json",
    "pumpfun/ix_sell_n16.json",
    "pumpfun/ix_sell_n17.json",
    "pumpfun/ix_sell_n18.json",
    "pumpfun/ix_sell_n19.json"
))]
pub struct SellAccounts {
    /// Global state PDA.
    #[account]
    #[build(key = GLOBAL_PDA)]
    pub global: Pubkey,
    /// Fee collector: any recipient the Global config lists (the fixture sell
    /// used `fee_recipients\[0\]`; the fixture buy used `reserved_fee_recipient`).
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
    /// Creator vault (seeded by the coin creator — not in the instruction).
    #[account(writable)]
    #[build(input)]
    pub creator_vault: Pubkey,
    /// Token program owning the mint (classic SPL or Token-2022).
    #[account]
    #[build(input)]
    pub token_program: Pubkey,
    /// Event authority PDA.
    #[account]
    #[build(key = EVENT_AUTHORITY_PDA)]
    pub event_authority: Pubkey,
    /// Pumpfun program ID.
    #[account]
    #[build(key = PROGRAM_ID)]
    pub program: Pubkey,
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

impl SellAccounts {
    /// Create sell accounts from keys and user with **classic-SPL defaults**
    /// (`spl_token::id()` + the const [`FEE_COLLECTOR`]). Token-2022 mints and
    /// config-aware recipient picks go through [`Self::derive`].
    #[must_use]
    pub fn new(keys: &PumpfunKeys, user: &Pubkey) -> Self {
        Self::derive(
            FEE_COLLECTOR,
            keys.mint,
            *user,
            keys.creator_vault,
            spl_token::id(),
        )
    }
}

/// Parameters for a sell instruction.
///
/// Decode and encode both come from the derive, which means borsh. This struct
/// hand-rolled both: `from_le_bytes` at literal offsets behind a `data.len() < 16`
/// check — a *minimum*, so trailing bytes were ignored rather than refused, which
/// is exactly how an undeclared `track_volume` rode along on a sibling instruction
/// unnoticed. It also meant no generated fixture test could exist here, because
/// the struct owned its own decoder.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = SELL_DISCRIMINATOR, fixtures(
    "pumpfun/ix_sell.json",
    "pumpfun/ix_sell_n16.json",
    "pumpfun/ix_sell_n17.json",
    "pumpfun/ix_sell_n18.json",
    "pumpfun/ix_sell_n19.json"
), idl(program = "pump", instruction = "sell"))]
pub struct SellParams {
    /// Amount of tokens to sell.
    pub amount: u64,
    /// Minimum SOL to receive (slippage protection).
    pub min_sol_output: u64,
    /// Trailing `track_volume` that no IDL declares.
    ///
    /// Neither the vendored nor the live on-chain IDL lists an argument here,
    /// and senders emit one anyway — `[1, 1]`, borsh's `Option<bool>` encoding
    /// of `Some(true)`, on `ix_sell_n16` and `ix_sell_n17`. Three other captured
    /// sells send nothing, which is why the type is
    /// [`OptionBool`](crate::protocols::OptionBool) rather than a `bool`: the
    /// argument's *width* is what varies, and absent is not false.
    ///
    /// The hand-rolled decoder this replaced checked `data.len() < 16` — a
    /// minimum — so it read the first sixteen bytes and discarded these two
    /// without a word. Refusing them is what surfaced them.
    #[idl(undeclared = "senders emit a trailing track_volume that neither the vendored nor the live on-chain IDL declares; the bytes are in the fixtures")]
    pub track_volume: OptionBool,
}

impl SellParams {
    /// Create new sell parameters.
    #[must_use]
    pub fn new(amount: u64, min_sol_output: u64) -> Self {
        SellParams {
            amount,
            min_sol_output,
            // We send no flag. Absent is what the majority of captured sells
            // carry, and it is the only value we can pick honestly: opting a
            // trade into volume tracking is the caller's decision, not a default.
            track_volume: OptionBool::None,
        }
    }

    /// Create sell parameters from swap output with slippage.
    ///
    /// # Arguments
    /// * `tokens_in` - Tokens to sell
    /// * `sol_out` - Expected SOL from swap calculation
    /// * `slippage_bps` - Slippage tolerance in basis points
    #[must_use]
    pub fn from_swap_output(tokens_in: u64, sol_out: u64, slippage_bps: u16) -> Self {
        // Apply slippage to get minimum acceptable SOL
        let min_sol = (sol_out as u128 * (10000 - slippage_bps as u128) / 10000) as u64;

        SellParams {
            amount: tokens_in,
            min_sol_output: min_sol,
            track_volume: OptionBool::None,
        }
    }

}



// FromAccountKeys for SellAccounts — auto-generated by #[derive(AccountMetas)] macro

/// Builder for pump.fun sell instructions.
///
/// Implements [`InstructionBuilder`] trait for consistent interface.
pub struct SellBuilder;

impl InstructionBuilder for SellBuilder {
    type Keys = PumpfunKeys;
    type Params = SellParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> Result<Instruction> {
        let accounts = SellAccounts::new(keys, user);

        Ok(Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts.to_account_metas(),
            data: params.to_data(),
        })
    }
}

impl SellBuilder {
    /// Build a sell with runtime-resolved values: the mint's owning token
    /// program (changes both ATAs) and the fee collector (any recipient the
    /// live Global config lists). [`Self::build`] assumes classic SPL.
    #[must_use]
    pub fn build_with(
        keys: &PumpfunKeys,
        user: &Pubkey,
        token_program: crate::tokens::TokenProgram,
        fee_recipient: Pubkey,
        token_amount: u64,
        min_sol_output: u64,
    ) -> Instruction {
        let accounts = SellAccounts::derive(
            fee_recipient,
            keys.mint,
            *user,
            keys.creator_vault,
            token_program.id(),
        );
        Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts.to_account_metas(),
            data: SellParams::new(token_amount, min_sol_output).to_data(),
        }
    }

    /// Convenience method to build sell instruction directly.
    #[must_use]
    pub fn build(
        keys: &PumpfunKeys,
        user: &Pubkey,
        token_amount: u64,
        min_sol_output: u64,
    ) -> Instruction {
        Self::build_swap_instruction(keys, user, SellParams::new(token_amount, min_sol_output))
            .expect("SellBuilder::build_swap_instruction should not fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sell_params_serialization() {
        let params = SellParams::new(1_000_000_000, 50_000_000);
        let data = params.to_data();

        assert_eq!(&data[0..8], &SELL_DISCRIMINATOR);
        assert_eq!(data.len(), 24);

        // Parse back
        let amount = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let min_output = u64::from_le_bytes(data[16..24].try_into().unwrap());

        assert_eq!(amount, 1_000_000_000);
        assert_eq!(min_output, 50_000_000);
    }

    #[test]
    fn sell_params_from_swap_output() {
        let params = SellParams::from_swap_output(1_000_000_000, 100_000_000, 100); // 1% slippage

        // Tokens to sell stays the same
        assert_eq!(params.amount, 1_000_000_000);
        // Min SOL should be 99% of expected
        assert_eq!(params.min_sol_output, 99_000_000);
    }

    #[test]
    fn sell_builder_creates_instruction() {
        let mint = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let creator = Pubkey::new_unique();

        let keys = PumpfunKeys::new(mint, creator);
        let ix = SellBuilder::build(&keys, &user, 1_000_000_000, 50_000_000);

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert!(!ix.accounts.is_empty());
        assert!(!ix.data.is_empty());

        // Check user is a signer
        let user_account = ix.accounts.iter().find(|a| a.pubkey == user);
        assert!(user_account.is_some());
        assert!(user_account.unwrap().is_signer);
    }
}
