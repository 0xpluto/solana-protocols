//! `update_reward_funder` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`UpdateRewardFunderIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`UpdateRewardFunderIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::UpdateRewardFunderInstructionArgs;
pub use meteora_dlmm_sdk::instructions::UPDATE_REWARD_FUNDER_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct UpdateRewardFunderAccounts {
    #[account(writable)]
    pub lb_pair: Pubkey,
    #[account(signer)]
    pub admin: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct UpdateRewardFunderIx {
    pub accounts: UpdateRewardFunderAccounts,
    pub args: UpdateRewardFunderInstructionArgs,
}

impl UpdateRewardFunderIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`UPDATE_REWARD_FUNDER_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = UpdateRewardFunderAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<UpdateRewardFunderInstructionArgs>(
            &ix.data,
            &UPDATE_REWARD_FUNDER_DISCRIMINATOR,
            "update_reward_funder",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`UpdateRewardFunderAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: UpdateRewardFunderAccounts,
        args: UpdateRewardFunderInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`UpdateRewardFunderIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = UPDATE_REWARD_FUNDER_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize update_reward_funder args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
