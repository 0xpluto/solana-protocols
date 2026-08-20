//! `initialize_customizable_permissionless_lb_pair2` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`InitializeCustomizablePermissionlessLbPair2Ix::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`InitializeCustomizablePermissionlessLbPair2Ix::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::InitializeCustomizablePermissionlessLbPair2InstructionArgs;
pub use meteora_dlmm_sdk::instructions::INITIALIZE_CUSTOMIZABLE_PERMISSIONLESS_LB_PAIR2_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct InitializeCustomizablePermissionlessLbPair2Accounts {
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
    #[account]
    pub user_token_x: Pubkey,
    #[account(writable, signer)]
    pub funder: Pubkey,
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
    pub user_token_y: Pubkey,
    #[account]
    pub event_authority: Pubkey,
    #[account]
    pub program: Pubkey,
}

#[derive(Debug, Clone)]
pub struct InitializeCustomizablePermissionlessLbPair2Ix {
    pub accounts: InitializeCustomizablePermissionlessLbPair2Accounts,
    pub args: InitializeCustomizablePermissionlessLbPair2InstructionArgs,
}

impl InitializeCustomizablePermissionlessLbPair2Ix {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`INITIALIZE_CUSTOMIZABLE_PERMISSIONLESS_LB_PAIR2_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts =
            InitializeCustomizablePermissionlessLbPair2Accounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<InitializeCustomizablePermissionlessLbPair2InstructionArgs>(
            &ix.data,
            &INITIALIZE_CUSTOMIZABLE_PERMISSIONLESS_LB_PAIR2_DISCRIMINATOR,
            "initialize_customizable_permissionless_lb_pair2",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`InitializeCustomizablePermissionlessLbPair2Accounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: InitializeCustomizablePermissionlessLbPair2Accounts,
        args: InitializeCustomizablePermissionlessLbPair2InstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`InitializeCustomizablePermissionlessLbPair2Ix`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = INITIALIZE_CUSTOMIZABLE_PERMISSIONLESS_LB_PAIR2_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args)
            .expect("borsh serialize initialize_customizable_permissionless_lb_pair2 args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
