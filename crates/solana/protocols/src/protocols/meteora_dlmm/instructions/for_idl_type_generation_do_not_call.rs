//! `for_idl_type_generation_do_not_call` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`ForIdlTypeGenerationDoNotCallIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`ForIdlTypeGenerationDoNotCallIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::ForIdlTypeGenerationDoNotCallInstructionArgs;
pub use meteora_dlmm_sdk::instructions::FOR_IDL_TYPE_GENERATION_DO_NOT_CALL_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct ForIdlTypeGenerationDoNotCallAccounts {
    #[account]
    pub dummy_zc_account: Pubkey,
}

#[derive(Debug, Clone)]
pub struct ForIdlTypeGenerationDoNotCallIx {
    pub accounts: ForIdlTypeGenerationDoNotCallAccounts,
    pub args: ForIdlTypeGenerationDoNotCallInstructionArgs,
}

impl ForIdlTypeGenerationDoNotCallIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`FOR_IDL_TYPE_GENERATION_DO_NOT_CALL_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = ForIdlTypeGenerationDoNotCallAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<ForIdlTypeGenerationDoNotCallInstructionArgs>(
            &ix.data,
            &FOR_IDL_TYPE_GENERATION_DO_NOT_CALL_DISCRIMINATOR,
            "for_idl_type_generation_do_not_call",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`ForIdlTypeGenerationDoNotCallAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: ForIdlTypeGenerationDoNotCallAccounts,
        args: ForIdlTypeGenerationDoNotCallInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`ForIdlTypeGenerationDoNotCallIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = FOR_IDL_TYPE_GENERATION_DO_NOT_CALL_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args)
            .expect("borsh serialize for_idl_type_generation_do_not_call args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
