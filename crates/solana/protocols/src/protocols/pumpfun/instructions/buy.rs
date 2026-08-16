//! Pump.fun buy instruction builder.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, BuildAccounts, OnchainInstruction};
use solana_sdk::instruction::Instruction;

use crate::error::Result;
use crate::parsing::{FromInstructionData, InstructionParseError};
use crate::traits::InstructionBuilder;

use super::super::accounts::PumpfunKeys;
use super::super::constants::{
    BONDING_CURVE_SEED, BUY_DISCRIMINATOR, EVENT_AUTHORITY_PDA, FEE_COLLECTOR, FEE_CONFIG_PDA,
    GLOBAL_PDA, GLOBAL_VOLUME_ACCUMULATOR_PDA, PROGRAM_ID, PUMP_FEES_PROGRAM_ID,
    USER_VOLUME_ACCUMULATOR_SEED,
};

/// Account list for pump.fun buy instruction.
///
/// `#[derive(AccountMetas)]` generates `to_account_metas()`/`from_pubkeys()`;
/// `#[derive(BuildAccounts)]` generates `derive(mint, user, creator_vault)` from
/// the per-field `#[build(...)]` derivations and a replay test that rebuilds a
/// real on-chain buy from its own accounts (every PDA/ATA must match the chain).
/// `creator_vault` is an `input` rather than a `pda` because its seed (the coin
/// creator) is not present in the instruction — the self-contained replay can't
/// recover it, so we take it as given.
#[derive(Debug, Clone, AccountMetas, BuildAccounts, OnchainInstruction)]
#[idl(program = "pump", instruction = "buy")]
#[build(fixture = "pumpfun/ix_buy.json")]
#[onchain_ix(fixture = "pumpfun/ix_buy.json")]
pub struct BuyAccounts {
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
}

impl BuyAccounts {
    /// Create buy accounts from keys and user with **classic-SPL defaults**:
    /// `token_program = spl_token::id()` and the const [`FEE_COLLECTOR`]
    /// (`Global.fee_recipient`). Correct for classic mints; a Token-2022 mint or
    /// a config-aware recipient choice needs [`Self::derive`] with the runtime
    /// values (mint owner + cached Global config).
    #[must_use]
    pub fn new(keys: &PumpfunKeys, user: &Pubkey) -> Self {
        Self::derive(
            FEE_COLLECTOR,
            keys.mint,
            *user,
            spl_token::id(),
            keys.creator_vault,
        )
    }
}

/// Parameters for a buy instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyParams {
    /// Amount of tokens to receive.
    pub amount: u64,
    /// Maximum SOL to spend (slippage protection).
    pub max_sol_cost: u64,
}

impl BuyParams {
    /// Create new buy parameters.
    #[must_use]
    pub fn new(amount: u64, max_sol_cost: u64) -> Self {
        BuyParams {
            amount,
            max_sol_cost,
        }
    }

    /// Create buy parameters from swap output with slippage.
    ///
    /// # Arguments
    /// * `tokens_out` - Expected tokens from swap calculation
    /// * `sol_in` - SOL amount from swap calculation
    /// * `slippage_bps` - Slippage tolerance in basis points
    #[must_use]
    pub fn from_swap_output(tokens_out: u64, sol_in: u64, slippage_bps: u16) -> Self {
        // Apply slippage to get minimum acceptable tokens and max SOL
        let min_tokens = (tokens_out as u128 * (10000 - slippage_bps as u128) / 10000) as u64;
        let max_sol = (sol_in as u128 * (10000 + slippage_bps as u128) / 10000) as u64;

        BuyParams {
            amount: min_tokens,
            max_sol_cost: max_sol,
        }
    }

    /// Serialize to instruction data.
    #[must_use]
    pub fn to_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&BUY_DISCRIMINATOR);
        data.extend_from_slice(&self.amount.to_le_bytes());
        data.extend_from_slice(&self.max_sol_cost.to_le_bytes());
        data
    }
}

/// Params for pumpfun's exact-IN buys (`buy_exact_sol_in`,
/// `buy_exact_quote_in_v2`).
///
/// A distinct type from [`BuyParams`] on purpose. The byte layout is the same
/// two `u64`s, but the *meaning* is inverted: `buy` pins the tokens it
/// delivers, these pin the SOL they spend. Sharing one struct would name the
/// pinned side wrong for half its uses, which is precisely the confusion that
/// makes a quoter "close but never exact".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuyExactInParams {
    /// SOL/quote the trader spends — the pinned side.
    pub spendable_in: u64,
    /// Minimum tokens to accept.
    pub min_tokens_out: u64,
}

impl FromInstructionData for BuyExactInParams {
    fn from_instruction_data(data: &[u8]) -> std::result::Result<Self, InstructionParseError> {
        if data.len() < 16 {
            return Err(InstructionParseError::DeserializationFailed(format!(
                "BuyExactInParams: expected 16 bytes, got {}",
                data.len()
            )));
        }
        Ok(Self {
            spendable_in: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            min_tokens_out: u64::from_le_bytes(data[8..16].try_into().unwrap()),
        })
    }
}

