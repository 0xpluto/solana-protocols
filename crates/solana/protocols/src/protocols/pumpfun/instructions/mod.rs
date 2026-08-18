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
mod buy_exact_quote_in_v2;
mod buy_exact_sol_in;
mod buy_v2;
mod collect_creator_fee;
mod collect_creator_fee_v2;
mod common;
mod create;
mod create_v2;
mod distribute_creator_fees;
mod distribute_creator_fees_v2;
mod sell;
mod sell_v2;

pub use buy::{BuyAccounts, BuyBuilder, BuyParams};
pub use buy_exact_quote_in_v2::BuyExactQuoteInV2Params;
pub use buy_exact_sol_in::BuyExactSolInParams;
pub use buy_v2::BuyV2Params;
pub use collect_creator_fee::CollectCreatorFeeParams;
pub use collect_creator_fee_v2::CollectCreatorFeeV2Params;
pub use common::{
    create_ata_idempotent_instruction, create_ata_idempotent_instruction_for,
    create_ata_instruction,
};
pub use create::{CreateAccounts, CreateParams};
pub use create_v2::{CreateV2Accounts, CreateV2Params};
pub use distribute_creator_fees::DistributeCreatorFeesParams;
pub use distribute_creator_fees_v2::DistributeCreatorFeesV2Params;
pub use sell::{SellAccounts, SellBuilder, SellParams};
pub use sell_v2::SellV2Params;

use super::constants::{
    BUY_DISCRIMINATOR, BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR, BUY_EXACT_SOL_IN_DISCRIMINATOR,
    BUY_V2_DISCRIMINATOR, COLLECT_CREATOR_FEE_DISCRIMINATOR, COLLECT_CREATOR_FEE_V2_DISCRIMINATOR,
    CREATE_DISCRIMINATOR, CREATE_V2_DISCRIMINATOR, DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR,
    DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR, PROGRAM_ID, SELL_DISCRIMINATOR,
    SELL_V2_DISCRIMINATOR,
};
use solana_program::pubkey::Pubkey;

use solana_protocols_macros::ProtocolInstruction;

// =============================================================================
// PumpfunInstruction - Unified Instruction Enum
// =============================================================================

/// Every pump.fun instruction this crate decodes, one variant per on-chain
/// discriminator.
///
/// `#[derive(ProtocolInstruction)]` generates `try_from_slice`,
/// `discriminator` and `data` from the table below. The hand-written versions
/// were a 60-line if-else chain over the same constants — three places to add a
/// new instruction, and nothing that failed if you updated two of them.
///
/// Variants carrying no `accounts =` do so deliberately: their on-chain account
/// lists are variable (the v2 swap forms) or run longer than the IDL declares
/// (the creator-fee forms), so identity comes from the event rather than from a
/// slot index.
#[derive(Debug, Clone, ProtocolInstruction)]
#[protocol(program_id = PROGRAM_ID)]
pub enum PumpfunInstruction {
    /// Buy tokens from the bonding curve, pinning the tokens delivered.
    #[instruction(discriminator = BUY_DISCRIMINATOR, accounts = BuyAccounts)]
    Buy(BuyParams),
    /// Exact-out buy against the v2 layout.
    #[instruction(discriminator = BUY_V2_DISCRIMINATOR)]
    BuyV2(BuyV2Params),
    /// Exact-in buy: pins the SOL spent, not the tokens received.
    #[instruction(discriminator = BUY_EXACT_SOL_IN_DISCRIMINATOR, accounts = BuyAccounts)]
    BuyExactSolIn(BuyExactSolInParams),
    /// Exact-in buy against the v2 layout, denominated in the pool's quote.
    #[instruction(discriminator = BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR)]
    BuyExactQuoteInV2(BuyExactQuoteInV2Params),
    /// Sell tokens back to the bonding curve.
    #[instruction(discriminator = SELL_DISCRIMINATOR, accounts = SellAccounts)]
    Sell(SellParams),
    /// Sell against the v2 layout.
    #[instruction(discriminator = SELL_V2_DISCRIMINATOR)]
    SellV2(SellV2Params),
    /// Legacy `create` (pre-2024), now rare in production.
    #[instruction(discriminator = CREATE_DISCRIMINATOR, accounts = CreateAccounts)]
    Create(CreateParams),
    /// Modern `create_v2`: 16 slots rather than 14, `user` at slot 5 not 7, and
    /// an explicit `creator` argument.
    #[instruction(discriminator = CREATE_V2_DISCRIMINATOR, accounts = CreateV2Accounts)]
    CreateV2(CreateV2Params),
    /// A creator draining their fee vault.
    #[instruction(discriminator = COLLECT_CREATOR_FEE_DISCRIMINATOR)]
    CollectCreatorFee(CollectCreatorFeeParams),
    /// The same, settling into a token account.
    #[instruction(discriminator = COLLECT_CREATOR_FEE_V2_DISCRIMINATOR)]
    CollectCreatorFeeV2(CollectCreatorFeeV2Params),
    /// A vault split across a sharing config.
    #[instruction(discriminator = DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR)]
    DistributeCreatorFees(DistributeCreatorFeesParams),
    /// The same, able to create the recipient token account on the way.
    #[instruction(discriminator = DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR)]
    DistributeCreatorFeesV2(DistributeCreatorFeesV2Params),
}

