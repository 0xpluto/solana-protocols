//! Raydium Launchpad protocol constants.
//!
//! Derived from Anchor IDL.

use solana_program::pubkey::Pubkey;

/// Raydium Launchpad program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");

// ---------------------------------------------------------------------------
// Instruction discriminators (Anchor: first 8 bytes of SHA256("global:<name>"))
// ---------------------------------------------------------------------------

/// BuyExactIn instruction discriminator.
pub const BUY_EXACT_IN_DISCRIMINATOR: [u8; 8] = [250, 234, 13, 123, 213, 156, 19, 236];

/// BuyExactOut instruction discriminator.
pub const BUY_EXACT_OUT_DISCRIMINATOR: [u8; 8] = [24, 211, 116, 40, 105, 3, 153, 56];

/// SellExactIn instruction discriminator.
pub const SELL_EXACT_IN_DISCRIMINATOR: [u8; 8] = [149, 39, 222, 155, 211, 124, 152, 26];

/// SellExactOut instruction discriminator.
pub const SELL_EXACT_OUT_DISCRIMINATOR: [u8; 8] = [95, 200, 71, 34, 8, 9, 11, 166];

// ---------------------------------------------------------------------------
// Fee constants
// ---------------------------------------------------------------------------

/// Fee rate denominator (1 million).
pub const FEE_RATE_DENOMINATOR: u64 = 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_is_valid() {
        assert_ne!(PROGRAM_ID, Pubkey::default());
    }

    #[test]
    fn discriminator_lengths() {
        assert_eq!(BUY_EXACT_IN_DISCRIMINATOR.len(), 8);
        assert_eq!(BUY_EXACT_OUT_DISCRIMINATOR.len(), 8);
        assert_eq!(SELL_EXACT_IN_DISCRIMINATOR.len(), 8);
        assert_eq!(SELL_EXACT_OUT_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn all_discriminators_unique() {
        let discs = [
            BUY_EXACT_IN_DISCRIMINATOR,
            BUY_EXACT_OUT_DISCRIMINATOR,
            SELL_EXACT_IN_DISCRIMINATOR,
            SELL_EXACT_OUT_DISCRIMINATOR,
        ];
        for i in 0..discs.len() {
            for j in (i + 1)..discs.len() {
                assert_ne!(
                    discs[i], discs[j],
                    "discriminators at {} and {} collide",
                    i, j
                );
            }
        }
    }
}
