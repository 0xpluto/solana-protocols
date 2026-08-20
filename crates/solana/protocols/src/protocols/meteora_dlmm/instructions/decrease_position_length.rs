//! `decrease_position_length` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`DecreasePositionLengthIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`DecreasePositionLengthIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::DecreasePositionLengthInstructionArgs;
pub use meteora_dlmm_sdk::instructions::DECREASE_POSITION_LENGTH_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have")]
pub struct DecreasePositionLengthAccounts {
    #[account(writable)]
    pub rent_receiver: Pubkey,
    #[account(writable)]
    pub position: Pubkey,
    #[account(signer)]
    pub owner: Pubkey,
    #[account]
    pub system_program: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct DecreasePositionLengthIx {
    pub accounts: DecreasePositionLengthAccounts,
    pub args: DecreasePositionLengthInstructionArgs,
}

impl DecreasePositionLengthIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`DECREASE_POSITION_LENGTH_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = DecreasePositionLengthAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<DecreasePositionLengthInstructionArgs>(
            &ix.data,
            &DECREASE_POSITION_LENGTH_DISCRIMINATOR,
            "decrease_position_length",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`DecreasePositionLengthAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: DecreasePositionLengthAccounts,
        args: DecreasePositionLengthInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`DecreasePositionLengthIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = DECREASE_POSITION_LENGTH_DISCRIMINATOR.to_vec();
        let args_bytes =
            ::borsh::to_vec(&self.args).expect("borsh serialize decrease_position_length args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
