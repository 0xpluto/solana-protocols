//! `claim_fee2` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`ClaimFee2Ix::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`ClaimFee2Ix::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::ClaimFee2InstructionArgs;
pub use meteora_dlmm_sdk::instructions::CLAIM_FEE2_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct ClaimFee2Accounts {
    #[account(writable)]
    pub lb_pair: Pubkey,
    #[account(writable)]
    pub position: Pubkey,
    #[account(signer)]
    pub sender: Pubkey,
    #[account(writable)]
    pub reserve_x: Pubkey,
    #[account(writable)]
    pub reserve_y: Pubkey,
    #[account(writable)]
    pub user_token_x: Pubkey,
    #[account(writable)]
    pub user_token_y: Pubkey,
    #[account]
    pub token_x_mint: Pubkey,
    #[account]
    pub token_y_mint: Pubkey,
    #[account]
    pub token_program_x: Pubkey,
    #[account]
    pub token_program_y: Pubkey,
    #[account]
    pub memo_program: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct ClaimFee2Ix {
    pub accounts: ClaimFee2Accounts,
    pub args: ClaimFee2InstructionArgs,
}

impl ClaimFee2Ix {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`CLAIM_FEE2_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = ClaimFee2Accounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<ClaimFee2InstructionArgs>(
            &ix.data,
            &CLAIM_FEE2_DISCRIMINATOR,
            "claim_fee2",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`ClaimFee2Accounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: ClaimFee2Accounts,
        args: ClaimFee2InstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`ClaimFee2Ix`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = CLAIM_FEE2_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args).expect("borsh serialize claim_fee2 args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
