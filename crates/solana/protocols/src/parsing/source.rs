//! Transaction source abstraction.
//!
//! The [`TransactionSource`] trait abstracts over different transaction data sources
//! (RPC, GRPC, cached data), allowing the same parsing code to work with all of them.
//!
//! Unlike the legacy `DerivableTransaction`, this uses Result-based error handling
//! instead of panics.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::collections::HashMap;
use thiserror::Error;

use super::instruction::ParsedInstruction;

/// Error from transaction source operations.
#[derive(Debug, Clone, Error)]
pub enum SourceError {
    /// Transaction metadata is missing.
    #[error("transaction metadata missing")]
    MissingMeta,

    /// Failed to parse signature.
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// Failed to parse pubkey.
    #[error("invalid pubkey at {context}: {value}")]
    InvalidPubkey { context: String, value: String },

    /// Failed to parse amount.
    #[error("invalid amount at {context}: {value}")]
    InvalidAmount { context: String, value: String },

    /// Transaction format not supported.
    #[error("unsupported transaction format: {0}")]
    UnsupportedFormat(String),

    /// Failed to decode instruction data.
    #[error("failed to decode instruction data: {0}")]
    DecodeError(String),

    /// Transaction failed on-chain.
    #[error("transaction failed: {0}")]
    TransactionFailed(String),
}

/// Trait for abstracting transaction data sources.
///
/// Implement this for RPC responses, GRPC updates, cached transactions, etc.
/// All methods return Result to enable proper error handling.
///
/// # Example Implementation
///
/// ```ignore
/// impl TransactionSource for MyTransactionType {
///     fn slot(&self) -> u64 { self.slot }
///     fn signature(&self) -> Result<Signature, SourceError> { ... }
///     fn accounts(&self) -> Result<Vec<Pubkey>, SourceError> { ... }
///     // ...
/// }
/// ```
pub trait TransactionSource {
    /// Get the slot this transaction was processed in.
    fn slot(&self) -> u64;

    /// Get the block time (Unix timestamp).
    ///
    /// Returns None if not available (e.g., some GRPC sources).
    fn block_time(&self) -> Option<i64> {
        None
    }

    /// Get the first signature (transaction ID).
    fn signature(&self) -> Result<Signature, SourceError>;

    /// Get all transaction signatures.
    fn signatures(&self) -> Result<Vec<Signature>, SourceError>;

    /// Get all account pubkeys (including LUT-resolved addresses).
    fn accounts(&self) -> Result<Vec<Pubkey>, SourceError>;

    /// Get static account pubkeys (excluding LUT addresses).
    fn static_accounts(&self) -> Result<Vec<Pubkey>, SourceError>;

    /// Get transaction metadata.
    fn meta(&self) -> Result<TransactionMeta, SourceError>;

    /// Get raw log messages.
    fn log_messages(&self) -> Result<Vec<String>, SourceError>;

    /// Check if transaction failed.
    fn is_failed(&self) -> bool {
        self.error().is_some()
    }

    /// Get transaction error if failed.
    fn error(&self) -> Option<String>;

    /// Get pre-transaction token balances.
    fn pre_token_balances(&self) -> Result<Vec<TokenBalanceChange>, SourceError>;

    /// Get post-transaction token balances.
    fn post_token_balances(&self) -> Result<Vec<TokenBalanceChange>, SourceError>;

    /// Get pre-transaction SOL balances (lamports).
    fn pre_balances(&self) -> Result<Vec<u64>, SourceError>;

    /// Get post-transaction SOL balances (lamports).
    fn post_balances(&self) -> Result<Vec<u64>, SourceError>;

    /// Parse instructions with resolved accounts and associated logs.
    ///
    /// This is the main method for getting structured instructions.
    fn parse_instructions(&self) -> Result<Vec<ParsedInstruction>, SourceError>;

    // =========================================================================
    // Convenience methods with default implementations
    // =========================================================================

    /// Get the fee payer (first account).
    fn fee_payer(&self) -> Result<Pubkey, SourceError> {
        self.accounts()?
            .into_iter()
            .next()
            .ok_or(SourceError::MissingMeta)
    }

