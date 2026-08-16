//! Pump.fun instruction builders and parsing.
//!
//! This module provides:
//! - Builders for constructing pump.fun instructions
//! - Parsing types for reading transaction instructions
//! - The unified [`PumpfunInstruction`] enum for all instruction types
//!
//! # Structure
//!
//! Each instruction type has its own file:
//! - `buy.rs` - Buy instruction builder and params
//! - `sell.rs` - Sell instruction builder and params
//! - `create.rs` - Create instruction params (parsing only)
//! - `common.rs` - Shared helpers (ATA creation, etc.)
//!
//! # Building Instructions
//!
//! ```ignore
//! use solana_protocols::pumpfun::{BuyBuilder, BuyParams, PumpfunKeys};
//!
//! let keys = PumpfunKeys::new(mint, creator);
//! let params = BuyParams::new(tokens_out, max_sol_cost);
//!
//! let ix = BuyBuilder::build_swap_instruction(&keys, &user, params)?;
//! ```
//!
//! # Parsing Instructions
//!
//! ```ignore
//! use solana_protocols::pumpfun::PumpfunInstruction;
//!
//! let instruction = PumpfunInstruction::try_from_slice(&ix_data)?;
//! match instruction {
//!     PumpfunInstruction::Buy(params) => { /* handle buy */ }
//!     PumpfunInstruction::Sell(params) => { /* handle sell */ }
//!     PumpfunInstruction::Create(params) => { /* handle create */ }
//! }
//! ```

mod buy;
mod common;
mod create;
mod create_v2;
mod creator_fee;
mod sell;

pub use buy::{BuyAccounts, BuyBuilder, BuyExactInParams, BuyParams};
pub use common::{
    create_ata_idempotent_instruction, create_ata_idempotent_instruction_for,
    create_ata_instruction,
};
pub use create::{CreateAccounts, CreateParams};
pub use create_v2::{CreateV2Accounts, CreateV2Params};
pub use creator_fee::{CreatorFeeParams, DistributeCreatorFeesV2Params};
pub use sell::{SellAccounts, SellBuilder, SellParams};

use super::constants::{
    BUY_DISCRIMINATOR, BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR, BUY_EXACT_SOL_IN_DISCRIMINATOR,
    BUY_V2_DISCRIMINATOR, COLLECT_CREATOR_FEE_DISCRIMINATOR, COLLECT_CREATOR_FEE_V2_DISCRIMINATOR,
    CREATE_DISCRIMINATOR, CREATE_V2_DISCRIMINATOR, DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR,
    DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR, PROGRAM_ID, SELL_DISCRIMINATOR,
    SELL_V2_DISCRIMINATOR,
};
use crate::parsing::{FromAccountKeys, FromInstructionData, InstructionParseError};

// =============================================================================
// PumpfunInstruction - Unified Instruction Enum
// =============================================================================

/// Unified instruction enum for all pump.fun instructions.
///
/// This enum enables exhaustive pattern matching on instruction types.
/// Use `try_from_slice()` to parse instruction data.
#[derive(Debug, Clone)]
pub enum PumpfunInstruction {
    /// Buy tokens from the bonding curve.
    Buy(BuyParams),
    /// `buy_v2` — same pinned side as [`Buy`](Self::Buy), newer layout.
    BuyV2(BuyParams),
    /// `buy_exact_sol_in` — pins the SOL spent, not the tokens received.
    BuyExactSolIn(BuyExactInParams),
    /// `buy_exact_quote_in_v2` — exact-in against the v2 layout.
    BuyExactQuoteInV2(BuyExactInParams),
    /// Sell tokens back to the bonding curve.
    Sell(SellParams),
    /// `sell_v2` — same pinned side as [`Sell`](Self::Sell), newer layout.
    SellV2(SellParams),
    /// Legacy `create` instruction (pre-2024). Now rare in production.
    Create(CreateParams),
    /// Modern `create_v2` instruction. Different account layout
    /// (16 slots vs 14, `user` at slot 5 not 7) and richer params
    /// (explicit `creator: Pubkey` arg + mayhem/cashback flags).
    CreateV2(CreateV2Params),
    /// `collect_creator_fee` — a creator draining their fee vault.
    CollectCreatorFee(CreatorFeeParams),
    /// `collect_creator_fee_v2` — the same, settling into a token account.
    CollectCreatorFeeV2(CreatorFeeParams),
    /// `distribute_creator_fees` — a vault split across a sharing config.
    DistributeCreatorFees(CreatorFeeParams),
    /// `distribute_creator_fees_v2` — the same, able to create the recipient
    /// token account on the way.
    DistributeCreatorFeesV2(DistributeCreatorFeesV2Params),
}

