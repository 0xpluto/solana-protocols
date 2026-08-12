//! Raydium Launchpad swap instruction types.
//!
//! All 4 swap variants (BuyExactIn, BuyExactOut, SellExactIn, SellExactOut)
//! share the same 15-account layout. Actual mints at indices \[9\] and \[10\].

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};
use solana_sdk::instruction::Instruction;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

use super::super::accounts::LaunchpadKeys;
use super::super::constants::{
    BUY_EXACT_IN_DISCRIMINATOR, BUY_EXACT_OUT_DISCRIMINATOR, PROGRAM_ID,
    SELL_EXACT_IN_DISCRIMINATOR, SELL_EXACT_OUT_DISCRIMINATOR,
};
use crate::traits::InstructionBuilder;

/// Swap instruction accounts (shared by all 4 swap variants).
///
/// Account indices from Anchor IDL:
/// \[0\]=payer(signer,writable), \[1\]=authority, \[2\]=global_config,
/// \[3\]=platform_config, \[4\]=pool_state(writable),
/// \[5\]=user_base_token(writable), \[6\]=user_quote_token(writable),
/// \[7\]=base_vault(writable), \[8\]=quote_vault(writable),
/// \[9\]=base_token_mint, \[10\]=quote_token_mint,
/// \[11\]=base_token_program, \[12\]=quote_token_program,
/// \[13\]=event_authority, \[14\]=program
///
/// Note: Remaining accounts (system_program, fee vaults) are optional
/// and not part of the fixed layout.
#[derive(Debug, Clone, AccountMetas)]
pub struct SwapAccounts {
    /// User wallet (signer).
    #[account(signer, writable)]
    pub payer: Pubkey,
    /// Vault authority PDA.
    #[account]
    pub authority: Pubkey,
    /// Global config account.
    #[account]
    pub global_config: Pubkey,
    /// Platform config account.
    #[account]
    pub platform_config: Pubkey,
    /// Pool state account.
    #[account(writable)]
    pub pool_state: Pubkey,
    /// User's base (launchpad) token account.
    #[account(writable)]
    pub user_base_token: Pubkey,
    /// User's quote token account.
    #[account(writable)]
    pub user_quote_token: Pubkey,
    /// Pool's base token vault.
    #[account(writable)]
    pub base_vault: Pubkey,
    /// Pool's quote token vault.
    #[account(writable)]
    pub quote_vault: Pubkey,
    /// Base token mint.
    #[account]
    pub base_token_mint: Pubkey,
    /// Quote token mint.
    #[account]
    pub quote_token_mint: Pubkey,
    /// Base token program (SPL Token or Token2022).
    #[account]
    pub base_token_program: Pubkey,
    /// Quote token program.
    #[account]
    pub quote_token_program: Pubkey,
    /// Event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// Launchpad program ID.
    #[account]
    pub program: Pubkey,
}

/// BuyExactIn instruction parameters.
#[derive(Debug, Clone, InstructionData)]
#[instruction_data(discriminator = BUY_EXACT_IN_DISCRIMINATOR)]
pub struct BuyExactInParams {
    /// Quote tokens to spend (exact input).
    pub amount_in: u64,
    /// Minimum base tokens to receive (slippage protection).
    pub minimum_amount_out: u64,
    /// Share fee rate (optional, usually 0).
    pub share_fee_rate: u64,
}

impl BuyExactInParams {
    /// Create new BuyExactIn parameters.
    #[must_use]
    pub fn new(amount_in: u64, minimum_amount_out: u64) -> Self {
        Self {
            amount_in,
            minimum_amount_out,
            share_fee_rate: 0,
        }
    }
}

/// BuyExactOut instruction parameters.
#[derive(Debug, Clone, InstructionData)]
#[instruction_data(discriminator = BUY_EXACT_OUT_DISCRIMINATOR)]
pub struct BuyExactOutParams {
    /// Base tokens to receive (exact output).
    pub amount_out: u64,
    /// Maximum quote tokens to spend (slippage protection).
    pub maximum_amount_in: u64,
    /// Share fee rate (optional, usually 0).
    pub share_fee_rate: u64,
}

impl BuyExactOutParams {
    /// Create new BuyExactOut parameters.
    #[must_use]
    pub fn new(amount_out: u64, maximum_amount_in: u64) -> Self {
        Self {
            amount_out,
            maximum_amount_in,
            share_fee_rate: 0,
        }
    }
}

