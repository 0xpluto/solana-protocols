//! `initialize_preset_parameter2` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`InitializePresetParameter2Ix::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`InitializePresetParameter2Ix::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::InitializePresetParameter2InstructionArgs;
pub use meteora_dlmm_sdk::instructions::INITIALIZE_PRESET_PARAMETER2_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct InitializePresetParameter2Accounts {
    #[account(writable)]
    pub preset_parameter: Pubkey,
    #[account(writable, signer)]
    pub admin: Pubkey,
    #[account]
    pub system_program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct InitializePresetParameter2Ix {
    pub accounts: InitializePresetParameter2Accounts,
    pub args: InitializePresetParameter2InstructionArgs,
}

impl InitializePresetParameter2Ix {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`INITIALIZE_PRESET_PARAMETER2_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = InitializePresetParameter2Accounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<InitializePresetParameter2InstructionArgs>(
            &ix.data,
            &INITIALIZE_PRESET_PARAMETER2_DISCRIMINATOR,
            "initialize_preset_parameter2",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`InitializePresetParameter2Accounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: InitializePresetParameter2Accounts,
        args: InitializePresetParameter2InstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`InitializePresetParameter2Ix`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = INITIALIZE_PRESET_PARAMETER2_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize initialize_preset_parameter2 args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
