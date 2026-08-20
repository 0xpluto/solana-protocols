//! Raydium V4 Withdraw instruction types.
//!
//! Account layout from Raydium V4 program IDL.
//! Withdraw removes liquidity from an AMM pool.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::WITHDRAW_IX;

/// Withdraw instruction accounts.
///
/// Account indices from v2 reference:
/// \[0\]=token_program, \[1\]=amm_id, \[2\]=authority, \[3\]=open_orders,
/// \[4\]=target_orders, \[5\]=lp_mint, \[6\]=coin_vault, \[7\]=pc_vault,
/// \[8\]=market_program, \[9\]=market, \[10\]=market_coin_vault,
/// \[11\]=market_pc_vault, \[12\]=vault_signer, \[13\]=user_lp,
/// \[14\]=user_coin, \[15\]=user_pc, \[16\]=user, \[17\]=market_event_queue,
/// \[18\]=market_bids, \[19\]=market_asks
#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct WithdrawAccounts {
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
    #[account(writable)]
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
    /// Market program (Serum/OpenBook).
    #[account]
    pub market_program: Pubkey,
    /// Market account.
    #[account(writable)]
    pub market: Pubkey,
    /// Market coin vault.
    #[account(writable)]
    pub market_coin_vault: Pubkey,
    /// Market PC vault.
    #[account(writable)]
    pub market_pc_vault: Pubkey,
    /// Market vault signer.
    #[account]
    pub vault_signer: Pubkey,
    /// User LP token account (LP tokens burned from here).
    #[account(writable)]
    pub user_pool_token_account: Pubkey,
    /// User coin token account (receives coin).
    #[account(writable)]
    pub user_coin_token_account: Pubkey,
    /// User PC token account (receives PC).
    #[account(writable)]
    pub user_pc_token_account: Pubkey,
    /// User wallet (signer).
    #[account(signer)]
    pub user: Pubkey,
    /// Market event queue.
    #[account(writable)]
    pub market_event_queue: Pubkey,
    /// Market bids.
    #[account(writable)]
    pub market_bids: Pubkey,
    /// Market asks.
    #[account(writable)]
    pub market_asks: Pubkey,
}

/// Withdraw instruction parameters.
///
/// Uses 1-byte instruction index (0x04).
/// Actual withdrawn amounts come from CPI transfers, not these params.
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = [WITHDRAW_IX], discriminator_size = 1, unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; pinning params here would claim a verification the rest of the vertical does not have")]
pub struct WithdrawParams {
    /// LP token amount to burn (withdraw).
    pub amount: u64,
}

impl WithdrawParams {
    /// Create new Withdraw parameters.
    #[must_use]
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn withdraw_params_roundtrip() {
        let params = WithdrawParams::new(100_000);
        let data = params.to_data();

        // 1 disc + 8 amount = 9
        assert_eq!(data.len(), 9);
        assert_eq!(data[0], WITHDRAW_IX);

        let parsed = WithdrawParams::from_instruction_data(&data[1..]).unwrap();
        assert_eq!(parsed.amount, 100_000);
    }

    #[test]
    fn withdraw_accounts_count() {
        assert_eq!(WithdrawAccounts::ACCOUNT_COUNT, 20);
    }
}
