//! Raydium CLMM SwapV2 instruction types.
//!
//! Enhanced swap with Token2022 support, memo integration, and explicit mints.
//! Account layout from Raydium CLMM Anchor IDL.
//!
//! Note: This instruction has 13 fixed accounts followed by an optional
//! extended bitmap and variable-length tick array accounts.
//! Our `AccountMetas` struct covers only the 13 fixed accounts.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};
use solana_sdk::instruction::{AccountMeta, Instruction};
use spl_associated_token_account::get_associated_token_address;

use super::super::accounts::RaydiumClmmKeys;
use super::super::constants::{
    MAX_TICK, MEMO_PROGRAM_ID, MIN_TICK, PROGRAM_ID, SWAP_V2_DISCRIMINATOR,
};
use crate::error::Result;
use crate::traits::InstructionBuilder;

/// SwapV2 instruction fixed accounts.
///
/// Account indices from Anchor IDL / v2-crates reference:
/// \[0\]=payer(signer), \[1\]=amm_config, \[2\]=pool_state(writable),
/// \[3\]=input_token_account(writable), \[4\]=output_token_account(writable),
/// \[5\]=input_vault(writable), \[6\]=output_vault(writable),
/// \[7\]=observation_state(writable), \[8\]=token_program, \[9\]=token_program_2022,
/// \[10\]=memo_program, \[11\]=input_vault_mint, \[12\]=output_vault_mint
///
/// Remaining accounts: \[13\]=optional extended_bitmap, [14+]=tick arrays
#[derive(Debug, Clone, AccountMetas)]
pub struct SwapV2Accounts {
    /// User wallet (signer).
    #[account(signer)]
    pub payer: Pubkey,
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
    /// Observation state account.
    #[account(writable)]
    pub observation_state: Pubkey,
    /// SPL Token program.
    #[account]
    pub token_program: Pubkey,
    /// Token-2022 program.
    #[account]
    pub token_program_2022: Pubkey,
    /// Memo program.
    #[account]
    pub memo_program: Pubkey,
    /// Input vault mint (for Token2022 transfer hooks).
    #[account]
    pub input_vault_mint: Pubkey,
    /// Output vault mint (for Token2022 transfer hooks).
    #[account]
    pub output_vault_mint: Pubkey,
}

/// SwapV2 instruction parameters (identical to Swap v1 params).
///
/// Uses 8-byte Anchor discriminator.
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = SWAP_V2_DISCRIMINATOR)]
pub struct SwapV2Params {
    /// Input amount (or output amount if `is_base_input` is false).
    pub amount: u64,
    /// Minimum output (or maximum input if `is_base_input` is false).
    pub other_amount_threshold: u64,
    /// Price limit as sqrt_price_x64 (Q64.64 fixed-point).
    pub sqrt_price_limit_x64: u128,
    /// True if `amount` is the input amount, false if output amount.
    pub is_base_input: bool,
}

impl SwapV2Params {
    /// Create new SwapV2 parameters.
    #[must_use]
    pub fn new(
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit_x64: u128,
        is_base_input: bool,
    ) -> Self {
        Self {
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64,
            is_base_input,
        }
    }

    /// Create exact-input swap parameters with slippage.
    ///
    /// For buying token0 with token1: amount = token1 in, threshold = min token0 out.
    /// For selling token0 for token1: amount = token0 in, threshold = min token1 out.
    #[must_use]
    pub fn exact_in(amount_in: u64, expected_out: u64, slippage_bps: u16, is_buy: bool) -> Self {
        let min_out = (expected_out as u128 * (10000 - slippage_bps as u128) / 10000) as u64;
        // Buy: price goes up → use max sqrt_price. Sell: price goes down → use min sqrt_price.
        let sqrt_price_limit = if is_buy {
            sqrt_price_x64_from_tick(MAX_TICK - 1)
        } else {
            sqrt_price_x64_from_tick(MIN_TICK + 1)
        };
        Self {
            amount: amount_in,
            other_amount_threshold: min_out,
            sqrt_price_limit_x64: sqrt_price_limit,
            is_base_input: true,
        }
    }
}