    /// Get SOL balance change for an account.
    fn sol_balance_change(&self, account: &Pubkey) -> Result<i64, SourceError> {
        let accounts = self.accounts()?;
        let pre = self.pre_balances()?;
        let post = self.post_balances()?;

        for (i, acc) in accounts.iter().enumerate() {
            if acc == account {
                let pre_balance = pre.get(i).copied().unwrap_or(0) as i64;
                let post_balance = post.get(i).copied().unwrap_or(0) as i64;
                return Ok(post_balance - pre_balance);
            }
        }

        Ok(0)
    }

    /// Get token balance changes grouped by owner.
    fn token_balance_changes(
        &self,
    ) -> Result<HashMap<Pubkey, Vec<TokenBalanceChange>>, SourceError> {
        let pre = self.pre_token_balances()?;
        let post = self.post_token_balances()?;

        let mut changes: HashMap<Pubkey, Vec<TokenBalanceChange>> = HashMap::new();

        // Index pre-balances by (owner, mint)
        let mut pre_map: HashMap<(Pubkey, Pubkey), u64> = HashMap::new();
        for balance in &pre {
            pre_map.insert((balance.owner, balance.mint), balance.amount);
        }

        // Calculate changes from post-balances
        for balance in post {
            let pre_amount = pre_map
                .get(&(balance.owner, balance.mint))
                .copied()
                .unwrap_or(0);
            let change = TokenBalanceChange {
                owner: balance.owner,
                mint: balance.mint,
                amount: balance.amount,
                decimals: balance.decimals,
                pre_amount: Some(pre_amount),
            };
            changes.entry(balance.owner).or_default().push(change);
        }

        Ok(changes)
    }
}

/// Transaction metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionMeta {
    /// Transaction fee in lamports.
    pub fee: u64,

    /// Pre-transaction balances (lamports).
    pub pre_balances: Vec<u64>,

    /// Post-transaction balances (lamports).
    pub post_balances: Vec<u64>,

    /// Log messages.
    pub log_messages: Vec<String>,

    /// Error message if transaction failed.
    pub error: Option<String>,

    /// Compute units consumed.
    pub compute_units_consumed: Option<u64>,
}

/// Token balance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceChange {
    /// Token account owner.
    pub owner: Pubkey,

    /// Token mint.
    pub mint: Pubkey,

    /// Current/post balance (smallest units).
    pub amount: u64,

    /// Token decimals.
    pub decimals: u8,

    /// Pre-transaction balance (if known).
    pub pre_amount: Option<u64>,
}

impl TokenBalanceChange {
    /// Calculate the balance change.
    ///
    /// Returns None if pre_amount is not known.
    #[must_use]
    pub fn change(&self) -> Option<i128> {
        self.pre_amount.map(|pre| self.amount as i128 - pre as i128)
    }

    /// Check if this is a deposit (balance increased).
    #[must_use]
    pub fn is_deposit(&self) -> Option<bool> {
        self.change().map(|c| c > 0)
    }

    /// Check if this is a withdrawal (balance decreased).
    #[must_use]
    pub fn is_withdrawal(&self) -> Option<bool> {
        self.change().map(|c| c < 0)
    }

    /// Get the absolute change amount.
    #[must_use]
    pub fn abs_change(&self) -> Option<u64> {
        self.change().map(|c| c.unsigned_abs() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_balance_change() {
        let change = TokenBalanceChange {
            owner: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            amount: 1000,
            decimals: 6,
            pre_amount: Some(500),
        };

        assert_eq!(change.change(), Some(500));
        assert_eq!(change.is_deposit(), Some(true));
        assert_eq!(change.is_withdrawal(), Some(false));
        assert_eq!(change.abs_change(), Some(500));
    }

    #[test]
    fn token_balance_change_withdrawal() {
        let change = TokenBalanceChange {
            owner: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            amount: 200,
            decimals: 6,
            pre_amount: Some(500),
        };

        assert_eq!(change.change(), Some(-300));
        assert_eq!(change.is_deposit(), Some(false));
        assert_eq!(change.is_withdrawal(), Some(true));
        assert_eq!(change.abs_change(), Some(300));
    }

    #[test]
    fn token_balance_change_unknown_pre() {
        let change = TokenBalanceChange {
            owner: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            amount: 1000,
            decimals: 6,
            pre_amount: None,
        };

        assert_eq!(change.change(), None);
        assert_eq!(change.is_deposit(), None);
    }
}
