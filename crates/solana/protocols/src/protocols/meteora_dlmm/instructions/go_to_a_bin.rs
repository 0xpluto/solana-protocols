//! `go_to_a_bin` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`GoToABinIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`GoToABinIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::GoToABinInstructionArgs;
pub use meteora_dlmm_sdk::instructions::GO_TO_A_BIN_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct GoToABinAccounts {
    #[account(writable)]
    pub lb_pair: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account]
    pub bin_array_bitmap_extension: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account]
    pub from_bin_array: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account]
    pub to_bin_array: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct GoToABinIx {
    pub accounts: GoToABinAccounts,
    pub args: GoToABinInstructionArgs,
}

impl GoToABinIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`GO_TO_A_BIN_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = GoToABinAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<GoToABinInstructionArgs>(
            &ix.data,
            &GO_TO_A_BIN_DISCRIMINATOR,
            "go_to_a_bin",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`GoToABinAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: GoToABinAccounts,
        args: GoToABinInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`GoToABinIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = GO_TO_A_BIN_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args).expect("borsh serialize go_to_a_bin args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