impl SwapV2Accounts {
    /// Build swap accounts from pool keys and user for an exact-input swap.
    ///
    /// `is_buy` determines the direction:
    /// - buy (token1 → token0): input=token1, output=token0
    /// - sell (token0 → token1): input=token0, output=token1
    #[must_use]
    pub fn from_keys(keys: &RaydiumClmmKeys, user: &Pubkey, is_buy: bool) -> Self {
        let (input_mint, output_mint, input_vault, output_vault) = if is_buy {
            (
                &keys.token_mint_1,
                &keys.token_mint_0,
                &keys.token_vault_1,
                &keys.token_vault_0,
            )
        } else {
            (
                &keys.token_mint_0,
                &keys.token_mint_1,
                &keys.token_vault_0,
                &keys.token_vault_1,
            )
        };

        SwapV2Accounts {
            payer: *user,
            amm_config: keys.amm_config,
            pool_state: keys.pool_id,
            input_token_account: get_associated_token_address(user, input_mint),
            output_token_account: get_associated_token_address(user, output_mint),
            input_vault: *input_vault,
            output_vault: *output_vault,
            observation_state: keys.observation_key,
            token_program: spl_token::id(),
            token_program_2022: spl_token_2022::id(),
            memo_program: MEMO_PROGRAM_ID,
            input_vault_mint: *input_mint,
            output_vault_mint: *output_mint,
        }
    }
}

// =============================================================================
// Builder
// =============================================================================

/// Builder for Raydium CLMM SwapV2 instructions.
///
/// The `Keys` type includes the pool keys, a buy/sell flag, the current tick
/// (for tick array derivation), and the number of tick arrays to include.
pub struct SwapV2Builder;

/// Configuration for building a CLMM swap instruction.
#[derive(Debug, Clone)]
pub struct SwapV2BuilderConfig {
    /// Pool keys.
    pub keys: RaydiumClmmKeys,
    /// True if buying token0 with token1.
    pub is_buy: bool,
    /// Current tick index (from pool state).
    pub current_tick: i32,
    /// Number of tick arrays to include (typically 3).
    pub tick_array_count: usize,
}

impl InstructionBuilder for SwapV2Builder {
    type Keys = SwapV2BuilderConfig;
    type Params = SwapV2Params;

    fn build_swap_instruction(
        config: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> Result<Instruction> {
        let accounts = SwapV2Accounts::from_keys(&config.keys, user, config.is_buy);
        let mut account_metas = accounts.to_account_metas();

        // Append tick array accounts as remaining accounts (writable)
        let tick_arrays = config.keys.tick_arrays_for_swap(
            config.current_tick,
            config.is_buy,
            config.tick_array_count,
        );
        for tick_array in &tick_arrays {
            account_metas.push(AccountMeta::new(*tick_array, false));
        }

        Ok(Instruction {
            program_id: PROGRAM_ID,
            accounts: account_metas,
            data: params.to_data(),
        })
    }

    fn build_swap_with_setup(
        config: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
        create_ata: bool,
    ) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::with_capacity(2);

        if create_ata {
            // Create the output token ATA
            let output_mint = if config.is_buy {
                &config.keys.token_mint_0
            } else {
                &config.keys.token_mint_1
            };
            let ata_ix = spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                user,
                user,
                output_mint,
                &spl_token::id(),
            );
            instructions.push(ata_ix);
        }

        instructions.push(Self::build_swap_instruction(config, user, params)?);

        Ok(instructions)
    }
}