impl PumpfunInstruction {
    /// Program ID for pumpfun.
    #[must_use]
    pub fn program_id() -> solana_program::pubkey::Pubkey {
        PROGRAM_ID
    }

    /// Parse instruction data into the appropriate variant.
    ///
    /// Matches the first 8 bytes against known discriminators.
    pub fn try_from_slice(data: &[u8]) -> Result<Self, InstructionParseError> {
        if data.len() < 8 {
            return Err(InstructionParseError::DataTooShort);
        }

        let discriminator: [u8; 8] = data[..8].try_into().unwrap();
        let params_data = &data[8..];

        // Exhaustive matching - no wildcards
        if discriminator == BUY_DISCRIMINATOR {
            let params = BuyParams::from_instruction_data(params_data)?;
            Ok(PumpfunInstruction::Buy(params))
        } else if discriminator == SELL_DISCRIMINATOR {
            let params = SellParams::from_instruction_data(params_data)?;
            Ok(PumpfunInstruction::Sell(params))
        } else if discriminator == BUY_V2_DISCRIMINATOR {
            Ok(PumpfunInstruction::BuyV2(BuyParams::from_instruction_data(
                params_data,
            )?))
        } else if discriminator == BUY_EXACT_SOL_IN_DISCRIMINATOR {
            Ok(PumpfunInstruction::BuyExactSolIn(
                BuyExactInParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR {
            Ok(PumpfunInstruction::BuyExactQuoteInV2(
                BuyExactInParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == SELL_V2_DISCRIMINATOR {
            Ok(PumpfunInstruction::SellV2(
                SellParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == CREATE_DISCRIMINATOR {
            let params = CreateParams::from_instruction_data(params_data)?;
            Ok(PumpfunInstruction::Create(params))
        } else if discriminator == COLLECT_CREATOR_FEE_DISCRIMINATOR {
            Ok(PumpfunInstruction::CollectCreatorFee(
                CreatorFeeParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == COLLECT_CREATOR_FEE_V2_DISCRIMINATOR {
            Ok(PumpfunInstruction::CollectCreatorFeeV2(
                CreatorFeeParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR {
            Ok(PumpfunInstruction::DistributeCreatorFees(
                CreatorFeeParams::from_instruction_data(params_data)?,
            ))
        } else if discriminator == DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR {
            Ok(PumpfunInstruction::DistributeCreatorFeesV2(
                DistributeCreatorFeesV2Params::from_instruction_data(params_data)?,
            ))
        } else if discriminator == CREATE_V2_DISCRIMINATOR {
            let params = CreateV2Params::from_instruction_data(params_data)?;
            Ok(PumpfunInstruction::CreateV2(params))
        } else {
            Err(InstructionParseError::UnknownDiscriminator(discriminator))
        }
    }

    /// Get the discriminator for this instruction variant.
    #[must_use]
    pub fn discriminator(&self) -> [u8; 8] {
        match self {
            PumpfunInstruction::Buy(_) => BUY_DISCRIMINATOR,
            PumpfunInstruction::Sell(_) => SELL_DISCRIMINATOR,
            PumpfunInstruction::BuyV2(_) => BUY_V2_DISCRIMINATOR,
            PumpfunInstruction::BuyExactSolIn(_) => BUY_EXACT_SOL_IN_DISCRIMINATOR,
            PumpfunInstruction::BuyExactQuoteInV2(_) => BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR,
            PumpfunInstruction::SellV2(_) => SELL_V2_DISCRIMINATOR,
            PumpfunInstruction::Create(_) => CREATE_DISCRIMINATOR,
            PumpfunInstruction::CreateV2(_) => CREATE_V2_DISCRIMINATOR,
            PumpfunInstruction::CollectCreatorFee(_) => COLLECT_CREATOR_FEE_DISCRIMINATOR,
            PumpfunInstruction::CollectCreatorFeeV2(_) => COLLECT_CREATOR_FEE_V2_DISCRIMINATOR,
            PumpfunInstruction::DistributeCreatorFees(_) => DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR,
            PumpfunInstruction::DistributeCreatorFeesV2(_) => {
                DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR
            }
        }
    }

    /// Serialize the instruction back to bytes.
    ///
    /// `CreateV2` is decode-only — there's no encoder yet because we
    /// don't build v2 launches in this codebase. Calling `data()` on
    /// a `CreateV2` variant returns just the discriminator + a
    /// truncated body covering the leading `(name, symbol, uri)`
    /// fields, which matches the v1 wire shape but not v2's full
    /// payload. Don't rely on it for round-tripping.
    #[must_use]
    pub fn data(&self) -> Vec<u8> {
        match self {
            PumpfunInstruction::Buy(params) => params.to_data(),
            PumpfunInstruction::Sell(params) => params.to_data(),
            // No encoders for the newer forms; callers should not be
            // re-emitting them through this path. Returning the bare
            // discriminator keeps the shape honest rather than encoding a
            // v1 body under a v2 discriminator.
            PumpfunInstruction::BuyV2(_) => BUY_V2_DISCRIMINATOR.to_vec(),
            PumpfunInstruction::BuyExactSolIn(_) => BUY_EXACT_SOL_IN_DISCRIMINATOR.to_vec(),
            PumpfunInstruction::BuyExactQuoteInV2(_) => {
                BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR.to_vec()
            }
            PumpfunInstruction::SellV2(_) => SELL_V2_DISCRIMINATOR.to_vec(),
            PumpfunInstruction::Create(params) => params.to_data(),
            PumpfunInstruction::CreateV2(_) => {
                // No encoder. Return just the disc; callers shouldn't
                // be re-emitting v2 instructions via this path.
                CREATE_V2_DISCRIMINATOR.to_vec()
            }
            // Zero-argument instructions: the discriminator *is* the whole
            // encoding, so these are complete rather than truncated.
            PumpfunInstruction::CollectCreatorFee(_) => COLLECT_CREATOR_FEE_DISCRIMINATOR.to_vec(),
            PumpfunInstruction::CollectCreatorFeeV2(_) => {
                COLLECT_CREATOR_FEE_V2_DISCRIMINATOR.to_vec()
            }
            PumpfunInstruction::DistributeCreatorFees(_) => {
                DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR.to_vec()
            }
            PumpfunInstruction::DistributeCreatorFeesV2(params) => {
                let mut data = DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR.to_vec();
                data.push(u8::from(params.initialize_ata));
                data
            }
        }
    }

    /// Parse account pubkeys into the corresponding accounts struct.
    pub fn from_accounts(
        &self,
        account_keys: &[solana_program::pubkey::Pubkey],
    ) -> Result<PumpfunInstructionAccounts, InstructionParseError> {
        match self {
            // `buy_exact_sol_in` shares `buy`'s account layout.
            PumpfunInstruction::Buy(_) | PumpfunInstruction::BuyExactSolIn(_) => {
                let accounts = BuyAccounts::from_account_keys(account_keys)?;
                Ok(PumpfunInstructionAccounts::Buy(accounts))
            }
            // v2 layouts are not captured; decoding them through a v1 struct
            // would succeed and be wrong.
            PumpfunInstruction::BuyV2(_)
            | PumpfunInstruction::BuyExactQuoteInV2(_)
            | PumpfunInstruction::SellV2(_) => Err(InstructionParseError::DeserializationFailed(
                "pumpfun v2 account layout not captured".to_string(),
            )),
            // Deliberately not captured: mainnet sends these with more accounts
            // than the IDL declares, so a fixed-slot struct would decode the
            // wrong pubkeys. Their event names its own participants.
            PumpfunInstruction::CollectCreatorFee(_)
            | PumpfunInstruction::CollectCreatorFeeV2(_)
            | PumpfunInstruction::DistributeCreatorFees(_)
            | PumpfunInstruction::DistributeCreatorFeesV2(_) => {
                Err(InstructionParseError::DeserializationFailed(
                    "pumpfun creator-fee instructions are identified, not slot-decoded".to_string(),
                ))
            }
            PumpfunInstruction::Sell(_) => {
                let accounts = SellAccounts::from_account_keys(account_keys)?;
                Ok(PumpfunInstructionAccounts::Sell(accounts))
            }
            PumpfunInstruction::Create(_) => {
                let accounts = CreateAccounts::from_account_keys(account_keys)?;
                Ok(PumpfunInstructionAccounts::Create(accounts))
            }
            PumpfunInstruction::CreateV2(_) => {
                let accounts = CreateV2Accounts::from_account_keys(account_keys)?;
                Ok(PumpfunInstructionAccounts::CreateV2(accounts))
            }
        }
    }

    /// Check if this is a buy instruction.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        matches!(self, PumpfunInstruction::Buy(_))
    }

    /// Check if this is a sell instruction.
    #[must_use]
    pub fn is_sell(&self) -> bool {
        matches!(self, PumpfunInstruction::Sell(_))
    }

    /// Check if this is a create instruction (v1 or v2).
    #[must_use]
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            PumpfunInstruction::Create(_) | PumpfunInstruction::CreateV2(_)
        )
    }

    /// Check if this is a swap instruction (buy or sell).
    #[must_use]
    pub fn is_swap(&self) -> bool {
        matches!(
            self,
            PumpfunInstruction::Buy(_) | PumpfunInstruction::Sell(_)
        )
    }
}

// =============================================================================
// PumpfunInstructionAccounts - Accounts Enum
// =============================================================================

/// Accounts enum for pumpfun instruction variants.
///
/// Each variant contains the resolved account pubkeys for that instruction.
#[derive(Debug, Clone)]
pub enum PumpfunInstructionAccounts {
    /// Accounts for buy instruction.
    Buy(BuyAccounts),
    /// Accounts for sell instruction.
    Sell(SellAccounts),
    /// Accounts for legacy `create` instruction.
    Create(CreateAccounts),
    /// Accounts for modern `create_v2` instruction (different layout).
    CreateV2(CreateV2Accounts),
}

impl PumpfunInstructionAccounts {
    /// Get the mint address from any instruction accounts.
    #[must_use]
    pub fn mint(&self) -> solana_program::pubkey::Pubkey {
        match self {
            PumpfunInstructionAccounts::Buy(a) => a.mint,
            PumpfunInstructionAccounts::Sell(a) => a.mint,
            PumpfunInstructionAccounts::Create(a) => a.mint,
            PumpfunInstructionAccounts::CreateV2(a) => a.mint,
        }
    }

    /// Get the user address from any instruction accounts.
    #[must_use]
    pub fn user(&self) -> solana_program::pubkey::Pubkey {
        match self {
            PumpfunInstructionAccounts::Buy(a) => a.user,
            PumpfunInstructionAccounts::Sell(a) => a.user,
            PumpfunInstructionAccounts::Create(a) => a.user,
            PumpfunInstructionAccounts::CreateV2(a) => a.user,
        }
    }

    /// Get the bonding curve address from any instruction accounts.
    #[must_use]
    pub fn bonding_curve(&self) -> solana_program::pubkey::Pubkey {
        match self {
            PumpfunInstructionAccounts::Buy(a) => a.bonding_curve,
            PumpfunInstructionAccounts::Sell(a) => a.bonding_curve,
            PumpfunInstructionAccounts::Create(a) => a.bonding_curve,
            PumpfunInstructionAccounts::CreateV2(a) => a.bonding_curve,
        }
    }
}

// =============================================================================
// PumpfunInstructionEvent - Parsed Event
// =============================================================================

/// Parsed pumpfun event combining instruction, accounts, and log.
///
/// This struct represents a fully parsed instruction from a transaction,
/// including the resolved account pubkeys and any associated log data.
#[derive(Debug, Clone)]
pub struct PumpfunInstructionEvent {
    /// The parsed instruction with its parameters.
    pub instruction: PumpfunInstruction,
    /// The resolved account pubkeys.
    pub accounts: PumpfunInstructionAccounts,
    /// Associated log data (discriminator + payload).
    /// None if no matching log was found.
    pub log_data: Option<Vec<u8>>,
    /// Whether transaction logs were truncated.
    pub logs_truncated: bool,
}

impl PumpfunInstructionEvent {
    /// Create a new event from parsed components.
    #[must_use]
    pub fn new(
        instruction: PumpfunInstruction,
        accounts: PumpfunInstructionAccounts,
        log_data: Option<Vec<u8>>,
        logs_truncated: bool,
    ) -> Self {
        Self {
            instruction,
            accounts,
            log_data,
            logs_truncated,
        }
    }

    /// Get the instruction discriminator.
    #[must_use]
    pub fn discriminator(&self) -> [u8; 8] {
        self.instruction.discriminator()
    }

    /// Get the mint address.
    #[must_use]
    pub fn mint(&self) -> solana_program::pubkey::Pubkey {
        self.accounts.mint()
    }

    /// Get the user address.
    #[must_use]
    pub fn user(&self) -> solana_program::pubkey::Pubkey {
        self.accounts.user()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn pumpfun_instruction_buy_roundtrip() {
        let params = BuyParams::new(1_000_000_000, 100_000_000);
        let data = params.to_data();

        let parsed = PumpfunInstruction::try_from_slice(&data).unwrap();
        assert!(parsed.is_buy());

        if let PumpfunInstruction::Buy(p) = parsed {
            assert_eq!(p.amount, 1_000_000_000);
            assert_eq!(p.max_sol_cost, 100_000_000);
        } else {
            panic!("Expected Buy variant");
        }
    }

    #[test]
    fn pumpfun_instruction_sell_roundtrip() {
        let params = SellParams::new(1_000_000_000, 50_000_000);
        let data = params.to_data();

        let parsed = PumpfunInstruction::try_from_slice(&data).unwrap();
        assert!(parsed.is_sell());

        if let PumpfunInstruction::Sell(p) = parsed {
            assert_eq!(p.amount, 1_000_000_000);
            assert_eq!(p.min_sol_output, 50_000_000);
        } else {
            panic!("Expected Sell variant");
        }
    }

    #[test]
    fn pumpfun_instruction_create_roundtrip() {
        let params = CreateParams::new(
            "Test Token".to_string(),
            "TEST".to_string(),
            "https://example.com".to_string(),
        );
        let data = params.to_data();

        let parsed = PumpfunInstruction::try_from_slice(&data).unwrap();
        assert!(parsed.is_create());

        if let PumpfunInstruction::Create(p) = parsed {
            assert_eq!(p.name, "Test Token");
            assert_eq!(p.symbol, "TEST");
            assert_eq!(p.uri, "https://example.com");
        } else {
            panic!("Expected Create variant");
        }
    }

    #[test]
    fn pumpfun_instruction_unknown_discriminator() {
        let data = [0xFF; 24];
        let result = PumpfunInstruction::try_from_slice(&data);
        assert!(matches!(
            result,
            Err(InstructionParseError::UnknownDiscriminator(_))
        ));
    }

    #[test]
    fn pumpfun_instruction_data_too_short() {
        let data = [0u8; 4];
        let result = PumpfunInstruction::try_from_slice(&data);
        assert!(matches!(result, Err(InstructionParseError::DataTooShort)));
    }

    #[test]
    fn pumpfun_instruction_is_swap() {
        let buy = PumpfunInstruction::Buy(BuyParams::new(100, 10));
        let sell = PumpfunInstruction::Sell(SellParams::new(100, 10));
        let create = PumpfunInstruction::Create(CreateParams::new(
            "A".to_string(),
            "A".to_string(),
            "A".to_string(),
        ));

        assert!(buy.is_swap());
        assert!(sell.is_swap());
        assert!(!create.is_swap());
    }

    #[test]
    fn pumpfun_accounts_from_keys() {
        let keys: Vec<Pubkey> = (0..16).map(|_| Pubkey::new_unique()).collect();

        let instruction = PumpfunInstruction::Buy(BuyParams::new(100, 10));
        let accounts = instruction.from_accounts(&keys).unwrap();

        if let PumpfunInstructionAccounts::Buy(a) = accounts {
            assert_eq!(a.mint, keys[2]);
            assert_eq!(a.user, keys[6]);
            assert_eq!(a.bonding_curve, keys[3]);
        } else {
            panic!("Expected Buy accounts");
        }
    }
}
