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
mod migrate;
mod migrate_v2;
mod sell;
mod sell_v2;

pub use buy::{BuyAccounts, BuyBuilder, BuyParams};
pub use buy_exact_quote_in_v2::BuyExactQuoteInV2Params;
pub use buy_exact_sol_in::BuyExactSolInParams;
pub use buy_v2::{BuyV2Accounts, BuyV2Params};
pub use collect_creator_fee::{CollectCreatorFeeAccounts, CollectCreatorFeeParams};
pub use collect_creator_fee_v2::{CollectCreatorFeeV2Accounts, CollectCreatorFeeV2Params};
pub use common::{
    create_ata_idempotent_instruction, create_ata_idempotent_instruction_for,
    create_ata_instruction,
};
pub use create::{CreateAccounts, CreateParams};
pub use create_v2::{CreateV2Accounts, CreateV2Params};
pub use distribute_creator_fees::{DistributeCreatorFeesAccounts, DistributeCreatorFeesParams};
pub use distribute_creator_fees_v2::{
    DistributeCreatorFeesV2Accounts, DistributeCreatorFeesV2Params,
};
pub use migrate::{MigrateAccounts, MigrateParams};
pub use migrate_v2::{MigrateV2Accounts, MigrateV2Params};
pub use sell::{SellAccounts, SellBuilder, SellParams};
pub use sell_v2::{SellV2Accounts, SellV2Params};