impl FromInstructionData for BuyParams {
    fn from_instruction_data(data: &[u8]) -> std::result::Result<Self, InstructionParseError> {
        if data.len() < 16 {
            return Err(InstructionParseError::DeserializationFailed(format!(
                "BuyParams: expected 16 bytes, got {}",
                data.len()
            )));
        }
        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let max_sol_cost = u64::from_le_bytes(data[8..16].try_into().unwrap());
        Ok(BuyParams {
            amount,
            max_sol_cost,
        })
    }
}

// FromAccountKeys for BuyAccounts — auto-generated by #[derive(AccountMetas)] macro

/// Builder for pump.fun buy instructions.
///
/// Implements [`InstructionBuilder`] trait for consistent interface.
pub struct BuyBuilder;

impl InstructionBuilder for BuyBuilder {
    type Keys = PumpfunKeys;
    type Params = BuyParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> Result<Instruction> {
        let accounts = BuyAccounts::new(keys, user);

        Ok(Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts.to_account_metas(),
            data: params.to_data(),
        })
    }

    fn build_swap_with_setup(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
        create_ata: bool,
    ) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::with_capacity(2);

        if create_ata {
            let ata_ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                user,
                user,
                &keys.mint,
                &spl_token::id(),
            );
            instructions.push(ata_ix);
        }

        instructions.push(Self::build_swap_instruction(keys, user, params)?);

        Ok(instructions)
    }
}

impl BuyBuilder {
    /// Build a buy with runtime-resolved values: the mint's owning token
    /// program (classic vs Token-2022 — changes both ATAs) and the fee
    /// collector (any recipient the live Global config lists). This is the
    /// correct path for arbitrary mints; [`Self::build`] assumes classic SPL.
    #[must_use]
    pub fn build_with(
        keys: &PumpfunKeys,
        user: &Pubkey,
        token_program: crate::tokens::TokenProgram,
        fee_recipient: Pubkey,
        token_amount: u64,
        max_sol_cost: u64,
    ) -> Instruction {
        let accounts = BuyAccounts::derive(
            fee_recipient,
            keys.mint,
            *user,
            token_program.id(),
            keys.creator_vault,
        );
        Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts.to_account_metas(),
            data: BuyParams::new(token_amount, max_sol_cost).to_data(),
        }
    }

    /// Convenience method to build buy instruction directly.
    #[must_use]
    pub fn build(
        keys: &PumpfunKeys,
        user: &Pubkey,
        token_amount: u64,
        max_sol_cost: u64,
    ) -> Instruction {
        Self::build_swap_instruction(keys, user, BuyParams::new(token_amount, max_sol_cost))
            .expect("BuyBuilder::build_swap_instruction should not fail")
    }

    /// Build buy instruction with ATA creation if needed.
    #[must_use]
    pub fn build_with_ata(
        keys: &PumpfunKeys,
        user: &Pubkey,
        token_amount: u64,
        max_sol_cost: u64,
    ) -> Vec<Instruction> {
        Self::build_swap_with_setup(keys, user, BuyParams::new(token_amount, max_sol_cost), true)
            .expect("BuyBuilder::build_swap_with_setup should not fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_params_serialization() {
        let params = BuyParams::new(1_000_000_000, 100_000_000);
        let data = params.to_data();

        assert_eq!(&data[0..8], &BUY_DISCRIMINATOR);
        assert_eq!(data.len(), 24);

        // Parse back
        let amount = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let max_cost = u64::from_le_bytes(data[16..24].try_into().unwrap());

        assert_eq!(amount, 1_000_000_000);
        assert_eq!(max_cost, 100_000_000);
    }

    #[test]
    fn buy_params_from_swap_output() {
        let params = BuyParams::from_swap_output(1_000_000_000, 100_000_000, 100); // 1% slippage

        // Min tokens should be 99% of expected
        assert_eq!(params.amount, 990_000_000);
        // Max SOL should be 101% of expected
        assert_eq!(params.max_sol_cost, 101_000_000);
    }

    #[test]
    fn buy_builder_creates_instruction() {
        let mint = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let creator = Pubkey::new_unique();

        let keys = PumpfunKeys::new(mint, creator);
        let ix = BuyBuilder::build(&keys, &user, 1_000_000_000, 100_000_000);

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert!(!ix.accounts.is_empty());
        assert!(!ix.data.is_empty());

        // Check user is a signer
        let user_account = ix.accounts.iter().find(|a| a.pubkey == user);
        assert!(user_account.is_some());
        assert!(user_account.unwrap().is_signer);
    }

    #[test]
    fn buy_with_ata_creates_two_instructions() {
        let mint = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let creator = Pubkey::new_unique();

        let keys = PumpfunKeys::new(mint, creator);
        let instructions = BuyBuilder::build_with_ata(&keys, &user, 1_000_000_000, 100_000_000);

        assert_eq!(instructions.len(), 2);

        // First should be ATA creation
        assert_eq!(
            instructions[0].program_id,
            spl_associated_token_account::id()
        );

        // Second should be the buy
        assert_eq!(instructions[1].program_id, PROGRAM_ID);
    }
}
