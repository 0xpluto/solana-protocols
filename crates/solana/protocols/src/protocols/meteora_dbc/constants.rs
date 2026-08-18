//! Meteora DBC (Dynamic Bonding Curve) protocol constants.
//!
//! Derived from Anchor IDL and on-chain program analysis.

use solana_program::pubkey::Pubkey;

/// Meteora DBC program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN");

/// Meteora DBC pool authority PDA (constant for all pools).
pub const POOL_AUTHORITY: Pubkey =
    solana_program::pubkey!("FhVo3mqL8PW5pH5U2CN4XE33DokiyZnUwuGpH2hmHLuM");

// ---------------------------------------------------------------------------
// Instruction discriminators (Anchor: first 8 bytes of SHA256("global:<name>"))
// ---------------------------------------------------------------------------

/// Swap instruction discriminator (legacy, exact-in only).
/// Note: Same bytes as DLMM swap — both use "global:swap". Different programs.
pub const SWAP_DISCRIMINATOR: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];

/// Swap2 instruction discriminator (supports ExactIn, PartialFill, ExactOut).
pub const SWAP2_DISCRIMINATOR: [u8; 8] = [65, 75, 63, 76, 235, 91, 91, 136];

// ---------------------------------------------------------------------------
// Fee constants
// ---------------------------------------------------------------------------

/// Fee rate denominator (1 billion = 100%).
pub const FEE_DENOMINATOR: u64 = 1_000_000_000;

/// `sha256("account:VirtualPool")[..8]`, derived at compile time.
///
/// Account identity on Solana is (owner program, discriminator, PDA) — the
/// discriminator alone is not unique. `account:PoolState` is shared by at least
/// three programs in this crate, so a decoder that checks only these eight bytes
/// will happily read one program's account as another's.
pub const DBC_VIRTUAL_POOL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("VirtualPool");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_is_valid() {
        assert_ne!(PROGRAM_ID, Pubkey::default());
    }

    #[test]
    fn discriminator_lengths() {
        assert_eq!(SWAP_DISCRIMINATOR.len(), 8);
        assert_eq!(SWAP2_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn discriminators_differ() {
        assert_ne!(SWAP_DISCRIMINATOR, SWAP2_DISCRIMINATOR);
    }
}
