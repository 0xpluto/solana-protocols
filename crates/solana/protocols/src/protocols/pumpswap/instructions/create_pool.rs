//! PumpSwap CreatePool instruction types.
//!
//! Account layout derived from PumpSwap IDL + v2-crates reference.
//! `CreatePool` creates a new AMM pool with initial liquidity.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::CREATE_POOL_DISCRIMINATOR;

/// PumpSwap CreatePool instruction accounts.
///
/// Account indices from v2-crates:
/// \[0\]=pool, \[1\]=global_config, \[2\]=creator, \[3\]=base_mint, \[4\]=quote_mint,
/// \[5\]=lp_mint, \[6\]=user_base_token_account, \[7\]=user_quote_token_account,
/// \[8\]=user_lp_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas)]
pub struct CreatePoolAccounts {
    /// Pool state account (created).
    #[account(writable)]
    pub pool: Pubkey,
    /// PumpSwap global configuration.
    #[account]
    pub global_config: Pubkey,
    /// Pool creator (signer).
    #[account(writable, signer)]
    pub creator: Pubkey,
    /// Base token mint (the meme token).
    #[account]
    pub base_mint: Pubkey,
    /// Quote token mint (WSOL).
    #[account]
    pub quote_mint: Pubkey,
    /// LP token mint (created for this pool).
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// Creator's base token account.
    #[account(writable)]
    pub user_base_token_account: Pubkey,
    /// Creator's quote token account.
    #[account(writable)]
    pub user_quote_token_account: Pubkey,
    /// Creator's LP token account.
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

/// PumpSwap CreatePool instruction parameters.
#[derive(Debug, Clone, Serialize, Deserialize, InstructionData)]
#[instruction_data(discriminator = CREATE_POOL_DISCRIMINATOR)]
pub struct CreatePoolParams {
    /// Pool index (used for PDA derivation).
    pub index: u16,
    /// Base tokens to deposit as initial liquidity.
    pub base_amount_in: u64,
    /// Quote tokens (SOL) to deposit as initial liquidity.
    pub quote_amount_in: u64,
}

impl CreatePoolParams {
    /// Create new CreatePool parameters.
    #[must_use]
    pub fn new(index: u16, base_amount_in: u64, quote_amount_in: u64) -> Self {
        Self {
            index,
            base_amount_in,
            quote_amount_in,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn create_pool_params_roundtrip() {
        let params = CreatePoolParams::new(1, 1_000_000, 500_000_000);
        let data = params.to_data();

        // 8 disc + 2 index + 8 base + 8 quote = 26
        assert_eq!(data.len(), 26);
        assert_eq!(&data[..8], &CREATE_POOL_DISCRIMINATOR);

        let parsed = CreatePoolParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.index, 1);
        assert_eq!(parsed.base_amount_in, 1_000_000);
        assert_eq!(parsed.quote_amount_in, 500_000_000);
    }

    #[test]
    fn create_pool_accounts_count() {
        assert_eq!(CreatePoolAccounts::ACCOUNT_COUNT, 18);
    }
}
