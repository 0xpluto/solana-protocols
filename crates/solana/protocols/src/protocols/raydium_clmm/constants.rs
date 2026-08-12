//! Raydium CLMM protocol constants.
//!
//! Derived from Anchor IDL + on-chain program analysis.

use solana_program::pubkey::Pubkey;

/// Raydium CLMM program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

/// Raydium CLMM Memo program ID (used in SwapV2).
pub const MEMO_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

// ---------------------------------------------------------------------------
// Instruction discriminators (Anchor: first 8 bytes of SHA256("global:<name>"))
// ---------------------------------------------------------------------------

/// Swap instruction discriminator.
pub const SWAP_DISCRIMINATOR: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];

/// SwapV2 instruction discriminator.
pub const SWAP_V2_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];

/// SwapRouterBaseIn instruction discriminator.
pub const SWAP_ROUTER_BASE_IN_DISCRIMINATOR: [u8; 8] = [69, 125, 115, 218, 245, 186, 242, 196];

/// CreatePool instruction discriminator.
pub const CREATE_POOL_DISCRIMINATOR: [u8; 8] = [233, 146, 209, 142, 207, 104, 64, 188];

/// OpenPosition instruction discriminator.
pub const OPEN_POSITION_DISCRIMINATOR: [u8; 8] = [135, 128, 47, 77, 15, 152, 240, 49];

/// OpenPositionV2 instruction discriminator.
pub const OPEN_POSITION_V2_DISCRIMINATOR: [u8; 8] = [77, 184, 74, 214, 112, 86, 241, 199];

/// ClosePosition instruction discriminator.
pub const CLOSE_POSITION_DISCRIMINATOR: [u8; 8] = [123, 134, 81, 0, 49, 68, 98, 98];

/// IncreaseLiquidity instruction discriminator.
pub const INCREASE_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [46, 156, 243, 118, 13, 205, 251, 178];

/// IncreaseLiquidityV2 instruction discriminator.
pub const INCREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] = [133, 29, 89, 223, 69, 238, 176, 10];

/// DecreaseLiquidity instruction discriminator.
pub const DECREASE_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [160, 38, 208, 111, 104, 91, 44, 1];

/// DecreaseLiquidityV2 instruction discriminator.
pub const DECREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] = [58, 127, 188, 62, 79, 82, 196, 96];

// ---------------------------------------------------------------------------
// CLMM tick/price constants
// ---------------------------------------------------------------------------

/// Minimum tick index.
pub const MIN_TICK: i32 = -443_636;

/// Maximum tick index.
pub const MAX_TICK: i32 = 443_636;

/// Tick array size (number of ticks per array).
pub const TICK_ARRAY_SIZE: i32 = 60;

/// Tick array bitmap size.
pub const TICK_ARRAY_BITMAP_SIZE: i32 = 512;

/// Fee rate denominator (1,000,000).
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
        assert_eq!(SWAP_DISCRIMINATOR.len(), 8);
        assert_eq!(SWAP_V2_DISCRIMINATOR.len(), 8);
        assert_eq!(SWAP_ROUTER_BASE_IN_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn swap_discriminators_differ() {
        assert_ne!(SWAP_DISCRIMINATOR, SWAP_V2_DISCRIMINATOR);
        assert_ne!(SWAP_DISCRIMINATOR, SWAP_ROUTER_BASE_IN_DISCRIMINATOR);
        assert_ne!(SWAP_V2_DISCRIMINATOR, SWAP_ROUTER_BASE_IN_DISCRIMINATOR);
    }
}
