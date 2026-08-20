//! PumpSwap Withdraw instruction types.
//!
//! Account layout derived from PumpSwap IDL + v2-crates reference.
//! `Withdraw` removes liquidity from an existing AMM pool.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::WITHDRAW_DISCRIMINATOR;

/// PumpSwap Withdraw instruction accounts.
///
/// Account indices from v2-crates:
/// \[0\]=pool, \[1\]=global_config, \[2\]=user, \[3\]=base_mint, \[4\]=quote_mint,
/// \[5\]=lp_mint, \[6\]=user_base_token_account, \[7\]=user_quote_token_account,
/// \[8\]=user_lp_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas)]
pub struct WithdrawAccounts {
    /// Pool state account.
    #[account(writable)]
    pub pool: Pubkey,
    /// PumpSwap global configuration.
    #[account]
    pub global_config: Pubkey,
    /// Liquidity provider (signer).
    #[account(writable, signer)]
    pub user: Pubkey,
    /// Base token mint (the meme token).
    #[account]
    pub base_mint: Pubkey,
    /// Quote token mint (WSOL).
    #[account]
    pub quote_mint: Pubkey,
    /// LP token mint.
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// User's base token account.
    #[account(writable)]
    pub user_base_token_account: Pubkey,
    /// User's quote token account.
    #[account(writable)]
    pub user_quote_token_account: Pubkey,
    /// User's LP token account.
    #[account(writable)]
    pub user_lp_token_account: Pubkey,
    /// Pool's base token vault.
    #[account(writable)]
    pub pool_base_token_account: Pubkey,
    /// Pool's quote token vault.
    #[account(writable)]
    pub pool_quote_token_account: Pubkey,
    /// Base token program (SPL Token or Token-2022).
    #[account]
    pub base_token_program: Pubkey,
    /// Quote token program.
    #[account]
    pub quote_token_program: Pubkey,
    /// LP token program.
    #[account]
    pub lp_token_program: Pubkey,
    /// System program.
    #[account]
    pub system_program: Pubkey,
    /// Associated token program.
    #[account]
    pub associated_token_program: Pubkey,
    /// Event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// PumpSwap program.
    #[account]
    pub program: Pubkey,
}

/// PumpSwap Withdraw instruction parameters.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = WITHDRAW_DISCRIMINATOR)]
pub struct WithdrawParams {
    /// LP tokens to burn.
    pub lp_amount_in: u64,
    /// Minimum base tokens to receive (slippage protection).
    pub min_base_amount_out: u64,
    /// Minimum quote tokens (SOL) to receive (slippage protection).
    pub min_quote_amount_out: u64,
}

impl WithdrawParams {
    /// Create new Withdraw parameters.
    #[must_use]
    pub fn new(lp_amount_in: u64, min_base_amount_out: u64, min_quote_amount_out: u64) -> Self {
        Self {
            lp_amount_in,
            min_base_amount_out,
            min_quote_amount_out,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn withdraw_params_roundtrip() {
        let params = WithdrawParams::new(100_000, 1_000_000, 500_000_000);
        let data = params.to_data();

        // 8 disc + 8 lp + 8 base + 8 quote = 32
        assert_eq!(data.len(), 32);
        assert_eq!(&data[..8], &WITHDRAW_DISCRIMINATOR);

        let parsed = WithdrawParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.lp_amount_in, 100_000);
        assert_eq!(parsed.min_base_amount_out, 1_000_000);
        assert_eq!(parsed.min_quote_amount_out, 500_000_000);
    }

    #[test]
    fn withdraw_accounts_count() {
        assert_eq!(WithdrawAccounts::ACCOUNT_COUNT, 18);
    }
}
