//! PumpSwap `collect_coin_creator_fee` instruction.
//!
//! The coin creator withdrawing fees the AMM accrued for them. Zero arguments,
//! so the economics live entirely in
//! [`CollectCoinCreatorFeeEvent`](crate::protocols::pumpswap::events::CollectCoinCreatorFeeEvent).
//!
//! Unlike pumpfun's creator-fee instructions this one *is* slot-decodable:
//! mainnet sends exactly the eight accounts the IDL declares. The struct exists
//! because the account list names the vault the fees came out of, which the
//! event only partly repeats.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

/// Account list for `collect_coin_creator_fee`. 8 slots.
#[derive(Debug, Clone, AccountMetas)]
#[idl(program = "pump_amm", instruction = "collect_coin_creator_fee")]
pub struct CollectCoinCreatorFeeAccounts {
    /// Mint the fees are denominated in.
    #[account]
    pub quote_mint: Pubkey,
    /// Token program owning `quote_mint`.
    #[account]
    pub quote_token_program: Pubkey,
    /// The creator being paid.
    #[account]
    pub coin_creator: Pubkey,
    /// PDA authority over the fee vault.
    #[account]
    pub coin_creator_vault_authority: Pubkey,
    /// Vault the fees accrued in.
    #[account(writable)]
    pub coin_creator_vault_ata: Pubkey,
    /// Destination token account.
    #[account(writable)]
    pub coin_creator_token_account: Pubkey,
    /// Anchor event authority.
    #[account]
    pub event_authority: Pubkey,
    /// The program itself.
    #[account]
    pub program: Pubkey,
}
