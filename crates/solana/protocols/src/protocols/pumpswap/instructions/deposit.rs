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
/// \[8\]=user_pool_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas)]
#[idl(program = "pump_amm", instruction = "deposit")]
#[accounts(
    unverified = "not witnessed on the firehose during capture, so there is no real instruction to pin the account list against"
)]
pub struct DepositAccounts {
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

/// PumpSwap Deposit instruction parameters.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = DEPOSIT_DISCRIMINATOR, unverified = "not witnessed on the firehose during capture, so there is no real instruction to pin it against", idl(program = "pump_amm", instruction = "deposit"))]
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
        // 15, per pump_amm.json. It asserted 18 against a struct written to a
        // layout the program never had.
        assert_eq!(DepositAccounts::ACCOUNT_COUNT, 15);
    }
}
