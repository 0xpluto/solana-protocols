//! Meteora DBC swap instruction types.
//!
//! Both swap and swap2 share the same 15-account layout.
//! Actual mints at indices \[7\] (base_mint) and \[8\] (quote_mint).

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};
use solana_sdk::instruction::Instruction;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

use super::super::accounts::DbcKeys;
use super::super::constants::{
    POOL_AUTHORITY, PROGRAM_ID, SWAP2_DISCRIMINATOR, SWAP_DISCRIMINATOR,
};
use crate::traits::InstructionBuilder;

/// Swap instruction accounts (shared by swap and swap2).
///
/// Account indices from Anchor IDL:
/// \[0\]=pool_authority, \[1\]=config, \[2\]=pool(writable),
/// \[3\]=input_token_account(writable), \[4\]=output_token_account(writable),
/// \[5\]=base_vault(writable), \[6\]=quote_vault(writable),
/// \[7\]=base_mint, \[8\]=quote_mint,
/// \[9\]=payer(signer), \[10\]=token_base_program, \[11\]=token_quote_program,
/// \[12\]=referral_token_account(writable), \[13\]=event_authority, \[14\]=program
#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct SwapAccounts {
    /// Pool authority PDA (constant across all pools).
    #[account]
    pub pool_authority: Pubkey,
    /// Pool config account.
    #[account]
    pub config: Pubkey,
    /// Pool (VirtualPool) account.
    #[account(writable)]
    pub pool: Pubkey,
    /// User's input token account.
    #[account(writable)]
    pub input_token_account: Pubkey,
    /// User's output token account.
    #[account(writable)]
    pub output_token_account: Pubkey,
    /// Pool's base token vault.
    #[account(writable)]
    pub base_vault: Pubkey,
    /// Pool's quote token vault.
    #[account(writable)]
    pub quote_vault: Pubkey,
    /// Base token mint.
    #[account]
    pub base_mint: Pubkey,
    /// Quote token mint.
    #[account]
    pub quote_mint: Pubkey,
    /// User wallet (signer).
    #[account(signer)]
    pub payer: Pubkey,
    /// Base token program (SPL Token or Token2022).
    #[account]
    pub token_base_program: Pubkey,
    /// Quote token program.
    #[account]
    pub token_quote_program: Pubkey,
    /// Referral token account (optional, uses program ID if no referral).
    #[account(writable)]
    pub referral_token_account: Pubkey,
    /// Event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// DBC program ID.
    #[account]
    pub program: Pubkey,
}

/// Legacy swap instruction parameters (exact-in only).
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = SWAP_DISCRIMINATOR, unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; pinning params here would claim a verification the rest of the vertical does not have")]
pub struct SwapParams {
    /// Amount of input tokens.
    pub amount_in: u64,
    /// Minimum output tokens (slippage protection).
    pub minimum_amount_out: u64,
}

impl SwapParams {
    /// Create new Swap parameters.
    #[must_use]
    pub fn new(amount_in: u64, minimum_amount_out: u64) -> Self {
        Self {
            amount_in,
            minimum_amount_out,
        }
    }
}

/// Swap mode for swap2 instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SwapMode {
    /// Exact input amount.
    ExactIn = 0,
    /// Partial fill — may not execute fully.
    PartialFill = 1,
    /// Exact output amount.
    ExactOut = 2,
}

impl SwapMode {
    /// Parse from a u8 byte.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SwapMode::ExactIn),
            1 => Some(SwapMode::PartialFill),
            2 => Some(SwapMode::ExactOut),
            _ => None,
        }
    }
}

/// Swap2 instruction parameters (supports multiple modes).
#[derive(Debug, Clone, borsh::BorshDeserialize, borsh::BorshSerialize, InstructionData)]
#[instruction_data(discriminator = SWAP2_DISCRIMINATOR, unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; pinning params here would claim a verification the rest of the vertical does not have")]
pub struct Swap2Params {
    /// For ExactIn/PartialFill: amount_in. For ExactOut: amount_out.
    pub amount_0: u64,
    /// For ExactIn/PartialFill: minimum_amount_out. For ExactOut: maximum_amount_in.
    pub amount_1: u64,
    /// Swap mode (0=ExactIn, 1=PartialFill, 2=ExactOut).
    pub swap_mode: u8,
}

