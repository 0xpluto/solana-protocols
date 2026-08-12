//! `update_fees_and_reward2` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`UpdateFeesAndReward2Ix::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`UpdateFeesAndReward2Ix::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::UpdateFeesAndReward2InstructionArgs;
pub use meteora_dlmm_sdk::instructions::UPDATE_FEES_AND_REWARD2_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct UpdateFeesAndReward2Accounts {
    #[account(writable)]
    pub position: Pubkey,
    #[account(writable)]
    pub lb_pair: Pubkey,
    #[account(signer)]
    pub owner: Pubkey,
}

#[derive(Debug, Clone)]
pub struct UpdateFeesAndReward2Ix {
    pub accounts: UpdateFeesAndReward2Accounts,
    pub args: UpdateFeesAndReward2InstructionArgs,
}

impl UpdateFeesAndReward2Ix {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`UPDATE_FEES_AND_REWARD2_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = UpdateFeesAndReward2Accounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<UpdateFeesAndReward2InstructionArgs>(
            &ix.data,
            &UPDATE_FEES_AND_REWARD2_DISCRIMINATOR,
            "update_fees_and_reward2",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`UpdateFeesAndReward2Accounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: UpdateFeesAndReward2Accounts,
        args: UpdateFeesAndReward2InstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`UpdateFeesAndReward2Ix`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = UPDATE_FEES_AND_REWARD2_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize update_fees_and_reward2 args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
