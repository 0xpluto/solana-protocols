//! `set_pre_activation_duration` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`SetPreActivationDurationIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`SetPreActivationDurationIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::SetPreActivationDurationInstructionArgs;
pub use meteora_dlmm_sdk::instructions::SET_PRE_ACTIVATION_DURATION_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct SetPreActivationDurationAccounts {
    #[account(writable)]
    pub lb_pair: Pubkey,
    #[account(signer)]
    pub creator: Pubkey,
}

#[derive(Debug, Clone)]
pub struct SetPreActivationDurationIx {
    pub accounts: SetPreActivationDurationAccounts,
    pub args: SetPreActivationDurationInstructionArgs,
}

impl SetPreActivationDurationIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`SET_PRE_ACTIVATION_DURATION_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = SetPreActivationDurationAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<SetPreActivationDurationInstructionArgs>(
            &ix.data,
            &SET_PRE_ACTIVATION_DURATION_DISCRIMINATOR,
            "set_pre_activation_duration",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`SetPreActivationDurationAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: SetPreActivationDurationAccounts,
        args: SetPreActivationDurationInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`SetPreActivationDurationIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = SET_PRE_ACTIVATION_DURATION_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize set_pre_activation_duration args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
