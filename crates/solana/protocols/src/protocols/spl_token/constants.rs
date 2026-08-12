//! SPL Token program constants.

use solana_program::pubkey::Pubkey;

/// SPL Token program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// Token-2022 program ID.
pub const TOKEN_2022_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Transfer instruction discriminator (1 byte).
pub const TRANSFER_DISCRIMINATOR: [u8; 1] = [3];

/// TransferChecked instruction discriminator (1 byte).
pub const TRANSFER_CHECKED_DISCRIMINATOR: [u8; 1] = [12];
