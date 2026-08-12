//! Raydium V4 protocol constants.
//!
//! Raydium V4 is a legacy AMM that uses 1-byte instruction indices
//! instead of 8-byte Anchor discriminators.

use solana_program::pubkey;
use solana_program::pubkey::Pubkey;

/// Raydium V4 AMM program ID.
pub const PROGRAM_ID: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");

/// Raydium authority (derived PDA for pool operations).
pub const AUTHORITY: Pubkey = pubkey!("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1");

/// Serum DEX program ID (used by Raydium V4 for order book).
pub const SERUM_PROGRAM_ID: Pubkey = pubkey!("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin");

// =============================================================================
// Instruction Indices (1-byte, NOT 8-byte Anchor discriminators)
// =============================================================================

/// Initialize instruction index.
pub const INITIALIZE_IX: u8 = 0;

/// Initialize2 instruction index.
pub const INITIALIZE2_IX: u8 = 1;

/// MonitorStep instruction index.
pub const MONITOR_STEP_IX: u8 = 2;

/// Deposit instruction index.
pub const DEPOSIT_IX: u8 = 3;

/// Withdraw instruction index.
pub const WITHDRAW_IX: u8 = 4;

/// MigrateToOpenBook instruction index.
pub const MIGRATE_TO_OPENBOOK_IX: u8 = 5;

/// SetParams instruction index.
pub const SET_PARAMS_IX: u8 = 6;

/// WithdrawPnl instruction index.
pub const WITHDRAW_PNL_IX: u8 = 7;

/// WithdrawSrm instruction index.
pub const WITHDRAW_SRM_IX: u8 = 8;

/// SwapBaseIn instruction index.
/// Swap by specifying exact input amount.
pub const SWAP_BASE_IN_IX: u8 = 9;

/// PreInitialize instruction index.
pub const PRE_INITIALIZE_IX: u8 = 10;

/// SwapBaseOut instruction index.
/// Swap by specifying exact output amount.
pub const SWAP_BASE_OUT_IX: u8 = 11;

/// SimulateInfo instruction index.
pub const SIMULATE_INFO_IX: u8 = 12;

/// AdminCancelOrders instruction index.
pub const ADMIN_CANCEL_ORDERS_IX: u8 = 13;

/// CreateConfigAccount instruction index.
pub const CREATE_CONFIG_ACCOUNT_IX: u8 = 14;

/// UpdateConfigAccount instruction index.
pub const UPDATE_CONFIG_ACCOUNT_IX: u8 = 15;

// =============================================================================
// Pool Constants
// =============================================================================

/// Pool state account size in bytes.
/// This is a large account (752 bytes).
pub const POOL_STATE_SIZE: usize = 752;

/// Minimum size for parsing essential swap fields.
/// We only need the first portion for swap calculations.
pub const POOL_STATE_MIN_SIZE: usize = 560;

// =============================================================================
// Fee Constants
// =============================================================================

/// Default trade fee numerator (25 = 0.25%).
pub const DEFAULT_TRADE_FEE_NUMERATOR: u64 = 25;

/// Default trade fee denominator.
pub const DEFAULT_TRADE_FEE_DENOMINATOR: u64 = 10000;
