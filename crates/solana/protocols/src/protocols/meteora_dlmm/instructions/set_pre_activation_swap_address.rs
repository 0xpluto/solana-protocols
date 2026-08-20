//! `set_pre_activation_swap_address` instruction.
//!
//! Generated wrapper. Re-run `tools/gen_dlmm_ix.py` against the
//! `meteora-dlmm-sdk` registry source to regenerate after an SDK
//! upgrade — this file isn't hand-edited.
//!
//! - **Parse:** [`SetPreActivationSwapAddressIx::parse`] decodes from a
//!   [`ParsedInstruction`] into typed accounts + args (when present).
//! - **Build:** [`SetPreActivationSwapAddressIx::build`] returns a ready-to-submit
//!   `solana_sdk::instruction::Instruction`. Account writability /
//!   signer flags come from the `#[account(...)]` attrs which mirror
//!   the SDK's `instruction_with_remaining_accounts` exactly.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::parse_anchor_args;

pub use meteora_dlmm_sdk::instructions::SetPreActivationSwapAddressInstructionArgs;
pub use meteora_dlmm_sdk::instructions::SET_PRE_ACTIVATION_SWAP_ADDRESS_DISCRIMINATOR;

#[derive(Debug, Clone, AccountMetas)]
#[accounts(unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have")]
pub struct SetPreActivationSwapAddressAccounts {
    #[account(writable)]
    pub lb_pair: Pubkey,
    #[account(signer)]
    pub creator: Pubkey,
}

#[derive(Debug, Clone)]
pub struct SetPreActivationSwapAddressIx {
    pub accounts: SetPreActivationSwapAddressAccounts,
    pub args: SetPreActivationSwapAddressInstructionArgs,
}

impl SetPreActivationSwapAddressIx {
    /// Parse a [`ParsedInstruction`] whose program id is the DLMM
    /// program and whose discriminator matches [`SET_PRE_ACTIVATION_SWAP_ADDRESS_DISCRIMINATOR`].
    pub fn parse(ix: &ParsedInstruction) -> Result<Self, InstructionParseError> {
        let accounts = SetPreActivationSwapAddressAccounts::from_pubkeys(&ix.accounts)?;
        let args = parse_anchor_args::<SetPreActivationSwapAddressInstructionArgs>(
            &ix.data,
            &SET_PRE_ACTIVATION_SWAP_ADDRESS_DISCRIMINATOR,
            "set_pre_activation_swap_address",
        )?;
        Ok(Self { accounts, args })
    }

    /// Build a ready-to-submit instruction from accounts (+ args
    /// when the ix takes any). Account writable / signer flags come
    /// from the [`AccountMetas`] derive on [`SetPreActivationSwapAddressAccounts`].
    /// The data section is the 8-byte discriminator followed by
    /// borsh-serialised args.
    pub fn build(
        accounts: SetPreActivationSwapAddressAccounts,
        args: SetPreActivationSwapAddressInstructionArgs,
    ) -> ::solana_sdk::instruction::Instruction {
        Self { accounts, args }.to_instruction()
    }

    /// Variant of [`Self::build`] for callers that already hold a fully
    /// populated [`SetPreActivationSwapAddressIx`] (e.g. just-parsed and being
    /// re-built for replay).
    pub fn to_instruction(&self) -> ::solana_sdk::instruction::Instruction {
        let mut data = SET_PRE_ACTIVATION_SWAP_ADDRESS_DISCRIMINATOR.to_vec();
        let args_bytes = ::borsh::to_vec(&self.args)
            .expect("borsh serialize set_pre_activation_swap_address args");
        data.extend_from_slice(&args_bytes);
        ::solana_sdk::instruction::Instruction {
            program_id: super::super::PROGRAM_ID,
            accounts: self.accounts.to_account_metas(),
            data,
        }
    }
}
