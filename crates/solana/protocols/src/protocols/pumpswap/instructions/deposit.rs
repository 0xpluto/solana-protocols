//! PumpSwap Deposit instruction types.
//!
//! Account layout derived from PumpSwap IDL + v2-crates reference.
//! `Deposit` adds liquidity to an existing AMM pool.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::DEPOSIT_DISCRIMINATOR;

/// PumpSwap Deposit instruction accounts.
///
/// Account indices from v2-crates:
/// \[0\]=pool, \[1\]=global_config, \[2\]=user, \[3\]=base_mint, \[4\]=quote_mint,
/// \[5\]=lp_mint, \[6\]=user_base_token_account, \[7\]=user_quote_token_account,
/// \[8\]=user_lp_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas)]
pub struct DepositAccounts {
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

/// PumpSwap Deposit instruction parameters.
#[derive(Debug, Clone, Serialize, Deserialize, InstructionData)]
#[instruction_data(discriminator = DEPOSIT_DISCRIMINATOR)]
pub struct DepositParams {
    /// LP tokens to mint (desired output).
    pub lp_token_amount_out: u64,
    /// Maximum base tokens to deposit (slippage protection).
    pub max_base_amount_in: u64,
    /// Maximum quote tokens (SOL) to deposit (slippage protection).
    pub max_quote_amount_in: u64,
}

impl DepositParams {
    /// Create new Deposit parameters.
    #[must_use]
    pub fn new(
        lp_token_amount_out: u64,
        max_base_amount_in: u64,
        max_quote_amount_in: u64,
    ) -> Self {
        Self {
            lp_token_amount_out,
            max_base_amount_in,
            max_quote_amount_in,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn deposit_params_roundtrip() {
        let params = DepositParams::new(100_000, 1_000_000, 500_000_000);
        let data = params.to_data();

        // 8 disc + 8 lp + 8 base + 8 quote = 32
        assert_eq!(data.len(), 32);
        assert_eq!(&data[..8], &DEPOSIT_DISCRIMINATOR);

        let parsed = DepositParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.lp_token_amount_out, 100_000);
        assert_eq!(parsed.max_base_amount_in, 1_000_000);
        assert_eq!(parsed.max_quote_amount_in, 500_000_000);
    }

    #[test]
    fn deposit_accounts_count() {
        assert_eq!(DepositAccounts::ACCOUNT_COUNT, 18);
    }
}
