//! Raydium CLMM Swap (v1) instruction types.
//!
//! The original swap instruction — supports SPL Token only (no Token2022).
//! Account layout from Raydium CLMM Anchor IDL.
//!
//! Note: This instruction has 9 fixed accounts followed by variable-length
//! tick array accounts. Our `AccountMetas` struct covers only the fixed portion.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::SWAP_DISCRIMINATOR;

/// Swap (v1) instruction fixed accounts.
///
/// Account indices from Anchor IDL:
/// \[0\]=payer(signer), \[1\]=amm_config, \[2\]=pool_state(writable),
/// \[3\]=input_token_account(writable), \[4\]=output_token_account(writable),
/// \[5\]=input_vault(writable), \[6\]=output_vault(writable),
/// \[7\]=observation_state(writable), \[8\]=token_program
///
/// Remaining accounts (index 9+) are tick array accounts (writable, variable count).
#[derive(Debug, Clone, AccountMetas)]
pub struct SwapAccounts {
    /// User wallet (signer).
    #[account(signer)]
    pub payer: Pubkey,
    /// AMM config account.
    #[account]
    pub amm_config: Pubkey,
    /// Pool state account.
    #[account(writable)]
    pub pool_state: Pubkey,
    /// User's input token account.
    #[account(writable)]
    pub input_token_account: Pubkey,
    /// User's output token account.
    #[account(writable)]
    pub output_token_account: Pubkey,
    /// Pool's input token vault.
    #[account(writable)]
    pub input_vault: Pubkey,
    /// Pool's output token vault.
    #[account(writable)]
    pub output_vault: Pubkey,
    /// Observation state account.
    #[account(writable)]
    pub observation_state: Pubkey,
    /// SPL Token program.
    #[account]
    pub token_program: Pubkey,
}

/// Swap instruction parameters.
///
/// Uses 8-byte Anchor discriminator.
#[derive(Debug, Clone, InstructionData)]
#[instruction_data(discriminator = SWAP_DISCRIMINATOR)]
pub struct SwapParams {
    /// Input amount (or output amount if `is_base_input` is false).
    pub amount: u64,
    /// Minimum output (or maximum input if `is_base_input` is false).
    pub other_amount_threshold: u64,
    /// Price limit as sqrt_price_x64 (Q64.64 fixed-point).
    pub sqrt_price_limit_x64: u128,
    /// True if `amount` is the input amount, false if output amount.
    pub is_base_input: bool,
}

impl SwapParams {
    /// Create new Swap parameters.
    #[must_use]
    pub fn new(
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit_x64: u128,
        is_base_input: bool,
    ) -> Self {
        Self {
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64,
            is_base_input,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn swap_params_roundtrip() {
        let params = SwapParams::new(1_000_000_000, 900_000, 4_295_048_017u128, true);
        let data = params.to_data();

        // 8 disc + 8 amount + 8 threshold + 16 sqrt_price + 1 is_base_input = 41
        assert_eq!(data.len(), 41);
        assert_eq!(&data[..8], &SWAP_DISCRIMINATOR);

        let parsed = SwapParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount, 1_000_000_000);
        assert_eq!(parsed.other_amount_threshold, 900_000);
        assert_eq!(parsed.sqrt_price_limit_x64, 4_295_048_017u128);
        assert!(parsed.is_base_input);
    }

    #[test]
    fn swap_accounts_count() {
        assert_eq!(SwapAccounts::ACCOUNT_COUNT, 9);
    }
}
