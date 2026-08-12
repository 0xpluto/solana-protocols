//! `update_position_operator` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`UpdatePositionOperatorIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`UpdatePositionOperatorIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::UpdatePositionOperatorInstructionArgs;
pub use meteora_dlmm_sdk::instructions::UPDATE_POSITION_OPERATOR_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct UpdatePositionOperatorAccounts {
    #[account(writable)]
    pub position: Pubkey,
    #[account(signer)]
    pub owner: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct UpdatePositionOperatorIx {
    pub accounts: UpdatePositionOperatorAccounts,
    pub args: UpdatePositionOperatorInstructionArgs,
}

impl UpdatePositionOperatorIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`UPDATE_POSITION_OPERATOR_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = UpdatePositionOperatorAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<UpdatePositionOperatorInstructionArgs>(
            &ix.data,
            &UPDATE_POSITION_OPERATOR_DISCRIMINATOR,
            "update_position_operator",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`UpdatePositionOperatorAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: UpdatePositionOperatorAccounts,
        args: UpdatePositionOperatorInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`UpdatePositionOperatorIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = UPDATE_POSITION_OPERATOR_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize update_position_operator args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
