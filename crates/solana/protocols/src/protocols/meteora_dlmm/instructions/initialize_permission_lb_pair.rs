//! `initialize_permission_lb_pair` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`InitializePermissionLbPairIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`InitializePermissionLbPairIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::InitializePermissionLbPairInstructionArgs;
pub use meteora_dlmm_sdk::instructions::INITIALIZE_PERMISSION_LB_PAIR_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct InitializePermissionLbPairAccounts {
    #[account(signer)]
    pub base: Pubkey,
    #[account(writable)]
    pub lb_pair: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account(writable)]
    pub bin_array_bitmap_extension: Pubkey,
    #[account]
    pub token_mint_x: Pubkey,
    #[account]
    pub token_mint_y: Pubkey,
    #[account(writable)]
    pub reserve_x: Pubkey,
    #[account(writable)]
    pub reserve_y: Pubkey,
    #[account(writable)]
    pub oracle: Pubkey,
    #[account(writable, signer)]
    pub admin: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account]
    pub token_badge_x: Pubkey,
    /// Optional on-chain (program id sentinel = absent).
    #[account]
    pub token_badge_y: Pubkey,
    #[account]
    pub token_program_x: Pubkey,
    #[account]
    pub token_program_y: Pubkey,
    #[account]
    pub system_program: Pubkey,
    #[account]
    pub rent: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct InitializePermissionLbPairIx {
    pub accounts: InitializePermissionLbPairAccounts,
    pub args: InitializePermissionLbPairInstructionArgs,
}

impl InitializePermissionLbPairIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`INITIALIZE_PERMISSION_LB_PAIR_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = InitializePermissionLbPairAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<InitializePermissionLbPairInstructionArgs>(
            &ix.data,
            &INITIALIZE_PERMISSION_LB_PAIR_DISCRIMINATOR,
            "initialize_permission_lb_pair",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`InitializePermissionLbPairAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: InitializePermissionLbPairAccounts,
        args: InitializePermissionLbPairInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`InitializePermissionLbPairIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = INITIALIZE_PERMISSION_LB_PAIR_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args)
            .expect("borsh serialize initialize_permission_lb_pair args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
