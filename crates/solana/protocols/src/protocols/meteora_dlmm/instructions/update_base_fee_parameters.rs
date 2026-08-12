//! `update_base_fee_parameters` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`UpdateBaseFeeParametersIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`UpdateBaseFeeParametersIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::UpdateBaseFeeParametersInstructionArgs;
pub use meteora_dlmm_sdk::instructions::UPDATE_BASE_FEE_PARAMETERS_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct UpdateBaseFeeParametersAccounts {
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
pub struct UpdateBaseFeeParametersIx {
    pub accounts: UpdateBaseFeeParametersAccounts,
    pub args: UpdateBaseFeeParametersInstructionArgs,
}

impl UpdateBaseFeeParametersIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`UPDATE_BASE_FEE_PARAMETERS_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = UpdateBaseFeeParametersAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<UpdateBaseFeeParametersInstructionArgs>(
            &ix.data,
            &UPDATE_BASE_FEE_PARAMETERS_DISCRIMINATOR,
            "update_base_fee_parameters",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`UpdateBaseFeeParametersAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: UpdateBaseFeeParametersAccounts,
        args: UpdateBaseFeeParametersInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`UpdateBaseFeeParametersIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = UPDATE_BASE_FEE_PARAMETERS_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize update_base_fee_parameters args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
