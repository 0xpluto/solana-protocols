//! PumpSwap protocol constants.
//!
//! These values are derived from the on-chain program.

use solana_program::pubkey::Pubkey;

/// PumpSwap program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");

/// Global configuration account.
pub const GLOBAL_CONFIG: Pubkey =
    solana_program::pubkey!("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw");

/// Protocol fee recipient.
pub const PROTOCOL_FEE_RECIPIENT: Pubkey =
    solana_program::pubkey!("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");

/// Protocol fee recipient token account.
pub const PROTOCOL_FEE_RECIPIENT_TOKEN_ACCOUNT: Pubkey =
    solana_program::pubkey!("94qWNrtmfn42h3ZjUZwWvK1MEo9uVmmrBPd2hpNjYDjb");

/// Event authority PDA.
pub const EVENT_AUTHORITY: Pubkey =
    solana_program::pubkey!("GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR");

/// Global volume accumulator.
pub const GLOBAL_VOLUME_ACCUMULATOR: Pubkey =
    solana_program::pubkey!("C2aFPdENg4A2HQsmrd5rTw5TaYBX5Ku887cWjbFKtZpw");

/// Fee config PDA.
pub const FEE_CONFIG: Pubkey =
    solana_program::pubkey!("5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx");

/// Fee program (shared with Pumpfun).
pub const FEE_PROGRAM: Pubkey =
    solana_program::pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");

// ---------------------------------------------------------------------------
// Instruction discriminators (Anchor: first 8 bytes of SHA256)
// ---------------------------------------------------------------------------

/// `buy` instruction discriminator — `sha256("global:buy")[..8]`, derived.
pub const BUY_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy");

/// `buy_exact_quote_in` instruction discriminator, derived.
///
/// The exact-IN sibling of `buy`: the trader pins the quote (SOL) they spend
/// and floors the base they receive, where `buy` pins the base out and ceilings
/// the quote in. Same accounts, same emitted `BuyEvent` — the difference is
/// which side the user fixed, which decides rounding direction.
pub const BUY_EXACT_QUOTE_IN_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy_exact_quote_in");

/// `sell` instruction discriminator — `sha256("global:sell")[..8]`, derived.
pub const SELL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("sell");

/// `withdraw` instruction discriminator — `sha256("global:withdraw")[..8]`, derived.
pub const WITHDRAW_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("withdraw");

/// `deposit` instruction discriminator — `sha256("global:deposit")[..8]`, derived.
pub const DEPOSIT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("deposit");

/// `create_pool` instruction discriminator — `sha256("global:create_pool")[..8]`, derived.
pub const CREATE_POOL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("create_pool");

// ---------------------------------------------------------------------------
// Pool state layout
// ---------------------------------------------------------------------------

/// Pool state account discriminator — first 8 bytes of `sha256("account:Pool")`,
/// derived at compile time. Verified against on-chain: mainnet Pool accounts under
/// this program carry exactly these bytes (see the test below), which is also the
/// standard Anchor derivation. Previously a `[0x00; 8]` placeholder that matched
/// zero real pools — the bug that motivated the compile-time derivation.
pub const POOL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("Pool");

/// Full/modern pool account size: 8 (discriminator) + 293 = 301.
pub const POOL_ACCOUNT_SIZE: usize = 301;

// ---------------------------------------------------------------------------
// Fee constants
// ---------------------------------------------------------------------------

/// Fee denominator for basis point calculations.
pub const FEE_DENOMINATOR: u64 = 10000;

/// SOL decimals.
pub const SOL_DECIMALS: u8 = 9;

/// Token decimals (pumpfun tokens are always 6 decimals).
pub const TOKEN_DECIMALS: u8 = 6;

/// 1 SOL in lamports.
pub const SOL_LAMPORTS: u64 = 1_000_000_000;

// ---------------------------------------------------------------------------
// PDA seeds
// ---------------------------------------------------------------------------

/// Seed for creator vault authority PDA.
pub const CREATOR_VAULT_SEED: &[u8] = b"creator_vault";

/// Seed for user volume accumulator PDA.
pub const USER_VOLUME_ACCUMULATOR_SEED: &[u8] = b"user_volume_accumulator";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_is_valid() {
        assert_ne!(PROGRAM_ID, Pubkey::default());
    }

    #[test]
    fn discriminator_lengths() {
        assert_eq!(BUY_DISCRIMINATOR.len(), 8);
        assert_eq!(SELL_DISCRIMINATOR.len(), 8);
        assert_eq!(CREATE_POOL_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn pool_account_size() {
        assert_eq!(POOL_ACCOUNT_SIZE, 301);
    }

    /// Pins the derived discriminator to the value observed on mainnet: every
    /// account under the PumpSwap program carrying `account:Pool` state has
    /// exactly these 8 bytes (measured from the live firehose, 2026-08-09). This
    /// is the chain-truth check the old placeholder never had — if the derivation
    /// or the account name ever drifts, this fails loudly.
    #[test]
    fn pool_discriminator_matches_onchain() {
        assert_eq!(
            POOL_DISCRIMINATOR,
            [241, 154, 109, 4, 17, 177, 109, 188],
            "PumpSwap Pool discriminator drifted from the on-chain value"
        );
        assert_ne!(
            POOL_DISCRIMINATOR, [0u8; 8],
            "must never be the placeholder"
        );
    }

    #[test]
    fn buy_sell_discriminators_differ() {
        assert_ne!(BUY_DISCRIMINATOR, SELL_DISCRIMINATOR);
    }
}