use super::constants::{
    BUY_DISCRIMINATOR, BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR, BUY_EXACT_SOL_IN_DISCRIMINATOR,
    BUY_V2_DISCRIMINATOR, COLLECT_CREATOR_FEE_DISCRIMINATOR, COLLECT_CREATOR_FEE_V2_DISCRIMINATOR,
    CREATE_DISCRIMINATOR, CREATE_V2_DISCRIMINATOR, DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR,
    DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR, MIGRATE_DISCRIMINATOR, MIGRATE_V2_DISCRIMINATOR,
    PROGRAM_ID, SELL_DISCRIMINATOR, SELL_V2_DISCRIMINATOR,
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
    #[instruction(discriminator = BUY_V2_DISCRIMINATOR, accounts = BuyV2Accounts)]
    BuyV2(BuyV2Params),
    /// Exact-in buy: pins the SOL spent, not the tokens received.
    #[instruction(discriminator = BUY_EXACT_SOL_IN_DISCRIMINATOR, accounts = BuyAccounts)]
    BuyExactSolIn(BuyExactSolInParams),
    /// Exact-in buy against the v2 layout, denominated in the pool's quote.
    #[instruction(discriminator = BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR, accounts = BuyV2Accounts)]
    BuyExactQuoteInV2(BuyExactQuoteInV2Params),
    /// Sell tokens back to the bonding curve.
    #[instruction(discriminator = SELL_DISCRIMINATOR, accounts = SellAccounts)]
    Sell(SellParams),
    /// Sell against the v2 layout.
    #[instruction(discriminator = SELL_V2_DISCRIMINATOR, accounts = SellV2Accounts)]
    SellV2(SellV2Params),
    /// Legacy `create` (pre-2024), now rare in production.
    #[instruction(discriminator = CREATE_DISCRIMINATOR, accounts = CreateAccounts)]
    Create(CreateParams),
    /// Modern `create_v2`: 16 slots rather than 14, `user` at slot 5 not 7, and
    /// an explicit `creator` argument.
    #[instruction(discriminator = CREATE_V2_DISCRIMINATOR, accounts = CreateV2Accounts)]
    CreateV2(CreateV2Params),
    /// A bonding curve graduating to the PumpSwap AMM.
    #[instruction(discriminator = MIGRATE_DISCRIMINATOR, accounts = MigrateAccounts)]
    Migrate(MigrateParams),
    /// The same, against the v2 layout.
    #[instruction(discriminator = MIGRATE_V2_DISCRIMINATOR, accounts = MigrateV2Accounts)]
    MigrateV2(MigrateV2Params),
    /// A creator draining their fee vault.
    #[instruction(discriminator = COLLECT_CREATOR_FEE_DISCRIMINATOR, accounts = CollectCreatorFeeAccounts)]
    CollectCreatorFee(CollectCreatorFeeParams),
    /// The same, settling into a token account.
    #[instruction(discriminator = COLLECT_CREATOR_FEE_V2_DISCRIMINATOR, accounts = CollectCreatorFeeV2Accounts)]
    CollectCreatorFeeV2(CollectCreatorFeeV2Params),
    /// A vault split across a sharing config.
    #[instruction(discriminator = DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR, accounts = DistributeCreatorFeesAccounts)]
    DistributeCreatorFees(DistributeCreatorFeesParams),
    /// The same, able to create the recipient token account on the way.
    #[instruction(discriminator = DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR, accounts = DistributeCreatorFeesV2Accounts)]
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
    /// `None` only for the creator-fee forms, which carry no account struct:
    /// their on-chain lists run longer than the IDL declares in ways not yet
    /// settled against real instructions. Returning a defaulted pubkey there
    /// would be an answer we do not have.
    ///
    /// The v2 swap forms *do* answer: their appended accounts are located by
    /// deriving them, so every declared slot sits at its declared index.
    #[must_use]
    pub fn mint(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.mint),
            Self::Sell(a) => Some(a.mint),
            Self::Create(a) => Some(a.mint),
            Self::CreateV2(a) => Some(a.mint),
            Self::BuyV2(a) | Self::BuyExactQuoteInV2(a) => Some(a.base_mint),
            Self::SellV2(a) => Some(a.base_mint),
            Self::DistributeCreatorFees(a) => Some(a.mint),
            Self::DistributeCreatorFeesV2(a) => Some(a.mint),
            Self::Migrate(a) => Some(a.mint),
            Self::MigrateV2(a) => Some(a.base_mint),
            // The collect forms pay out of a vault; no coin mint is in scope.
            // `collect_creator_fee_v2` carries a `quote_mint`, which is the
            // settlement asset, not the coin — returning it here would answer a
            // different question than the one asked.
            Self::CollectCreatorFee(_) | Self::CollectCreatorFeeV2(_) => None,
        }
    }

    /// The bonding curve backing [`mint`](Self::mint).
    ///
    /// `None` only for the creator-fee forms, which carry no account struct:
    /// their on-chain lists run longer than the IDL declares in ways not yet
    /// settled against real instructions. Returning a defaulted pubkey there
    /// would be an answer we do not have.
    ///
    /// The v2 swap forms *do* answer: their appended accounts are located by
    /// deriving them, so every declared slot sits at its declared index.
    #[must_use]
    pub fn bonding_curve(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.bonding_curve),
            Self::Sell(a) => Some(a.bonding_curve),
            Self::Create(a) => Some(a.bonding_curve),
            Self::CreateV2(a) => Some(a.bonding_curve),
            Self::BuyV2(a) | Self::BuyExactQuoteInV2(a) => Some(a.bonding_curve),
            Self::SellV2(a) => Some(a.bonding_curve),
            Self::DistributeCreatorFees(a) => Some(a.bonding_curve),
            Self::DistributeCreatorFeesV2(a) => Some(a.bonding_curve),
            Self::Migrate(a) => Some(a.bonding_curve),
            Self::MigrateV2(a) => Some(a.bonding_curve),
            Self::CollectCreatorFee(_) | Self::CollectCreatorFeeV2(_) => None,
        }
    }

    /// The wallet that signed — trader on a swap, creator on a launch.
    ///
    /// `None` only for the creator-fee forms, which carry no account struct:
    /// their on-chain lists run longer than the IDL declares in ways not yet
    /// settled against real instructions. Returning a defaulted pubkey there
    /// would be an answer we do not have.
    ///
    /// The v2 swap forms *do* answer: their appended accounts are located by
    /// deriving them, so every declared slot sits at its declared index.
    #[must_use]
    pub fn user(&self) -> Option<Pubkey> {
        match self {
            Self::Buy(a) | Self::BuyExactSolIn(a) => Some(a.user),
            Self::Sell(a) => Some(a.user),
            Self::Create(a) => Some(a.user),
            Self::CreateV2(a) => Some(a.user),
            Self::BuyV2(a) | Self::BuyExactQuoteInV2(a) => Some(a.user),
            Self::SellV2(a) => Some(a.user),
            Self::CollectCreatorFee(a) => Some(a.creator),
            Self::CollectCreatorFeeV2(a) => Some(a.creator),
            Self::DistributeCreatorFeesV2(a) => Some(a.payer),
            // The migration is cranked by whoever pays for it, not by the
            // coin's owner — `user` here is the transaction's payer.
            Self::Migrate(a) => Some(a.user),
            Self::MigrateV2(a) => Some(a.user),
            // `distribute_creator_fees` has no signer slot at all — anyone may
            // crank it — so there is no wallet to name.
            Self::DistributeCreatorFees(_) => None,
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
            Pubkey::new_unique(),
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
            Pubkey::new_unique(),
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

#[cfg(test)]
mod v2_account_layout {
    use super::*;
    use solana_program::pubkey::Pubkey;
    use std::str::FromStr;

    /// 63 real mainnet v2 instructions, captured with their full account lists.
    fn records() -> Vec<serde_json::Value> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pumpfun/v2recs.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("v2recs is readable"))
            .expect("v2recs parses")
    }

    fn keys(r: &serde_json::Value) -> Vec<Pubkey> {
        r["accounts"]
            .as_array()
            .expect("accounts array")
            .iter()
            .map(|k| Pubkey::from_str(k.as_str().expect("pubkey string")).expect("valid pubkey"))
            .collect()
    }

    /// Every named slot lands on the account the IDL says it should, at every
    /// observed length — the claim the whole v2 account struct rests on.
    ///
    /// If a program ever inserts an account mid-list instead of appending,
    /// `global` or `program` moves and this fails immediately.
    #[test]
    fn named_slots_are_stable_across_every_observed_length() {
        let global = super::super::constants::GLOBAL_PDA;
        let program = super::super::constants::PROGRAM_ID;
        for r in records() {
            let ix = r["ix"].as_str().expect("ix name");
            let k = keys(&r);
            let mint = Pubkey::from_str(r["mint"].as_str().expect("mint")).expect("valid mint");
            let user = Pubkey::from_str(r["user"].as_str().expect("user")).expect("valid user");
            let curve = super::super::accounts::derive_bonding_curve_pda(&mint);

            let (got_global, got_mint, got_curve, got_user, got_program) = match ix {
                "buy_v2" | "buy_exact_quote_in_v2" => {
                    let a = BuyV2Accounts::from_pubkeys(&k).expect("v2 buy accounts parse");
                    (a.global, a.base_mint, a.bonding_curve, a.user, a.program)
                }
                "sell_v2" => {
                    let a = SellV2Accounts::from_pubkeys(&k).expect("v2 sell accounts parse");
                    (a.global, a.base_mint, a.bonding_curve, a.user, a.program)
                }
                other => panic!("unexpected instruction in v2recs: {other}"),
            };
            assert_eq!(got_global, global, "{ix} n={}: global slot", k.len());
            assert_eq!(got_mint, mint, "{ix} n={}: base_mint slot", k.len());
            assert_eq!(got_curve, curve, "{ix} n={}: bonding_curve slot", k.len());
            assert_eq!(got_user, user, "{ix} n={}: user slot", k.len());
            // The terminator. Its position proves the whole named prefix: an
            // inserted account would shift it and nothing else needs checking.
            assert_eq!(
                got_program,
                program,
                "{ix} n={}: program terminator",
                k.len()
            );
        }
    }

    /// A consumer asks for `bonding_curve_v2` by name and gets it, at whichever
    /// index the caller happened to put it.
    ///
    /// This is why the appended accounts are resolved by derivation: across
    /// these records the account sits at tail index 0 on most and index 1 on the
    /// rest, so a consumer reading a fixed slot would be right most of the time
    /// and wrong the rest — the worst available outcome.
    #[test]
    fn the_appended_accounts_are_reachable_by_name() {
        use crate::parsing::accounts::Conditional;
        let (mut with_curve, mut with_vaults, mut total) = (0usize, 0usize, 0usize);
        for r in records() {
            let ix = r["ix"].as_str().expect("ix");
            let k = keys(&r);
            let named = if ix == "sell_v2" { 26 } else { 27 };
            if k.len() <= named {
                continue;
            }
            total += 1;
            let mint = k[1];
            let expected = super::super::accounts::derive_bonding_curve_v2_pda(&mint);
            let (curve, vaults) = match ix {
                "buy_v2" | "buy_exact_quote_in_v2" => {
                    let a = BuyV2Accounts::from_pubkeys(&k).expect("parses");
                    (a.bonding_curve_v2, a.buyback_vaults)
                }
                "sell_v2" => {
                    let a = SellV2Accounts::from_pubkeys(&k).expect("parses");
                    (a.bonding_curve_v2, a.buyback_vaults)
                }
                other => panic!("unexpected {other}"),
            };
            assert_eq!(
                curve,
                Conditional::Present(expected),
                "{ix}: bonding_curve_v2 must resolve regardless of its index"
            );
            with_curve += 1;
            with_vaults += usize::from(!vaults.is_empty());
        }
        assert!(total > 0, "no tailed records — the fixture changed");
        assert_eq!(with_curve, total, "every tail carries a bonding_curve_v2");
        assert!(
            with_vaults > 0,
            "the buyback_vaults path is never exercised"
        );
    }

    /// Nothing dropped, nothing invented: the named appended fields plus the
    /// vaults account for exactly the accounts the instruction carried.
    #[test]
    fn every_appended_account_is_still_accounted_for() {
        for r in records() {
            let ix = r["ix"].as_str().expect("ix");
            let k = keys(&r);
            let (named, uva, curve, vaults) = match ix {
                "buy_v2" | "buy_exact_quote_in_v2" => {
                    let a = BuyV2Accounts::from_pubkeys(&k).expect("parses");
                    (
                        BuyV2Accounts::ACCOUNT_COUNT,
                        a.appended_user_volume_accumulator,
                        a.bonding_curve_v2,
                        a.buyback_vaults.len(),
                    )
                }
                "sell_v2" => {
                    let a = SellV2Accounts::from_pubkeys(&k).expect("parses");
                    (
                        SellV2Accounts::ACCOUNT_COUNT,
                        a.appended_user_volume_accumulator,
                        a.bonding_curve_v2,
                        a.buyback_vaults.len(),
                    )
                }
                other => panic!("unexpected {other}"),
            };
            let counted =
                named + usize::from(uva.is_present()) + usize::from(curve.is_present()) + vaults;
            assert_eq!(counted, k.len(), "{ix} lost or invented an account");
        }
    }
}
