//! Raydium CPMM protocol constants.
//!
//! Derived from Anchor IDL (raydium_cp_swap v0.2.0).

use solana_program::pubkey::Pubkey;

/// Raydium CPMM program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");

/// CPMM authority PDA.
pub const AUTHORITY: Pubkey =
    solana_program::pubkey!("GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL");

// ---------------------------------------------------------------------------
// Instruction discriminators (Anchor: first 8 bytes of SHA256("global:<name>"))
// ---------------------------------------------------------------------------

/// SwapBaseInput instruction discriminator.
pub const SWAP_BASE_INPUT_DISCRIMINATOR: [u8; 8] = [143, 190, 90, 218, 196, 30, 51, 222];

/// SwapBaseOutput instruction discriminator.
pub const SWAP_BASE_OUTPUT_DISCRIMINATOR: [u8; 8] = [55, 217, 98, 86, 163, 74, 180, 173];

// ---------------------------------------------------------------------------
// Fee constants
// ---------------------------------------------------------------------------

/// Fee rate denominator (1 million).
pub const FEE_RATE_DENOMINATOR: u64 = 1_000_000;

/// `sha256("account:PoolState")[..8]`, derived at compile time.
///
/// Account identity on Solana is (owner program, discriminator, PDA) — the
/// discriminator alone is not unique. `account:PoolState` is shared by at least
/// three programs in this crate, so a decoder that checks only these eight bytes
/// will happily read one program's account as another's.
pub const CPMM_POOL_STATE_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("PoolState");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_is_valid() {
        assert_ne!(PROGRAM_ID, Pubkey::default());
    }

    #[test]
    fn discriminator_lengths() {
        assert_eq!(SWAP_BASE_INPUT_DISCRIMINATOR.len(), 8);
        assert_eq!(SWAP_BASE_OUTPUT_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn discriminators_differ() {
        assert_ne!(
            SWAP_BASE_INPUT_DISCRIMINATOR,
            SWAP_BASE_OUTPUT_DISCRIMINATOR
        );
    }
}