impl SwapV2Builder {
    /// Build a buy instruction (token1 → token0, exact input).
    ///
    /// `current_tick` from pool state is needed for tick array derivation.
    #[must_use]
    pub fn buy(
        keys: &RaydiumClmmKeys,
        user: &Pubkey,
        current_tick: i32,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Instruction {
        let config = SwapV2BuilderConfig {
            keys: keys.clone(),
            is_buy: true,
            current_tick,
            tick_array_count: 3,
        };
        let sqrt_price_limit = sqrt_price_x64_from_tick(MAX_TICK - 1);
        let params = SwapV2Params::new(amount_in, minimum_amount_out, sqrt_price_limit, true);
        Self::build_swap_instruction(&config, user, params)
            .expect("SwapV2Builder::buy should not fail")
    }

    /// Build a sell instruction (token0 → token1, exact input).
    ///
    /// `current_tick` from pool state is needed for tick array derivation.
    #[must_use]
    pub fn sell(
        keys: &RaydiumClmmKeys,
        user: &Pubkey,
        current_tick: i32,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Instruction {
        let config = SwapV2BuilderConfig {
            keys: keys.clone(),
            is_buy: false,
            current_tick,
            tick_array_count: 3,
        };
        let sqrt_price_limit = sqrt_price_x64_from_tick(MIN_TICK + 1);
        let params = SwapV2Params::new(amount_in, minimum_amount_out, sqrt_price_limit, true);
        Self::build_swap_instruction(&config, user, params)
            .expect("SwapV2Builder::sell should not fail")
    }

    /// Build a buy instruction with ATA creation.
    #[must_use]
    pub fn buy_with_ata(
        keys: &RaydiumClmmKeys,
        user: &Pubkey,
        current_tick: i32,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Vec<Instruction> {
        let config = SwapV2BuilderConfig {
            keys: keys.clone(),
            is_buy: true,
            current_tick,
            tick_array_count: 3,
        };
        let sqrt_price_limit = sqrt_price_x64_from_tick(MAX_TICK - 1);
        let params = SwapV2Params::new(amount_in, minimum_amount_out, sqrt_price_limit, true);
        Self::build_swap_with_setup(&config, user, params, true)
            .expect("SwapV2Builder::buy_with_ata should not fail")
    }
}

/// Convert a tick index to sqrt_price as Q64.64 fixed-point.
///
/// Formula: `sqrt_price = 1.0001^(tick/2)`, then shift left by 64.
#[must_use]
pub fn sqrt_price_x64_from_tick(tick: i32) -> u128 {
    // Use f64 approximation — sufficient for price limits.
    // For exact swap math, we use the precise integer approach in math.rs.
    let sqrt_price = 1.0001_f64.powf(f64::from(tick) / 2.0);
    (sqrt_price * (1u128 << 64) as f64) as u128
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    fn make_test_keys() -> RaydiumClmmKeys {
        RaydiumClmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_mint_0: Pubkey::new_unique(),
            token_mint_1: Pubkey::new_unique(),
            token_vault_0: Pubkey::new_unique(),
            token_vault_1: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            tick_spacing: 10,
        }
    }

    #[test]
    fn swap_v2_params_roundtrip() {
        let params = SwapV2Params::new(630_000_000, 5_450_147_723_573u64, 4_295_048_017u128, true);
        let data = params.to_data();

        // 8 disc + 8 amount + 8 threshold + 16 sqrt_price + 1 is_base_input = 41
        assert_eq!(data.len(), 41);
        assert_eq!(&data[..8], &SWAP_V2_DISCRIMINATOR);

        let parsed = SwapV2Params::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount, 630_000_000);
        assert_eq!(parsed.other_amount_threshold, 5_450_147_723_573u64);
        assert_eq!(parsed.sqrt_price_limit_x64, 4_295_048_017u128);
        assert!(parsed.is_base_input);
    }

    #[test]
    fn swap_v2_accounts_count() {
        assert_eq!(SwapV2Accounts::ACCOUNT_COUNT, 13);
    }

    #[test]
    fn swap_v2_params_base_output() {
        let params = SwapV2Params::new(
            500_000,
            1_000_000_000,
            79_226_673_521_066_979_257_578_248_091u128,
            false,
        );
        let data = params.to_data();
        let parsed = SwapV2Params::from_instruction_data(&data[8..]).unwrap();
        assert!(!parsed.is_base_input);
        assert_eq!(
            parsed.sqrt_price_limit_x64,
            79_226_673_521_066_979_257_578_248_091u128
        );
    }

    #[test]
    fn exact_in_slippage() {
        let params = SwapV2Params::exact_in(1_000_000_000, 100_000_000, 100, true);
        assert_eq!(params.amount, 1_000_000_000);
        assert_eq!(params.other_amount_threshold, 99_000_000); // 1% slippage
        assert!(params.is_base_input);
    }

    #[test]
    fn accounts_from_keys_buy() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let accounts = SwapV2Accounts::from_keys(&keys, &user, true);

        assert_eq!(accounts.payer, user);
        assert_eq!(accounts.amm_config, keys.amm_config);
        assert_eq!(accounts.pool_state, keys.pool_id);
        // Buy: input=token1, output=token0
        assert_eq!(accounts.input_vault, keys.token_vault_1);
        assert_eq!(accounts.output_vault, keys.token_vault_0);
        assert_eq!(accounts.input_vault_mint, keys.token_mint_1);
        assert_eq!(accounts.output_vault_mint, keys.token_mint_0);
        assert_eq!(
            accounts.input_token_account,
            get_associated_token_address(&user, &keys.token_mint_1)
        );
        assert_eq!(
            accounts.output_token_account,
            get_associated_token_address(&user, &keys.token_mint_0)
        );
    }