/// SellExactIn instruction parameters.
#[derive(Debug, Clone, InstructionData)]
#[instruction_data(discriminator = SELL_EXACT_IN_DISCRIMINATOR)]
pub struct SellExactInParams {
    /// Base tokens to sell (exact input).
    pub amount_in: u64,
    /// Minimum quote tokens to receive (slippage protection).
    pub minimum_amount_out: u64,
    /// Share fee rate (optional, usually 0).
    pub share_fee_rate: u64,
}

impl SellExactInParams {
    /// Create new SellExactIn parameters.
    #[must_use]
    pub fn new(amount_in: u64, minimum_amount_out: u64) -> Self {
        Self {
            amount_in,
            minimum_amount_out,
            share_fee_rate: 0,
        }
    }
}

/// SellExactOut instruction parameters.
#[derive(Debug, Clone, InstructionData)]
#[instruction_data(discriminator = SELL_EXACT_OUT_DISCRIMINATOR)]
pub struct SellExactOutParams {
    /// Quote tokens to receive (exact output).
    pub amount_out: u64,
    /// Maximum base tokens to sell (slippage protection).
    pub maximum_amount_in: u64,
    /// Share fee rate (optional, usually 0).
    pub share_fee_rate: u64,
}

impl SellExactOutParams {
    /// Create new SellExactOut parameters.
    #[must_use]
    pub fn new(amount_out: u64, maximum_amount_in: u64) -> Self {
        Self {
            amount_out,
            maximum_amount_in,
            share_fee_rate: 0,
        }
    }
}

impl SwapAccounts {
    /// Build swap accounts from pool keys and user wallet.
    ///
    /// The same 15-account layout is used for all 4 swap variants.
    #[must_use]
    pub fn from_keys(keys: &LaunchpadKeys, user: &Pubkey) -> Self {
        Self {
            payer: *user,
            authority: keys.authority,
            global_config: keys.global_config,
            platform_config: keys.platform_config,
            pool_state: keys.pool_id,
            user_base_token: keys.user_base_ata(user),
            user_quote_token: keys.user_quote_ata(user),
            base_vault: keys.base_vault,
            quote_vault: keys.quote_vault,
            base_token_mint: keys.base_mint,
            quote_token_mint: keys.quote_mint,
            base_token_program: keys.base_token_program,
            quote_token_program: keys.quote_token_program,
            event_authority: LaunchpadKeys::event_authority(),
            program: PROGRAM_ID,
        }
    }
}

/// Builder for Launchpad BuyExactIn instructions.
pub struct BuyExactInBuilder;

impl InstructionBuilder for BuyExactInBuilder {
    type Keys = LaunchpadKeys;
    type Params = BuyExactInParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> crate::Result<Instruction> {
        let accounts = SwapAccounts::from_keys(keys, user);
        let account_metas = accounts.to_account_metas();
        let data = params.to_data();

        Ok(Instruction::new_with_bytes(
            PROGRAM_ID,
            &data,
            account_metas,
        ))
    }

    fn build_swap_with_setup(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
        create_ata: bool,
    ) -> crate::Result<Vec<Instruction>> {
        let mut instructions = Vec::new();

        if create_ata {
            // Create base token ATA (output of buy)
            instructions.push(create_associated_token_account_idempotent(
                user,
                user,
                &keys.base_mint,
                &keys.base_token_program,
            ));
        }

        instructions.push(Self::build_swap_instruction(keys, user, params)?);
        Ok(instructions)
    }
}

