//! PumpSwap Withdraw instruction types.
//!
//! Account layout derived from PumpSwap IDL + v2-crates reference.
//! `Withdraw` removes liquidity from an existing AMM pool.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};

use super::super::constants::WITHDRAW_DISCRIMINATOR;

/// PumpSwap Withdraw instruction accounts.
///
/// Account indices from v2-crates:
/// \[0\]=pool, \[1\]=global_config, \[2\]=user, \[3\]=base_mint, \[4\]=quote_mint,
/// \[5\]=lp_mint, \[6\]=user_base_token_account, \[7\]=user_quote_token_account,
/// \[8\]=user_pool_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[idl(program = "pump_amm", instruction = "withdraw")]
#[onchain_ix(fixtures("pumpswap/ix_withdraw_n15.json"))]
pub struct WithdrawAccounts {
    /// IDL slot 0.
    #[account(writable)]
    pub pool: Pubkey,
    /// IDL slot 1.
    #[account]
    pub global_config: Pubkey,
    /// IDL slot 2.
    #[account(signer)]
    pub user: Pubkey,
    /// IDL slot 3.
    #[account]
    pub base_mint: Pubkey,
    /// IDL slot 4.
    #[account]
    pub quote_mint: Pubkey,
    /// IDL slot 5.
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// IDL slot 6.
    #[account(writable)]
    pub user_base_token_account: Pubkey,
    /// IDL slot 7.
    #[account(writable)]
    pub user_quote_token_account: Pubkey,
    /// IDL slot 8.
    #[account(writable)]
    pub user_pool_token_account: Pubkey,
    /// IDL slot 9.
    #[account(writable)]
    pub pool_base_token_account: Pubkey,
    /// IDL slot 10.
    #[account(writable)]
    pub pool_quote_token_account: Pubkey,
    /// IDL slot 11.
    #[account]
    pub token_program: Pubkey,
    /// IDL slot 12.
    #[account]
    pub token_2022_program: Pubkey,
    /// IDL slot 13.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 14.
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
#[instruction_data(discriminator = WITHDRAW_DISCRIMINATOR, fixtures(
    "pumpswap/ix_withdraw_n15.json"
), idl(program = "pump_amm", instruction = "withdraw"))]
pub struct WithdrawParams {
    /// LP tokens to burn.
    pub lp_token_amount_in: u64,
    /// Minimum base tokens to receive (slippage protection).
    pub min_base_amount_out: u64,
    /// Minimum quote tokens (SOL) to receive (slippage protection).
    pub min_quote_amount_out: u64,
}

impl WithdrawParams {
    /// Create new Withdraw parameters.
    #[must_use]
    pub fn new(
        lp_token_amount_in: u64,
        min_base_amount_out: u64,
        min_quote_amount_out: u64,
    ) -> Self {
        Self {
            lp_token_amount_in,
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
        assert_eq!(parsed.lp_token_amount_in, 100_000);
        assert_eq!(parsed.min_base_amount_out, 1_000_000);
        assert_eq!(parsed.min_quote_amount_out, 500_000_000);
    }

    #[test]
    fn withdraw_accounts_count() {
        // 15, per pump_amm.json and every real instruction. It asserted 18 while
        // the struct declared three accounts the program does not take.
        assert_eq!(WithdrawAccounts::ACCOUNT_COUNT, 15);
    }
}