impl Swap2Params {
    /// Create new Swap2 parameters.
    #[must_use]
    pub fn new(amount_0: u64, amount_1: u64, swap_mode: SwapMode) -> Self {
        Self {
            amount_0,
            amount_1,
            swap_mode: swap_mode as u8,
        }
    }

    /// Get the swap mode.
    pub fn mode(&self) -> Option<SwapMode> {
        SwapMode::from_u8(self.swap_mode)
    }
}

impl SwapAccounts {
    /// Build swap accounts from pool keys, user wallet, and direction.
    ///
    /// `is_buy` = true → quote (SOL) in, base (token) out
    /// `is_buy` = false → base (token) in, quote (SOL) out
    #[must_use]
    pub fn from_keys(keys: &DbcKeys, user: &Pubkey, is_buy: bool) -> Self {
        let (input_token_account, output_token_account) = if is_buy {
            // Quote (SOL) in → Base (token) out
            (keys.user_quote_ata(user), keys.user_base_ata(user))
        } else {
            // Base (token) in → Quote (SOL) out
            (keys.user_base_ata(user), keys.user_quote_ata(user))
        };

        Self {
            pool_authority: POOL_AUTHORITY,
            config: keys.config,
            pool: keys.pool,
            input_token_account,
            output_token_account,
            base_vault: keys.base_vault,
            quote_vault: keys.quote_vault,
            base_mint: keys.base_mint,
            quote_mint: keys.quote_mint,
            payer: *user,
            token_base_program: keys.token_base_program,
            token_quote_program: keys.token_quote_program,
            referral_token_account: PROGRAM_ID, // No referral — use program as placeholder
            event_authority: DbcKeys::event_authority(),
            program: PROGRAM_ID,
        }
    }
}

/// Builder configuration for DBC swaps.
#[derive(Debug, Clone)]
pub struct SwapBuilderConfig {
    /// Pool keys.
    pub keys: DbcKeys,
    /// Whether this is a buy (quote→base) or sell (base→quote).
    pub is_buy: bool,
}

/// Builder for DBC Swap instructions (legacy exact-in).
pub struct SwapBuilder;

