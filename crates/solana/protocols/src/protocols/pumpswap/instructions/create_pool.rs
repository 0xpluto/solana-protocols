//! PumpSwap CreatePool instruction types.
//!
//! Account layout derived from PumpSwap IDL + v2-crates reference.
//! `CreatePool` creates a new AMM pool with initial liquidity.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};

use super::super::constants::CREATE_POOL_DISCRIMINATOR;

/// PumpSwap CreatePool instruction accounts.
///
/// Account indices from v2-crates:
/// \[0\]=pool, \[1\]=global_config, \[2\]=creator, \[3\]=base_mint, \[4\]=quote_mint,
/// \[5\]=lp_mint, \[6\]=user_base_token_account, \[7\]=user_quote_token_account,
/// \[8\]=user_pool_token_account, \[9\]=pool_base_token_account,
/// \[10\]=pool_quote_token_account, \[11\]=base_token_program,
/// \[12\]=quote_token_program, \[13\]=lp_token_program, \[14\]=system_program,
/// \[15\]=associated_token_program, \[16\]=event_authority, \[17\]=program
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[idl(program = "pump_amm", instruction = "create_pool")]
#[onchain_ix(fixtures("pumpswap/ix_create_pool_n18.json"))]
pub struct CreatePoolAccounts {
    /// IDL slot 0.
    #[account(writable)]
    pub pool: Pubkey,
    /// IDL slot 1.
    #[account]
    pub global_config: Pubkey,
    /// IDL slot 2.
    #[account(writable, signer)]
    pub creator: Pubkey,
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
    pub system_program: Pubkey,
    /// IDL slot 12.
    #[account]
    pub token_2022_program: Pubkey,
    /// IDL slot 13.
    #[account]
    pub base_token_program: Pubkey,
    /// IDL slot 14.
    #[account]
    pub quote_token_program: Pubkey,
    /// IDL slot 15.
    #[account]
    pub associated_token_program: Pubkey,
    /// IDL slot 16.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 17.
    #[account]
    pub program: Pubkey,
}

/// PumpSwap CreatePool instruction parameters.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = CREATE_POOL_DISCRIMINATOR, fixtures(
    "pumpswap/ix_create_pool_n18.json"
), idl(program = "pump_amm", instruction = "create_pool"))]
pub struct CreatePoolParams {
    /// Pool index (used for PDA derivation).
    pub index: u16,
    /// Base tokens to deposit as initial liquidity.
    pub base_amount_in: u64,
    /// Quote tokens (SOL) to deposit as initial liquidity.
    pub quote_amount_in: u64,
    /// `coin_creator` — the launcher this pool accrues creator fees for.
    ///
    /// Declared by the IDL and **silently discarded until 2026-08-17**: the
    /// generated offset walk treated its length check as a minimum, so it read
    /// the first three arguments and ignored the remaining 41 bytes. Strict
    /// borsh refused the instruction outright, which is how it was found.
    ///
    /// It matters beyond tidiness — `ChainEvent::Migration` records a pool with
    /// no creator today, and this is where the creator was the whole time.
    pub coin_creator: Pubkey,
    /// `is_mayhem_mode` — declared by the program IDL.
    pub is_mayhem_mode: bool,
    /// `is_cashback_coin` — declared by the program IDL.
    ///
    /// Trailing, so it consumes whatever remains; see [`OptionBool`].
    ///
    /// [`OptionBool`]: crate::protocols::OptionBool
    pub is_cashback_coin: crate::protocols::OptionBool,
}

impl CreatePoolParams {
    /// Create new CreatePool parameters.
    ///
    /// `coin_creator` is required rather than defaulted: a pool's creator is
    /// who its fees accrue to, and a zero pubkey there is a claim that nobody
    /// owns them.
    #[must_use]
    pub fn new(
        index: u16,
        base_amount_in: u64,
        quote_amount_in: u64,
        coin_creator: Pubkey,
    ) -> Self {
        Self {
            index,
            base_amount_in,
            quote_amount_in,
            coin_creator,
            is_mayhem_mode: false,
            is_cashback_coin: crate::protocols::OptionBool::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn create_pool_params_roundtrip() {
        let params =
            CreatePoolParams::new(1, 1_000_000, 500_000_000, Pubkey::new_from_array([9; 32]));
        let data = params.to_data();

        // 8 disc + 2 index + 8 base + 8 quote + 32 coin_creator + 1 mayhem = 59,
        // with a trailing OptionBool::None contributing nothing.
        //
        // This asserted 26 until 2026-08-17, pinning an encoding that dropped
        // the last three declared arguments. The offset walk accepted it because
        // its length check was a minimum; strict borsh does not.
        assert_eq!(data.len(), 59);
        assert_eq!(&data[..8], &CREATE_POOL_DISCRIMINATOR);

        let parsed = CreatePoolParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.index, 1);
        assert_eq!(parsed.base_amount_in, 1_000_000);
        assert_eq!(parsed.quote_amount_in, 500_000_000);
        assert_eq!(parsed.coin_creator, Pubkey::new_from_array([9; 32]));
        assert!(!parsed.is_mayhem_mode);
    }

    #[test]
    fn create_pool_accounts_count() {
        assert_eq!(CreatePoolAccounts::ACCOUNT_COUNT, 18);
    }
}
