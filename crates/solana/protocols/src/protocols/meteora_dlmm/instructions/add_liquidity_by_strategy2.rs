//! `add_liquidity_by_strategy2` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`AddLiquidityByStrategy2Ix::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`AddLiquidityByStrategy2Ix::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::AddLiquidityByStrategy2InstructionArgs;
pub use meteora_dlmm_sdk::instructions::ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct AddLiquidityByStrategy2Accounts {
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

    /// The bin-array PDAs the deposit range spans, appended as writable
    /// remaining accounts.
    ///
    /// How many depends on how wide the range is, which is not knowable from
    /// the account list — the same shape as CLMM's tick arrays. Meteora's own
    /// client appends them here and `remaining_accounts_info.slices` is left
    /// empty, so position past the declared list is all that identifies them.
    #[account(
        writable,
        remaining,
        reason = "bin-array PDAs for the deposit range: the count depends on how \
                  many bins the range spans, which the account list does not say, \
                  and each is the same kind of thing so there is nothing to name"
    )]
    pub bin_arrays: Vec<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct AddLiquidityByStrategy2Ix {
    pub accounts: AddLiquidityByStrategy2Accounts,
    pub args: AddLiquidityByStrategy2InstructionArgs,
}

impl AddLiquidityByStrategy2Ix {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = AddLiquidityByStrategy2Accounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<AddLiquidityByStrategy2InstructionArgs>(
            &ix.data,
            &ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR,
            "add_liquidity_by_strategy2",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`AddLiquidityByStrategy2Accounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: AddLiquidityByStrategy2Accounts,
        args: AddLiquidityByStrategy2InstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`AddLiquidityByStrategy2Ix`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize add_liquidity_by_strategy2 args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
