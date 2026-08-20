//! Raydium CPMM swap instruction types.
//!
//! Both SwapBaseInput and SwapBaseOutput share the same 13-account layout.
//! CPMM has actual mints at indices \[10\] and \[11\].

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};
use solana_sdk::instruction::Instruction;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

use super::super::accounts::RaydiumCpmmKeys;
use super::super::constants::{
    AUTHORITY, PROGRAM_ID, SWAP_BASE_INPUT_DISCRIMINATOR, SWAP_BASE_OUTPUT_DISCRIMINATOR,
};
use crate::traits::InstructionBuilder;

/// Swap instruction accounts (shared by SwapBaseInput and SwapBaseOutput).
///
/// Account indices from Anchor IDL:
/// \[0\]=payer(signer,writable), \[1\]=authority, \[2\]=amm_config,
/// \[3\]=pool_state(writable), \[4\]=input_token_account(writable),
/// \[5\]=output_token_account(writable), \[6\]=input_vault(writable),
/// \[7\]=output_vault(writable), \[8\]=input_token_program, \[9\]=output_token_program,
/// \[10\]=input_token_mint, \[11\]=output_token_mint, \[12\]=observation_state(writable)
#[derive(Debug, Clone, AccountMetas)]
#[accounts(unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have")]
pub struct SwapAccounts {
    /// User wallet (signer).
    #[account(signer, writable)]
    pub payer: Pubkey,
    /// Vault authority PDA.
    #[account]
    pub authority: Pubkey,
    /// AMM config account.
    #[account]
    pub amm_config: Pubkey,
    /// Pool state account.
    #[account(writable)]
    pub pool_state: Pubkey,
    /// User's input token account.
    #[account(writable)]
    pub input_token_account: Pubkey,
    /// User's output token account.
    #[account(writable)]
    pub output_token_account: Pubkey,
    /// Pool's input token vault.
    #[account(writable)]
    pub input_vault: Pubkey,
    /// Pool's output token vault.
    #[account(writable)]
    pub output_vault: Pubkey,
    /// Input token program (SPL Token or Token2022).
    #[account]
    pub input_token_program: Pubkey,
    /// Output token program.
    #[account]
    pub output_token_program: Pubkey,
    /// Input token mint.
    #[account]
    pub input_token_mint: Pubkey,
    /// Output token mint.
    #[account]
    pub output_token_mint: Pubkey,
    /// Oracle observation state.
    #[account(writable)]
    pub observation_state: Pubkey,
}

/// SwapBaseInput instruction parameters.
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = SWAP_BASE_INPUT_DISCRIMINATOR, unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; pinning params here would claim a verification the rest of the vertical does not have")]
pub struct SwapBaseInputParams {
    /// Input token amount.
    pub amount_in: u64,
    /// Minimum output tokens (slippage protection).
    pub minimum_amount_out: u64,
}

impl SwapBaseInputParams {
    /// Create new SwapBaseInput parameters.
    #[must_use]
    pub fn new(amount_in: u64, minimum_amount_out: u64) -> Self {
        Self {
            amount_in,
            minimum_amount_out,
        }
    }
}

/// SwapBaseOutput instruction parameters.
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = SWAP_BASE_OUTPUT_DISCRIMINATOR, unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; pinning params here would claim a verification the rest of the vertical does not have")]
pub struct SwapBaseOutputParams {
    /// Maximum input tokens (slippage protection).
    pub maximum_amount_in: u64,
    /// Output token amount.
    pub amount_out: u64,
}

impl SwapBaseOutputParams {
    /// Create new SwapBaseOutput parameters.
    #[must_use]
    pub fn new(maximum_amount_in: u64, amount_out: u64) -> Self {
        Self {
            maximum_amount_in,
            amount_out,
        }
    }
}

impl SwapAccounts {
    /// Build swap accounts from pool keys, user wallet, and direction.
    ///
    /// `is_buy` = true means SOL (token_1) in → token_0 out.
    /// `is_buy` = false means token_0 in → SOL (token_1) out.
    #[must_use]
    pub fn from_keys(keys: &RaydiumCpmmKeys, user: &Pubkey, is_buy: bool) -> Self {
        let (input_mint, output_mint) = if is_buy {
            (keys.token_1_mint, keys.token_0_mint)
        } else {
            (keys.token_0_mint, keys.token_1_mint)
        };

        let (input_vault, output_vault) = if is_buy {
            (keys.token_1_vault, keys.token_0_vault)
        } else {
            (keys.token_0_vault, keys.token_1_vault)
        };

        let (input_token_program, output_token_program) = if is_buy {
            (keys.token_1_program, keys.token_0_program)
        } else {
            (keys.token_0_program, keys.token_1_program)
        };

        let input_token_account =
            spl_associated_token_account::get_associated_token_address_with_program_id(
                user,
                &input_mint,
                &input_token_program,
            );
        let output_token_account =
            spl_associated_token_account::get_associated_token_address_with_program_id(
                user,
                &output_mint,
                &output_token_program,
            );

        Self {
            payer: *user,
            authority: AUTHORITY,
            amm_config: keys.amm_config,
            pool_state: keys.pool_id,
            input_token_account,
            output_token_account,
            input_vault,
            output_vault,
            input_token_program,
            output_token_program,
            input_token_mint: input_mint,
            output_token_mint: output_mint,
            observation_state: keys.observation_key,
        }
    }
}

/// Builder configuration for CPMM swaps.
#[derive(Debug, Clone)]
pub struct SwapBaseInputBuilderConfig {
    /// Pool keys.
    pub keys: RaydiumCpmmKeys,
    /// Whether this is a buy (SOL → token) or sell (token → SOL).
    pub is_buy: bool,
}

/// Builder for CPMM SwapBaseInput instructions.
pub struct SwapBaseInputBuilder;

