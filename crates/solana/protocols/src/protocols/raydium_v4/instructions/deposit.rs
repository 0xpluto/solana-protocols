//! Raydium V4 Deposit instruction types.
//!
//! Account layout from Raydium V4 program IDL.
//! Deposit adds liquidity to an existing AMM pool.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::DEPOSIT_IX;

/// Deposit instruction accounts.
///
/// Account indices from v2 reference:
/// \[0\]=token_program, \[1\]=amm_id, \[2\]=authority, \[3\]=open_orders,
/// \[4\]=target_orders, \[5\]=lp_mint, \[6\]=coin_vault, \[7\]=pc_vault,
/// \[8\]=market, \[9\]=user_coin, \[10\]=user_pc, \[11\]=user_lp,
/// \[12\]=user, \[13\]=market_event_queue
#[derive(Debug, Clone, AccountMetas)]
pub struct DepositAccounts {
    /// SPL Token program.
    #[account]
    pub token_program: Pubkey,
    /// AMM account (pool state).
    #[account(writable)]
    pub amm_id: Pubkey,
    /// AMM authority PDA.
    #[account]
    pub amm_authority: Pubkey,
    /// AMM open orders account.
    #[account]
    pub amm_open_orders: Pubkey,
    /// AMM target orders account.
    #[account(writable)]
    pub target_orders: Pubkey,
    /// LP mint.
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// AMM coin vault.
    #[account(writable)]
    pub coin_vault: Pubkey,
    /// AMM PC vault.
    #[account(writable)]
    pub pc_vault: Pubkey,
    /// Market account.
    #[account]
    pub market: Pubkey,
    /// User coin token account.
    #[account(writable)]
    pub user_coin_token_account: Pubkey,
    /// User PC token account.
    #[account(writable)]
    pub user_pc_token_account: Pubkey,
    /// User LP token account.
    #[account(writable)]
    pub user_lp_token_account: Pubkey,
    /// User wallet (signer).
    #[account(signer)]
    pub user: Pubkey,
    /// Market event queue.
    #[account]
    pub market_event_queue: Pubkey,
}

/// Deposit instruction parameters.
///
/// Uses 1-byte instruction index (0x03).
/// Actual deposited amounts come from CPI transfers, not these params.
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = [DEPOSIT_IX], discriminator_size = 1)]
pub struct DepositParams {
    /// Maximum coin (base) tokens to deposit.
    pub max_coin_amount: u64,
    /// Maximum PC (quote) tokens to deposit.
    pub max_pc_amount: u64,
    /// Base side (0 = coin, 1 = pc).
    pub base_side: u64,
}

impl DepositParams {
    /// Create new Deposit parameters.
    #[must_use]
    pub fn new(max_coin_amount: u64, max_pc_amount: u64, base_side: u64) -> Self {
        Self {
            max_coin_amount,
            max_pc_amount,
            base_side,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn deposit_params_roundtrip() {
        let params = DepositParams::new(1_000_000, 500_000_000, 0);
        let data = params.to_data();

        // 1 disc + 8 coin + 8 pc + 8 base_side = 25
        assert_eq!(data.len(), 25);
        assert_eq!(data[0], DEPOSIT_IX);

        let parsed = DepositParams::from_instruction_data(&data[1..]).unwrap();
        assert_eq!(parsed.max_coin_amount, 1_000_000);
        assert_eq!(parsed.max_pc_amount, 500_000_000);
        assert_eq!(parsed.base_side, 0);
    }

    #[test]
    fn deposit_accounts_count() {
        assert_eq!(DepositAccounts::ACCOUNT_COUNT, 14);
    }
}