impl BuyExactInBuilder {
    /// Build a buy instruction (quote → base).
    pub fn buy(
        keys: &LaunchpadKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Instruction> {
        Self::build_swap_instruction(
            keys,
            user,
            BuyExactInParams::new(amount_in, minimum_amount_out),
        )
    }

    /// Build a buy instruction with ATA creation.
    pub fn buy_with_ata(
        keys: &LaunchpadKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Vec<Instruction>> {
        Self::build_swap_with_setup(
            keys,
            user,
            BuyExactInParams::new(amount_in, minimum_amount_out),
            true,
        )
    }
}

/// Builder for Launchpad SellExactIn instructions.
pub struct SellExactInBuilder;

impl InstructionBuilder for SellExactInBuilder {
    type Keys = LaunchpadKeys;
    type Params = SellExactInParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> crate::Result<Instruction> {
        let accounts = SwapAccounts::from_keys(keys, user);
        let account_metas = accounts.to_account_metas();
        let data = params.to_data();

        Ok(Instruction::new_with_bytes(
            PROGRAM_ID,
            &data,
            account_metas,
        ))
    }
}

impl SellExactInBuilder {
    /// Build a sell instruction (base → quote).
    pub fn sell(
        keys: &LaunchpadKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Instruction> {
        Self::build_swap_instruction(
            keys,
            user,
            SellExactInParams::new(amount_in, minimum_amount_out),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    fn make_test_keys() -> LaunchpadKeys {
        LaunchpadKeys {
            pool_id: Pubkey::new_unique(),
            authority: Pubkey::new_unique(),
            global_config: Pubkey::new_unique(),
            platform_config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            base_token_program: spl_token::id(),
            quote_token_program: spl_token::id(),
        }
    }

    #[test]
    fn buy_exact_in_roundtrip() {
        let params = BuyExactInParams::new(500_000_000, 900_000);
        let data = params.to_data();

        assert_eq!(data.len(), 32); // 8 disc + 8 + 8 + 8
        assert_eq!(&data[..8], &BUY_EXACT_IN_DISCRIMINATOR);

        let parsed = BuyExactInParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_in, 500_000_000);
        assert_eq!(parsed.minimum_amount_out, 900_000);
        assert_eq!(parsed.share_fee_rate, 0);
    }

    #[test]
    fn buy_exact_out_roundtrip() {
        let params = BuyExactOutParams::new(1_000_000, 1_100_000_000);
        let data = params.to_data();

        assert_eq!(data.len(), 32);
        assert_eq!(&data[..8], &BUY_EXACT_OUT_DISCRIMINATOR);

        let parsed = BuyExactOutParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_out, 1_000_000);
        assert_eq!(parsed.maximum_amount_in, 1_100_000_000);
    }

    #[test]
    fn sell_exact_in_roundtrip() {
        let params = SellExactInParams::new(1_000_000, 400_000_000);
        let data = params.to_data();

        assert_eq!(data.len(), 32);
        assert_eq!(&data[..8], &SELL_EXACT_IN_DISCRIMINATOR);

        let parsed = SellExactInParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_in, 1_000_000);
        assert_eq!(parsed.minimum_amount_out, 400_000_000);
    }

    #[test]
    fn sell_exact_out_roundtrip() {
        let params = SellExactOutParams::new(400_000_000, 1_100_000);
        let data = params.to_data();

        assert_eq!(data.len(), 32);
        assert_eq!(&data[..8], &SELL_EXACT_OUT_DISCRIMINATOR);

        let parsed = SellExactOutParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_out, 400_000_000);
        assert_eq!(parsed.maximum_amount_in, 1_100_000);
    }

    #[test]
    fn swap_accounts_count() {
        assert_eq!(SwapAccounts::ACCOUNT_COUNT, 15);
    }

    #[test]
    fn accounts_from_keys() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let accounts = SwapAccounts::from_keys(&keys, &user);
        assert_eq!(accounts.payer, user);
        assert_eq!(accounts.authority, keys.authority);
        assert_eq!(accounts.pool_state, keys.pool_id);
        assert_eq!(accounts.base_vault, keys.base_vault);
        assert_eq!(accounts.quote_vault, keys.quote_vault);
        assert_eq!(accounts.base_token_mint, keys.base_mint);
        assert_eq!(accounts.quote_token_mint, keys.quote_mint);
        assert_eq!(accounts.program, PROGRAM_ID);
    }

    #[test]
    fn builder_creates_buy_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = BuyExactInBuilder::buy(&keys, &user, 1_000_000_000, 900_000).unwrap();
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 15);
        // Data: 8 disc + 8 + 8 + 8 = 32
        assert_eq!(ix.data.len(), 32);
        assert_eq!(&ix.data[..8], &BUY_EXACT_IN_DISCRIMINATOR);
    }

    #[test]
    fn builder_creates_sell_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = SellExactInBuilder::sell(&keys, &user, 500_000, 400_000_000).unwrap();
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 15);
        assert_eq!(&ix.data[..8], &SELL_EXACT_IN_DISCRIMINATOR);
    }

    #[test]
    fn builder_buy_with_ata() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ixs = BuyExactInBuilder::buy_with_ata(&keys, &user, 1_000_000_000, 900_000).unwrap();
        assert_eq!(ixs.len(), 2); // ATA + swap
        assert_eq!(ixs[1].program_id, PROGRAM_ID);
    }
}