    #[test]
    fn accounts_from_keys_sell() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let accounts = SwapV2Accounts::from_keys(&keys, &user, false);

        // Sell: input=token0, output=token1
        assert_eq!(accounts.input_vault, keys.token_vault_0);
        assert_eq!(accounts.output_vault, keys.token_vault_1);
        assert_eq!(accounts.input_vault_mint, keys.token_mint_0);
        assert_eq!(accounts.output_vault_mint, keys.token_mint_1);
    }

    #[test]
    fn builder_buy_creates_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = SwapV2Builder::buy(&keys, &user, 0, 1_000_000_000, 900_000);

        assert_eq!(ix.program_id, PROGRAM_ID);
        // 13 fixed + 3 tick arrays = 16
        assert_eq!(ix.accounts.len(), 16);
        assert_eq!(&ix.data[..8], &SWAP_V2_DISCRIMINATOR);

        // User should be signer
        let user_account = ix.accounts.iter().find(|a| a.pubkey == user);
        assert!(user_account.is_some());
        assert!(user_account.unwrap().is_signer);

        // Last 3 accounts should be writable (tick arrays)
        for ta in &ix.accounts[13..] {
            assert!(ta.is_writable);
            assert!(!ta.is_signer);
        }
    }

    #[test]
    fn builder_sell_creates_instruction() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ix = SwapV2Builder::sell(&keys, &user, 100, 1_000_000, 900_000_000);

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 16);
    }

    #[test]
    fn builder_buy_with_ata() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let instructions = SwapV2Builder::buy_with_ata(&keys, &user, 0, 1_000_000_000, 900_000);

        assert_eq!(instructions.len(), 2);
        assert_eq!(
            instructions[0].program_id,
            spl_associated_token_account::id()
        );
        assert_eq!(instructions[1].program_id, PROGRAM_ID);
    }

    #[test]
    fn sqrt_price_from_tick_zero() {
        // tick=0 → price=1.0 → sqrt_price=1.0 → Q64.64 = 2^64
        let sqrt_price = sqrt_price_x64_from_tick(0);
        let expected = 1u128 << 64;
        // Allow small floating-point deviation
        let diff = sqrt_price.abs_diff(expected);
        assert!(diff < 100, "tick 0 should map to ~2^64, got {sqrt_price}");
    }

    #[test]
    fn sqrt_price_from_tick_positive() {
        // Positive tick → price > 1.0 → sqrt_price > 2^64
        let sqrt_price = sqrt_price_x64_from_tick(10000);
        assert!(sqrt_price > 1u128 << 64);
    }

    #[test]
    fn sqrt_price_from_tick_negative() {
        // Negative tick → price < 1.0 → sqrt_price < 2^64
        let sqrt_price = sqrt_price_x64_from_tick(-10000);
        assert!(sqrt_price < 1u128 << 64);
    }
}