impl InstructionBuilder for SwapBuilder {
    type Keys = SwapBuilderConfig;
    type Params = SwapParams;

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
            // Create output ATA
            let (output_mint, output_program) = if keys.is_buy {
                (keys.keys.base_mint, keys.keys.token_base_program)
            } else {
                (keys.keys.quote_mint, keys.keys.token_quote_program)
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

impl SwapBuilder {
    /// Build a buy instruction (quote → base).
    pub fn buy(
        keys: &DbcKeys,
        user: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> crate::Result<Instruction> {
        let config = SwapBuilderConfig {
            keys: keys.clone(),
            is_buy: true,
        };
        Self::build_swap_instruction(&config, user, SwapParams::new(amount_in, min_amount_out))
    }

    /// Build a buy instruction with ATA creation.
    pub fn buy_with_ata(
        keys: &DbcKeys,
        user: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> crate::Result<Vec<Instruction>> {
        let config = SwapBuilderConfig {
            keys: keys.clone(),
            is_buy: true,
        };
        Self::build_swap_with_setup(
            &config,
            user,
            SwapParams::new(amount_in, min_amount_out),
            true,
        )
    }

    /// Build a sell instruction (base → quote).
    pub fn sell(
        keys: &DbcKeys,
        user: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> crate::Result<Instruction> {
        let config = SwapBuilderConfig {
            keys: keys.clone(),
            is_buy: false,
        };
        Self::build_swap_instruction(&config, user, SwapParams::new(amount_in, min_amount_out))
    }
}

/// Account list for `swap2`.
///
/// A copy of its sibling's layout rather than a shared struct: every
/// discriminator owns its own accounts struct, so each one's IDL entry is
/// checked and each one's fixtures pin only itself.
#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have"
)]
pub struct Swap2Accounts {
    /// Pool authority PDA (constant across all pools).
    #[account]
    pub pool_authority: Pubkey,
    /// Pool config account.
    #[account]
    pub config: Pubkey,
    /// Pool (VirtualPool) account.
    #[account(writable)]
    pub pool: Pubkey,
    /// User's input token account.
    #[account(writable)]
    pub input_token_account: Pubkey,
    /// User's output token account.
    #[account(writable)]
    pub output_token_account: Pubkey,
    /// Pool's base token vault.
    #[account(writable)]
    pub base_vault: Pubkey,
    /// Pool's quote token vault.
    #[account(writable)]
    pub quote_vault: Pubkey,
    /// Base token mint.
    #[account]
    pub base_mint: Pubkey,
    /// Quote token mint.
    #[account]
    pub quote_mint: Pubkey,
    /// User wallet (signer).
    #[account(signer)]
    pub payer: Pubkey,
    /// Base token program (SPL Token or Token2022).
    #[account]
    pub token_base_program: Pubkey,
    /// Quote token program.
    #[account]
    pub token_quote_program: Pubkey,
    /// Referral token account (optional, uses program ID if no referral).
    #[account(writable)]
    pub referral_token_account: Pubkey,
    /// Event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// DBC program ID.
    #[account]
    pub program: Pubkey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn swap_params_roundtrip() {
        let params = SwapParams::new(1_000_000_000, 900_000);
        let data = params.to_data();

        assert_eq!(data.len(), 24); // 8 disc + 8 + 8
        assert_eq!(&data[..8], &SWAP_DISCRIMINATOR);

        let parsed = SwapParams::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_in, 1_000_000_000);
        assert_eq!(parsed.minimum_amount_out, 900_000);
    }

    #[test]
    fn swap2_params_roundtrip() {
        let params = Swap2Params::new(1_000_000_000, 900_000, SwapMode::ExactIn);
        let data = params.to_data();

        assert_eq!(data.len(), 25); // 8 disc + 8 + 8 + 1
        assert_eq!(&data[..8], &SWAP2_DISCRIMINATOR);

        let parsed = Swap2Params::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.amount_0, 1_000_000_000);
        assert_eq!(parsed.amount_1, 900_000);
        assert_eq!(parsed.swap_mode, 0);
        assert_eq!(parsed.mode(), Some(SwapMode::ExactIn));
    }

    #[test]
    fn swap2_exact_out_mode() {
        let params = Swap2Params::new(500_000, 1_100_000_000, SwapMode::ExactOut);
        let data = params.to_data();

        let parsed = Swap2Params::from_instruction_data(&data[8..]).unwrap();
        assert_eq!(parsed.swap_mode, 2);
        assert_eq!(parsed.mode(), Some(SwapMode::ExactOut));
    }

    #[test]
    fn swap_accounts_count() {
        assert_eq!(SwapAccounts::ACCOUNT_COUNT, 15);
    }

    fn make_test_keys() -> DbcKeys {
        DbcKeys {
            pool: Pubkey::new_unique(),
            config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            token_base_program: spl_token::id(),
            token_quote_program: spl_token::id(),
        }
    }

    #[test]
    fn accounts_from_keys_buy() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();
        let accounts = SwapAccounts::from_keys(&keys, &user, true);

        assert_eq!(accounts.pool, keys.pool);
        assert_eq!(accounts.payer, user);
        assert_eq!(accounts.pool_authority, POOL_AUTHORITY);
        assert_eq!(accounts.base_mint, keys.base_mint);
        assert_eq!(accounts.quote_mint, keys.quote_mint);
        // Buy: input is quote, output is base
        assert_eq!(accounts.input_token_account, keys.user_quote_ata(&user));
        assert_eq!(accounts.output_token_account, keys.user_base_ata(&user));
    }

    #[test]
    fn accounts_from_keys_sell() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();
        let accounts = SwapAccounts::from_keys(&keys, &user, false);

        // Sell: input is base, output is quote
        assert_eq!(accounts.input_token_account, keys.user_base_ata(&user));
        assert_eq!(accounts.output_token_account, keys.user_quote_ata(&user));
    }

    #[test]
    fn builder_buy() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();
        let ix = SwapBuilder::buy(&keys, &user, 1_000_000_000, 900_000).unwrap();

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 15);
        assert_eq!(&ix.data[..8], &SWAP_DISCRIMINATOR);
    }

    #[test]
    fn builder_sell() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();
        let ix = SwapBuilder::sell(&keys, &user, 100_000_000, 500_000).unwrap();

        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 15);
    }

    #[test]
    fn builder_buy_with_ata() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();
        let ixs = SwapBuilder::buy_with_ata(&keys, &user, 1_000_000_000, 900_000).unwrap();

        assert_eq!(ixs.len(), 2); // ATA create + swap
        assert_eq!(ixs[1].program_id, PROGRAM_ID);
    }
}
