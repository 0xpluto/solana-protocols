//! Raydium V4 Initialize2 instruction types.
//!
//! Account layout from Raydium V4 program IDL.
//! Initialize2 creates a new AMM pool with initial liquidity.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};

use super::super::constants::INITIALIZE2_IX;

/// Initialize2 instruction accounts.
///
/// Account indices from v2 reference:
/// \[0\]=token_program, \[1\]=associated_token_program, \[2\]=system_program,
/// \[3\]=rent, \[4\]=amm_id, \[5\]=authority, \[6\]=open_orders, \[7\]=lp_mint,
/// \[8\]=coin_mint, \[9\]=pc_mint, \[10\]=coin_vault, \[11\]=pc_vault,
/// \[12\]=target_orders, \[13\]=amm_config, \[14\]=create_pool_fee_dest,
/// \[15\]=market_program, \[16\]=market, \[17\]=user, \[18\]=user_coin,
/// \[19\]=user_pc, \[20\]=user_lp
#[derive(Debug, Clone, AccountMetas)]
pub struct Initialize2Accounts {
    /// SPL Token program.
    #[account]
    pub token_program: Pubkey,
    /// Associated Token program.
    #[account]
    pub associated_token_program: Pubkey,
    /// System program.
    #[account]
    pub system_program: Pubkey,
    /// Rent sysvar.
    #[account]
    pub rent: Pubkey,
    /// New AMM account (pool state).
    #[account(writable)]
    pub amm_id: Pubkey,
    /// AMM authority PDA.
    #[account]
    pub amm_authority: Pubkey,
    /// AMM open orders account.
    #[account(writable)]
    pub amm_open_orders: Pubkey,
    /// AMM LP mint.
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// Coin (base) token mint.
    #[account]
    pub coin_mint: Pubkey,
    /// PC (quote) token mint.
    #[account]
    pub pc_mint: Pubkey,
    /// AMM coin vault.
    #[account(writable)]
    pub coin_vault: Pubkey,
    /// AMM PC vault.
    #[account(writable)]
    pub pc_vault: Pubkey,
    /// AMM target orders account.
    #[account(writable)]
    pub target_orders: Pubkey,
    /// AMM config account.
    #[account]
    pub amm_config: Pubkey,
    /// Create pool fee destination.
    #[account]
    pub create_pool_fee_destination: Pubkey,
    /// Market program (Serum/OpenBook).
    #[account]
    pub market_program: Pubkey,
    /// Market account.
    #[account(writable)]
    pub market: Pubkey,
    /// User wallet (signer).
    #[account(writable, signer)]
    pub user: Pubkey,
    /// User coin token account.
    #[account]
    pub user_coin_token_account: Pubkey,
    /// User PC token account.
    #[account]
    pub user_pc_token_account: Pubkey,
    /// User LP token account.
    #[account(writable)]
    pub user_lp_token_account: Pubkey,
}

/// Initialize2 instruction parameters.
///
/// Uses 1-byte instruction index (0x01).
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = [INITIALIZE2_IX], discriminator_size = 1)]
pub struct Initialize2Params {
    /// Nonce for PDA derivation.
    pub nonce: u8,
    /// UTC timestamp for pool open.
    pub open_time: u64,
    /// Initial PC (quote) amount.
    pub init_pc_amount: u64,
    /// Initial coin (base) amount.
    pub init_coin_amount: u64,
}

impl Initialize2Params {
    /// Create new Initialize2 parameters.
    #[must_use]
    pub fn new(nonce: u8, open_time: u64, init_pc_amount: u64, init_coin_amount: u64) -> Self {
        Self {
            nonce,
            open_time,
            init_pc_amount,
            init_coin_amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn initialize2_params_roundtrip() {
        let params = Initialize2Params::new(255, 1_700_000_000, 500_000_000, 1_000_000);
        let data = params.to_data();

        // 1 disc + 1 nonce + 8 open_time + 8 pc + 8 coin = 26
        assert_eq!(data.len(), 26);
        assert_eq!(data[0], INITIALIZE2_IX);

        let parsed = Initialize2Params::from_instruction_data(&data[1..]).unwrap();
        assert_eq!(parsed.nonce, 255);
        assert_eq!(parsed.open_time, 1_700_000_000);
        assert_eq!(parsed.init_pc_amount, 500_000_000);
        assert_eq!(parsed.init_coin_amount, 1_000_000);
    }

    #[test]
    fn initialize2_accounts_count() {
        assert_eq!(Initialize2Accounts::ACCOUNT_COUNT, 21);
    }
}
