//! PumpSwap `buy_exact_quote_in` — exact-in buy.
//!
//! The same 23 accounts as [`buy`](super::buy), pinning the opposite side: `buy`
//! pins the base it delivers, this pins the quote it spends. The accounts struct
//! is a copy rather than a shared alias, so this discriminator's own IDL entry is
//! checked and its own fixtures pin only itself. A shared struct validated
//! against `buy`'s IDL entry alone, and would have kept doing so if the two ever
//! diverged.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};

use super::super::constants::BUY_EXACT_QUOTE_IN_DISCRIMINATOR;
use crate::parsing::accounts::Conditional;

/// Account list for PumpSwap `buy_exact_quote_in`.
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[idl(program = "pump_amm", instruction = "buy_exact_quote_in")]
#[onchain_ix(fixtures(
    "pumpswap/ix_buy_exact_quote_in_n25.json",
    "pumpswap/ix_buy_exact_quote_in_n26.json",
    "pumpswap/ix_buy_exact_quote_in_n27.json",
    "pumpswap/ix_buy_exact_quote_in_n30.json"
))]
pub struct BuyExactQuoteInAccounts {
    /// Pool state account.
    #[account(writable)]
    pub pool: Pubkey,
    /// User wallet (signer).
    #[account(writable, signer)]
    pub user: Pubkey,
    /// PumpSwap global configuration.
    #[account]
    pub global_config: Pubkey,
    /// Base token mint (the meme token).
    #[account]
    pub base_mint: Pubkey,
    /// Quote token mint (WSOL).
    #[account]
    pub quote_mint: Pubkey,
    /// User's base token account.
    #[account(writable)]
    pub user_base_token_account: Pubkey,
    /// User's quote token account.
    #[account(writable)]
    pub user_quote_token_account: Pubkey,
    /// Pool's base token vault.
    #[account(writable)]
    pub pool_base_token_account: Pubkey,
    /// Pool's quote token vault.
    #[account(writable)]
    pub pool_quote_token_account: Pubkey,
    /// Protocol fee recipient.
    #[account]
    pub protocol_fee_recipient: Pubkey,
    /// Protocol fee recipient token account.
    #[account(writable)]
    pub protocol_fee_recipient_token_account: Pubkey,
    /// Base token program (SPL Token or Token-2022).
    #[account]
    pub base_token_program: Pubkey,
    /// Quote token program.
    #[account]
    pub quote_token_program: Pubkey,
    /// System program.
    #[account]
    pub system_program: Pubkey,
    /// Associated token program.
    #[account]
    pub associated_token_program: Pubkey,
    /// Event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// PumpSwap program.
    #[account]
    pub program: Pubkey,
    /// Creator vault ATA (receives creator fees).
    #[account(writable)]
    pub coin_creator_vault_ata: Pubkey,
    /// Creator vault authority PDA.
    #[account]
    pub coin_creator_vault_authority: Pubkey,
    /// Global volume accumulator PDA.
    #[account]
    pub global_volume_accumulator: Pubkey,
    /// User volume accumulator PDA.
    #[account(writable)]
    pub user_volume_accumulator: Pubkey,
    /// Fee config PDA.
    #[account]
    pub fee_config: Pubkey,
    /// Fee program.
    #[account]
    pub fee_program: Pubkey,

    /// The quote-token account of the cashback volume accumulator.
    ///
    /// `ATA(user_volume_accumulator, quote_mint)`. Pump's cashback docs name it
    /// as the 0th appended account on a PumpSwap buy, and mainnet agrees: it is
    /// the first tail entry on the 27-account buys.
    #[account(resolved = spl_associated_token_account::get_associated_token_address(
        &user_volume_accumulator,
        &quote_mint,
    ))]
    pub appended_quote_volume_accumulator: Conditional,
    /// Fee destinations from the `pump_fees` global roster.
    #[account(
        remaining,
        reason = "the pump_fees BuybackVault roster: eight exist on chain and a \
                  caller credits any subset, so which ones appear is caller policy \
                  rather than layout, and none is derivable from this instruction"
    )]
    pub buyback_vaults: Vec<Pubkey>,
}

/// PumpSwap `buy_exact_quote_in` parameters.
///
/// Mirror image of [`BuyParams`]: the trader pins the quote spent rather than
/// the base received. Executed amounts still come from the emitted `BuyEvent`,
/// so extraction is shared — these params are slippage bounds, as ever.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = BUY_EXACT_QUOTE_IN_DISCRIMINATOR, fixtures(
    "pumpswap/ix_buy_exact_quote_in_n25.json",
    "pumpswap/ix_buy_exact_quote_in_n26.json",
    "pumpswap/ix_buy_exact_quote_in_n27.json",
    "pumpswap/ix_buy_exact_quote_in_n30.json"
))]
pub struct BuyExactQuoteInParams {
    /// Quote (SOL) the trader is willing to spend — the pinned side.
    pub spendable_quote_in: u64,
    /// Minimum base tokens to accept.
    pub min_base_amount_out: u64,
    /// Trailing `OptionBool` — see [`BuyParams::track_volume`].
    pub track_volume: crate::protocols::OptionBool,
}

impl crate::pairs::NamesPair for BuyExactQuoteInAccounts {
    fn pair(&self) -> (Pubkey, Pubkey) {
        (self.base_mint, self.quote_mint)
    }
}

impl crate::pairs::SwapAccounts for BuyExactQuoteInAccounts {
    fn pool(&self) -> Pubkey {
        self.pool
    }
}