impl PumpfunInstruction {
    /// Any of the four buy forms.
    ///
    /// All four, deliberately: this matched only the v1 `buy` while `buy_v2`,
    /// `buy_exact_sol_in` and `buy_exact_quote_in_v2` existed as variants, so a
    /// caller asking "is this a buy" got `false` for roughly a third of the
    /// buys on chain. The per-discriminator split is what made the gap visible.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        matches!(
            self,
            PumpfunInstruction::Buy(_)
                | PumpfunInstruction::BuyV2(_)
                | PumpfunInstruction::BuyExactSolIn(_)
                | PumpfunInstruction::BuyExactQuoteInV2(_)
        )
    }

    /// Either sell form.
    #[must_use]
    pub fn is_sell(&self) -> bool {
        matches!(
            self,
            PumpfunInstruction::Sell(_) | PumpfunInstruction::SellV2(_)
        )
    }

    /// Check if this is a create instruction (v1 or v2).
    #[must_use]
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            PumpfunInstruction::Create(_) | PumpfunInstruction::CreateV2(_)
        )
    }

    /// Any buy or sell, in any of its forms.
    #[must_use]
    pub fn is_swap(&self) -> bool {
        self.is_buy() || self.is_sell()
    }
}

// =============================================================================
// PumpfunInstructionAccounts - Accounts Enum
// =============================================================================

impl PumpfunInstructionAccounts {
    /// The token mint this instruction acts on.
    ///
    /// `None` for the v2 and creator-fee forms, which carry no account struct:
    /// their on-chain lists are variable or longer than the IDL declares, so
    /// there is no slot to read. Identity for those comes from the event, and
    /// returning a defaulted pubkey here would be an answer we do not have.
    #[must_use]
    pub fn mint(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.mint),
            Self::Sell(a) => Some(a.mint),
            Self::Create(a) => Some(a.mint),
            Self::CreateV2(a) => Some(a.mint),
            Self::BuyV2
            | Self::BuyExactQuoteInV2
            | Self::SellV2
            | Self::CollectCreatorFee
            | Self::CollectCreatorFeeV2
            | Self::DistributeCreatorFees
            | Self::DistributeCreatorFeesV2 => None,
        }
    }

    /// The bonding curve backing [`mint`](Self::mint).
    ///
    /// `None` for the v2 and creator-fee forms, which carry no account struct:
    /// their on-chain lists are variable or longer than the IDL declares, so
    /// there is no slot to read. Identity for those comes from the event, and
    /// returning a defaulted pubkey here would be an answer we do not have.
    #[must_use]
    pub fn bonding_curve(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.bonding_curve),
            Self::Sell(a) => Some(a.bonding_curve),
            Self::Create(a) => Some(a.bonding_curve),
            Self::CreateV2(a) => Some(a.bonding_curve),
            Self::BuyV2
            | Self::BuyExactQuoteInV2
            | Self::SellV2
            | Self::CollectCreatorFee
            | Self::CollectCreatorFeeV2
            | Self::DistributeCreatorFees
            | Self::DistributeCreatorFeesV2 => None,
        }
    }

    /// The wallet that signed — trader on a swap, creator on a launch.
    ///
    /// `None` for the v2 and creator-fee forms, which carry no account struct:
    /// their on-chain lists are variable or longer than the IDL declares, so
    /// there is no slot to read. Identity for those comes from the event, and
    /// returning a defaulted pubkey here would be an answer we do not have.
    #[must_use]
    pub fn user(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.user),
            Self::Sell(a) => Some(a.user),
            Self::Create(a) => Some(a.user),
            Self::CreateV2(a) => Some(a.user),
            Self::BuyV2
            | Self::BuyExactQuoteInV2
            | Self::SellV2
            | Self::CollectCreatorFee
            | Self::CollectCreatorFeeV2
            | Self::DistributeCreatorFees
            | Self::DistributeCreatorFeesV2 => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::InstructionParseError;
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