impl InstructionBuilder for SwapBaseInputBuilder {
    type Keys = SwapBaseInputBuilderConfig;
    type Params = SwapBaseInputParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> crate::Result<Instruction> {
        let accounts = SwapAccounts::from_keys(&keys.keys, user, keys.is_buy);
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
            // Create the output ATA (input ATA should already have tokens)
            let (output_mint, output_program) = if keys.is_buy {
                (keys.keys.token_0_mint, keys.keys.token_0_program)
            } else {
                (keys.keys.token_1_mint, keys.keys.token_1_program)
            };

            instructions.push(create_associated_token_account_idempotent(
                user,
                user,
                &output_mint,
                &output_program,
            ));
        }

        instructions.push(Self::build_swap_instruction(keys, user, params)?);
        Ok(instructions)
    }
}

impl SwapBaseInputBuilder {
    /// Build a buy instruction (SOL → token_0).
    pub fn buy(
        keys: &RaydiumCpmmKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Instruction> {
        let config = SwapBaseInputBuilderConfig {
            keys: keys.clone(),
            is_buy: true,
        };
        Self::build_swap_instruction(
            &config,
            user,
            SwapBaseInputParams::new(amount_in, minimum_amount_out),
        )
    }

    /// Build a sell instruction (token_0 → SOL).
    pub fn sell(
        keys: &RaydiumCpmmKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Instruction> {
        let config = SwapBaseInputBuilderConfig {
            keys: keys.clone(),
            is_buy: false,
        };
        Self::build_swap_instruction(
            &config,
            user,
            SwapBaseInputParams::new(amount_in, minimum_amount_out),
        )
    }

    /// Build a buy instruction with optional ATA creation.
    pub fn buy_with_ata(
        keys: &RaydiumCpmmKeys,
        user: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> crate::Result<Vec<Instruction>> {
        let config = SwapBaseInputBuilderConfig {
            keys: keys.clone(),
            is_buy: true,
        };
        Self::build_swap_with_setup(
            &config,
            user,
            SwapBaseInputParams::new(amount_in, minimum_amount_out),
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    fn make_test_keys() -> RaydiumCpmmKeys {
        RaydiumCpmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_0_mint: Pubkey::new_unique(),
            token_1_mint: Pubkey::new_unique(),
            token_0_vault: Pubkey::new_unique(),
            token_1_vault: Pubkey::new_unique(),
            token_0_program: spl_token::id(),
            token_1_program: spl_token::id(),
            observation_key: Pubkey::new_unique(),
        }
    }

    #[test]
    fn swap_base_input_roundtrip() {
        let params = SwapBaseInputParams::new(1_000_000_000, 900_000);
        let data = params.to_data();

        assert_eq!(data.len(), 24); // 8 disc + 8 + 8
        assert_eq!(&data[..8], &SWAP_BASE_INPUT_DISCRIMINATOR);

        let parsed = SwapBaseInputParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_in, 1_000_000_000);
        assert_eq!(parsed.minimum_amount_out, 900_000);
    }

    #[test]
    fn swap_base_output_roundtrip() {
        let params = SwapBaseOutputParams::new(1_100_000_000, 1_000_000);
        let data = params.to_data();

        assert_eq!(data.len(), 24); // 8 disc + 8 + 8
        assert_eq!(&data[..8], &SWAP_BASE_OUTPUT_DISCRIMINATOR);

        let parsed = SwapBaseOutputParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.maximum_amount_in, 1_100_000_000);
        assert_eq!(parsed.amount_out, 1_000_000);
    }

    #[test]
    fn swap_accounts_count() {
        assert_eq!(SwapAccounts::ACCOUNT_COUNT, 13);
    }

    #[test]
    fn accounts_from_keys_buy() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let accounts = SwapAccounts::from_keys(&keys, &user, true);
        assert_eq!(accounts.payer, user);
        assert_eq!(accounts.authority, AUTHORITY);
        assert_eq!(accounts.pool_state, keys.pool_id);
        // Buy: SOL (token_1) in → token_0 out
        assert_eq!(accounts.input_vault, keys.token_1_vault);
        assert_eq!(accounts.output_vault, keys.token_0_vault);
        assert_eq!(accounts.input_token_mint, keys.token_1_mint);
        assert_eq!(accounts.output_token_mint, keys.token_0_mint);
    }

    #[test]
    fn accounts_from_keys_sell() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let accounts = SwapAccounts::from_keys(&keys, &user, false);
        // Sell: token_0 in → SOL (token_1) out
        assert_eq!(accounts.input_vault, keys.token_0_vault);
        assert_eq!(accounts.output_vault, keys.token_1_vault);
        assert_eq!(accounts.input_token_mint, keys.token_0_mint);
        assert_eq!(accounts.output_token_mint, keys.token_1_mint);
    }

    #[test]
    fn builder_creates_buy_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = SwapBaseInputBuilder::buy(&keys, &user, 1_000_000_000, 900_000).unwrap();
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 13);
        // Data: 8 disc + 8 amount_in + 8 minimum_amount_out = 24
        assert_eq!(ix.data.len(), 24);
        assert_eq!(&ix.data[..8], &SWAP_BASE_INPUT_DISCRIMINATOR);
    }

    #[test]
    fn builder_creates_sell_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = SwapBaseInputBuilder::sell(&keys, &user, 500_000_000, 400_000).unwrap();
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 13);
    }

    #[test]
    fn builder_buy_with_ata() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ixs = SwapBaseInputBuilder::buy_with_ata(&keys, &user, 1_000_000_000, 900_000).unwrap();
        assert_eq!(ixs.len(), 2); // ATA creation + swap
        assert_eq!(ixs[1].program_id, PROGRAM_ID);
    }
}
