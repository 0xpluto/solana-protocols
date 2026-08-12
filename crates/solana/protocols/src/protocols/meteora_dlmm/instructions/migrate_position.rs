//! `migrate_position` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`MigratePositionIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`MigratePositionIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

pub use meteora_dlmm_sdk::instructions::MIGRATE_POSITION_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
pub struct MigratePositionAccounts {
    #[account(writable, signer)]
    pub position_v2: Pubkey,
    #[account(writable)]
    pub position_v1: Pubkey,
    #[account]
    pub lb_pair: Pubkey,
    #[account(writable)]
    pub bin_array_lower: Pubkey,
    #[account(writable)]
    pub bin_array_upper: Pubkey,
    #[account(writable, signer)]
    pub owner: Pubkey,
    #[account]
    pub system_program: Pubkey,
    #[account(writable)]
    pub rent_receiver: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct MigratePositionIx {
    pub accounts: MigratePositionAccounts,
}

impl MigratePositionIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`MIGRATE_POSITION_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = MigratePositionAccounts::from_pubkeys(&ix.accounts)?;
        Ok(Self { accounts })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`MigratePositionAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(accounts: MigratePositionAccounts) -> ::solana_sdk::instruction::Instruction {
        Self { accounts }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`MigratePositionIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let data = MIGRATE_POSITION_DISCRIMINATOR.to_vec();
        // No args — the discriminator alone is the body.
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
