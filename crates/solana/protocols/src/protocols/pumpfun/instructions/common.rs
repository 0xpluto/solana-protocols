//! Common instruction helpers.
//!
//! Shared utilities used across multiple instruction types.
//!
//! Note: pumpfun mints exist under **both** classic SPL and Token-2022 (a real
//! landed buy on a T22 pump mint is pinned in `fixtures/pumpfun/ix_buy.json` —
//! this module's original "pumpfun only uses SPL Token" note was wrong). The
//! classic-hardcoded helpers below remain for callers that have verified the
//! mint; resolve the mint's owner and use
//! [`create_ata_idempotent_instruction_for`] when it might be Token-2022.

use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::Instruction;

use crate::tokens::TokenProgram;

/// Helper to create an associated token account if it doesn't exist.
///
/// This creates the instruction to create an ATA for the user to receive tokens.
/// Should be included before the buy instruction if the ATA doesn't exist.
///
/// # Note
///
/// This uses SPL Token program. For Token 2022 support, use
/// [`TokenWithProgram::create_ata_instruction`](crate::tokens::TokenWithProgram::create_ata_instruction).
#[must_use]
pub fn create_ata_instruction(payer: &Pubkey, owner: &Pubkey, mint: &Pubkey) -> Instruction {
    spl_associated_token_account::instruction::create_associated_token_account(
        payer,
        owner,
        mint,
        &spl_token::id(),
    )
}

/// Helper to create an idempotent ATA instruction.
///
/// This creates the instruction that will create an ATA if it doesn't exist,
/// or do nothing if it already exists. Safer than the non-idempotent version.
///
/// # Note
///
/// This uses SPL Token program. For Token 2022 support, use
/// [`TokenWithProgram::create_ata_instruction`](crate::tokens::TokenWithProgram::create_ata_instruction).
#[must_use]
pub fn create_ata_idempotent_instruction(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Instruction {
    spl_associated_token_account::instruction::create_associated_token_account_idempotent(
        payer,
        owner,
        mint,
        &spl_token::id(),
    )
}

/// Program-aware idempotent ATA creation: the ATA address *and* the created
/// account's program both depend on whether the mint is classic SPL or
/// Token-2022 — resolve the mint's owner and pass it here.
#[must_use]
pub fn create_ata_idempotent_instruction_for(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    program: TokenProgram,
) -> Instruction {
    spl_associated_token_account::instruction::create_associated_token_account_idempotent(
        payer,
        owner,
        mint,
        &program.id(),
    )
}
