//! `add_liquidity_by_weight` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`AddLiquidityByWeightIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`AddLiquidityByWeightIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::AddLiquidityByWeightInstructionArgs;
pub use meteora_dlmm_sdk::instructions::ADD_LIQUIDITY_BY_WEIGHT_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct AddLiquidityByWeightAccounts {
    #[account(writable)]
    pub position: Pubkey,
    #[account(writable)]
    pub lb_pair: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account(writable)]
    pub bin_array_bitmap_extension: Pubkey,
    #[account(writable)]
    pub user_token_x: Pubkey,
    #[account(writable)]
    pub user_token_y: Pubkey,
    #[account(writable)]
    pub reserve_x: Pubkey,
    #[account(writable)]
    pub reserve_y: Pubkey,
    #[account]
    pub token_x_mint: Pubkey,
    #[account]
    pub token_y_mint: Pubkey,
    #[account(writable)]
    pub bin_array_lower: Pubkey,
    #[account(writable)]
    pub bin_array_upper: Pubkey,
    #[account(signer)]
    pub sender: Pubkey,
    #[account]
    pub token_x_program: Pubkey,
    #[account]
    pub token_y_program: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct AddLiquidityByWeightIx {
    pub accounts: AddLiquidityByWeightAccounts,
    pub args: AddLiquidityByWeightInstructionArgs,
}

impl AddLiquidityByWeightIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`ADD_LIQUIDITY_BY_WEIGHT_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = AddLiquidityByWeightAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<AddLiquidityByWeightInstructionArgs>(
            &ix.data,
            &ADD_LIQUIDITY_BY_WEIGHT_DISCRIMINATOR,
            "add_liquidity_by_weight",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`AddLiquidityByWeightAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: AddLiquidityByWeightAccounts,
        args: AddLiquidityByWeightInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`AddLiquidityByWeightIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = ADD_LIQUIDITY_BY_WEIGHT_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize add_liquidity_by_weight args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
