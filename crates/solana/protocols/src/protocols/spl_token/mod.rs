//! SPL Token program types.
//!
//! Typed instruction parsing for SPL Token Transfer and TransferChecked
//! using the same derive macro infrastructure as protocol-specific instructions.
//!
//! # Usage
//!
//! ```ignore
//! use solana_protocols::protocols::spl_token::*;
//!
//! // Parse a CPI child instruction
//! let ix = SplTokenInstruction::try_from_slice(&child.data)?;
//! let accounts = ix.from_accounts(&child.accounts)?;
//!
//! match (ix, accounts) {
//!     (SplTokenInstruction::Transfer(params),
//!      SplTokenInstructionAccounts::Transfer(accts)) => {
//!         println!("Transfer {} from {} to {}", params.amount, accts.source, accts.destination);
//!     }
//!     (SplTokenInstruction::TransferChecked(params),
//!      SplTokenInstructionAccounts::TransferChecked(accts)) => {
//!         println!("TransferChecked {} of mint {}", params.amount, accts.mint);
//!     }
//! }
//! ```

pub mod constants;
pub mod instructions;

pub use constants::{PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
pub use instructions::{
    SplTokenInstruction, SplTokenInstructionAccounts, TransferAccounts, TransferCheckedAccounts,
    TransferCheckedParams, TransferParams,
};
