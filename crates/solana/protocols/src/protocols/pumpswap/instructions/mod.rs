//! PumpSwap instruction modules.
//!
//! Each instruction gets its own file with accounts struct + params struct.
//! The `ProtocolInstruction` derive macro generates dispatch, accounts enum, and event struct.

pub mod buy;
pub mod create_pool;
pub mod deposit;
pub mod sell;
pub mod withdraw;

pub use buy::{BuyAccounts, BuyBuilder, BuyExactQuoteInAccounts, BuyExactQuoteInParams, BuyParams};
pub use create_pool::{CreatePoolAccounts, CreatePoolParams};
pub use deposit::{DepositAccounts, DepositParams};
pub use sell::{SellAccounts, SellBuilder, SellParams};
pub use withdraw::{WithdrawAccounts, WithdrawParams};

use solana_protocols_macros::ProtocolInstruction;

use super::constants::{
    BUY_DISCRIMINATOR, BUY_EXACT_QUOTE_IN_DISCRIMINATOR, CREATE_POOL_DISCRIMINATOR,
    DEPOSIT_DISCRIMINATOR, PROGRAM_ID, SELL_DISCRIMINATOR, WITHDRAW_DISCRIMINATOR,
};
use crate::parsing::{ClassifiesAsSwap, SwapAmount, SwapClassification};
use crate::traits::SwapDirection;

/// PumpSwap instruction enum.
///
/// `#[derive(ProtocolInstruction)]` generates:
/// - `try_from_slice(data)` — discriminator dispatch + `FromInstructionData`
/// - `discriminator()` — get 8-byte discriminator
/// - `data()` — serialize back to bytes
/// - `from_accounts(keys)` — parse accounts via `FromAccountKeys`
/// - `PumpSwapInstructionAccounts` enum
/// - `PumpSwapInstructionEvent` struct
#[derive(Debug, Clone, ProtocolInstruction)]
#[protocol(program_id = PROGRAM_ID)]
pub enum PumpSwapInstruction {
    /// Buy base tokens with quote tokens.
    #[instruction(discriminator = BUY_DISCRIMINATOR, accounts = BuyAccounts)]
    Buy(BuyParams),
    /// Buy base tokens, pinning the quote spent rather than the base received.
    #[instruction(discriminator = BUY_EXACT_QUOTE_IN_DISCRIMINATOR, accounts = BuyAccounts)]
    BuyExactQuoteIn(BuyExactQuoteInParams),
    /// Sell base tokens for quote tokens.
    #[instruction(discriminator = SELL_DISCRIMINATOR, accounts = SellAccounts)]
    Sell(SellParams),
    /// Create a new AMM pool with initial liquidity.
    #[instruction(discriminator = CREATE_POOL_DISCRIMINATOR, accounts = CreatePoolAccounts)]
    CreatePool(CreatePoolParams),
    /// Add liquidity to an existing pool.
    #[instruction(discriminator = DEPOSIT_DISCRIMINATOR, accounts = DepositAccounts)]
    Deposit(DepositParams),
    /// Remove liquidity from a pool.
    #[instruction(discriminator = WITHDRAW_DISCRIMINATOR, accounts = WithdrawAccounts)]
    Withdraw(WithdrawParams),
}

impl PumpSwapInstruction {
    /// Check if this is a buy instruction.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        matches!(self, PumpSwapInstruction::Buy(_))
    }

    /// Check if this is a sell instruction.
    #[must_use]
    pub fn is_sell(&self) -> bool {
        matches!(self, PumpSwapInstruction::Sell(_))
    }

    /// Check if this is a create pool instruction.
    #[must_use]
    pub fn is_create_pool(&self) -> bool {
        matches!(self, PumpSwapInstruction::CreatePool(_))
    }

    /// Check if this is a deposit instruction.
    #[must_use]
    pub fn is_deposit(&self) -> bool {
        matches!(self, PumpSwapInstruction::Deposit(_))
    }

    /// Check if this is a withdraw instruction.
    #[must_use]
    pub fn is_withdraw(&self) -> bool {
        matches!(self, PumpSwapInstruction::Withdraw(_))
    }
}

impl ClassifiesAsSwap for PumpSwapInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            // `buy` pins the BASE OUT and ceilings the quote spent. The out
            // side was `Minimum` here, which had it backwards: the program
            // delivers exactly `base_amount_out` or fails.
            PumpSwapInstruction::Buy(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Maximum(params.max_quote_amount_in),
                SwapAmount::Exact(params.base_amount_out),
            )),
            // The mirror image: quote in is pinned, base out is a floor.
            PumpSwapInstruction::BuyExactQuoteIn(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Exact(params.spendable_quote_in),
                SwapAmount::Minimum(params.min_base_amount_out),
            )),
            PumpSwapInstruction::Sell(params) => Some(SwapClassification::new(
                SwapDirection::Sell,
                SwapAmount::Exact(params.base_amount_in),
                SwapAmount::Minimum(params.min_quote_amount_out),
            )),
            PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_)
            | PumpSwapInstruction::Withdraw(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::constants::{
        BUY_DISCRIMINATOR, CREATE_POOL_DISCRIMINATOR, DEPOSIT_DISCRIMINATOR, SELL_DISCRIMINATOR,
        WITHDRAW_DISCRIMINATOR,
    };
    use super::*;

    fn make_buy_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&BUY_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes());
        data.extend_from_slice(&500_000_000u64.to_le_bytes());
        data
    }

    fn make_sell_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&SELL_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes());
        data.extend_from_slice(&400_000_000u64.to_le_bytes());
        data
    }

    fn make_create_pool_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&CREATE_POOL_DISCRIMINATOR);
        data.extend_from_slice(&1u16.to_le_bytes()); // index
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // base_amount_in
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // quote_amount_in
        data
    }

    fn make_deposit_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&DEPOSIT_DISCRIMINATOR);
        data.extend_from_slice(&100_000u64.to_le_bytes()); // lp_token_amount_out
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // max_base_amount_in
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // max_quote_amount_in
        data
    }

    fn make_withdraw_data() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
        data.extend_from_slice(&100_000u64.to_le_bytes()); // lp_amount_in
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // min_base_amount_out
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // min_quote_amount_out
        data
    }

    #[test]
    fn parse_buy_instruction() {
        let data = make_buy_data();
        let ix = PumpSwapInstruction::try_from_slice(&data).unwrap();

        assert!(ix.is_buy());
        match ix {
            PumpSwapInstruction::Buy(params) => {
                assert_eq!(params.base_amount_out, 1_000_000);
                assert_eq!(params.max_quote_amount_in, 500_000_000);
            }
            PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected Buy"),
        }
    }

    #[test]
    fn parse_sell_instruction() {
        let data = make_sell_data();
        let ix = PumpSwapInstruction::try_from_slice(&data).unwrap();

        assert!(ix.is_sell());
        match ix {
            PumpSwapInstruction::Sell(params) => {
                assert_eq!(params.base_amount_in, 1_000_000);
                assert_eq!(params.min_quote_amount_out, 400_000_000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected Sell"),
        }
    }

    #[test]
    fn parse_create_pool_instruction() {
        let data = make_create_pool_data();
        let ix = PumpSwapInstruction::try_from_slice(&data).unwrap();

        assert!(ix.is_create_pool());
        match ix {
            PumpSwapInstruction::CreatePool(params) => {
                assert_eq!(params.index, 1);
                assert_eq!(params.base_amount_in, 1_000_000);
                assert_eq!(params.quote_amount_in, 500_000_000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::Deposit(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected CreatePool"),
        }
    }

    #[test]
    fn parse_deposit_instruction() {
        let data = make_deposit_data();
        let ix = PumpSwapInstruction::try_from_slice(&data).unwrap();

        assert!(ix.is_deposit());
        match ix {
            PumpSwapInstruction::Deposit(params) => {
                assert_eq!(params.lp_token_amount_out, 100_000);
                assert_eq!(params.max_base_amount_in, 1_000_000);
                assert_eq!(params.max_quote_amount_in, 500_000_000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected Deposit"),
        }
    }

    #[test]
    fn parse_withdraw_instruction() {
        let data = make_withdraw_data();
        let ix = PumpSwapInstruction::try_from_slice(&data).unwrap();

        assert!(ix.is_withdraw());
        match ix {
            PumpSwapInstruction::Withdraw(params) => {
                assert_eq!(params.lp_amount_in, 100_000);
                assert_eq!(params.min_base_amount_out, 1_000_000);
                assert_eq!(params.min_quote_amount_out, 500_000_000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_) => panic!("expected Withdraw"),
        }
    }

    #[test]
    fn parse_unknown_discriminator() {
        let data = vec![0xFF; 32];
        assert!(PumpSwapInstruction::try_from_slice(&data).is_err());
    }

    #[test]
    fn parse_too_short() {
        assert!(PumpSwapInstruction::try_from_slice(&[0; 4]).is_err());
        assert!(PumpSwapInstruction::try_from_slice(&BUY_DISCRIMINATOR).is_err());
    }

    #[test]
    fn buy_classifies_as_swap() {
        let ix = PumpSwapInstruction::Buy(BuyParams::new(1_000_000, 500_000_000));
        let swap = ix.as_swap().expect("should classify as swap");

        assert!(matches!(swap.direction, SwapDirection::Buy));
        assert!(matches!(swap.amount_in, SwapAmount::Maximum(500_000_000)));
        // Pinned side, not a floor — see the pumpfun twin of this test.
        assert!(matches!(swap.amount_out, SwapAmount::Exact(1_000_000)));
    }

    /// `buy_exact_quote_in` is the mirror: quote in pinned, base out floored.
    #[test]
    fn buy_exact_quote_in_pins_the_input() {
        let ix = PumpSwapInstruction::BuyExactQuoteIn(BuyExactQuoteInParams {
            spendable_quote_in: 500_000_000,
            min_base_amount_out: 1_000_000,
            track_volume: crate::protocols::OptionBool::None,
        });
        let swap = ix.as_swap().expect("should classify as swap");
        assert!(matches!(swap.amount_in, SwapAmount::Exact(500_000_000)));
        assert!(matches!(swap.amount_out, SwapAmount::Minimum(1_000_000)));
    }

    #[test]
    fn sell_classifies_as_swap() {
        let ix = PumpSwapInstruction::Sell(SellParams::new(1_000_000, 400_000_000));
        let swap = ix.as_swap().expect("should classify as swap");

        assert!(matches!(swap.direction, SwapDirection::Sell));
        assert!(matches!(swap.amount_in, SwapAmount::Exact(1_000_000)));
        assert!(matches!(swap.amount_out, SwapAmount::Minimum(400_000_000)));
    }

    #[test]
    fn lp_instructions_do_not_classify_as_swap() {
        let create = PumpSwapInstruction::CreatePool(CreatePoolParams::new(0, 1000, 2000));
        assert!(create.as_swap().is_none());

        let deposit = PumpSwapInstruction::Deposit(DepositParams::new(100, 1000, 2000));
        assert!(deposit.as_swap().is_none());

        let withdraw = PumpSwapInstruction::Withdraw(WithdrawParams::new(100, 1000, 2000));
        assert!(withdraw.as_swap().is_none());
    }

    #[test]
    fn roundtrip_via_data() {
        let original = PumpSwapInstruction::Buy(BuyParams::new(42, 100));
        let data = original.data();
        let parsed = PumpSwapInstruction::try_from_slice(&data).unwrap();
        match parsed {
            PumpSwapInstruction::Buy(params) => {
                assert_eq!(params.base_amount_out, 42);
                assert_eq!(params.max_quote_amount_in, 100);
            }
            PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected Buy"),
        }
    }

    #[test]
    fn roundtrip_deposit_via_data() {
        let original = PumpSwapInstruction::Deposit(DepositParams::new(500, 1000, 2000));
        let data = original.data();
        let parsed = PumpSwapInstruction::try_from_slice(&data).unwrap();
        match parsed {
            PumpSwapInstruction::Deposit(params) => {
                assert_eq!(params.lp_token_amount_out, 500);
                assert_eq!(params.max_base_amount_in, 1000);
                assert_eq!(params.max_quote_amount_in, 2000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Withdraw(_) => panic!("expected Deposit"),
        }
    }

    #[test]
    fn roundtrip_withdraw_via_data() {
        let original = PumpSwapInstruction::Withdraw(WithdrawParams::new(500, 1000, 2000));
        let data = original.data();
        let parsed = PumpSwapInstruction::try_from_slice(&data).unwrap();
        match parsed {
            PumpSwapInstruction::Withdraw(params) => {
                assert_eq!(params.lp_amount_in, 500);
                assert_eq!(params.min_base_amount_out, 1000);
                assert_eq!(params.min_quote_amount_out, 2000);
            }
            PumpSwapInstruction::Buy(_)
            | PumpSwapInstruction::BuyExactQuoteIn(_)
            | PumpSwapInstruction::Sell(_)
            | PumpSwapInstruction::CreatePool(_)
            | PumpSwapInstruction::Deposit(_) => panic!("expected Withdraw"),
        }
    }
}

#[cfg(test)]
mod account_list_drift {
    use super::{BuyAccounts, SellAccounts};
    use crate::test_fixtures::InstructionFixture;

    /// PumpSwap keeps appending accounts, and our structs trail the chain.
    ///
    /// Parsing is unaffected: positions `0..N` are stable and
    /// `from_pubkeys` ignores a longer list, which is why the round-trip
    /// fixtures pass and why the swap tape is correct. **Building is not** —
    /// an instruction assembled from these structs would be short of what
    /// the program now expects and would fail on chain.
    ///
    /// The round-trip test structurally cannot catch this: it compares the
    /// declared prefix, so growth past the last declared field is invisible
    /// to it. This test is the instrument for that, and it is why
    /// `#[derive(BuildAccounts)]` is not yet on these structs — annotating
    /// them against a list we know is incomplete would pin the wrong thing.
    ///
    /// Observed: 25 accounts on 2026-08-09, 26 on 2026-08-11 — twice in two
    /// days. When this fails, the chain moved again: recapture, identify the
    /// new accounts, extend the struct, then update the numbers here.
    #[test]
    fn the_declared_account_lists_trail_the_chain_by_a_known_amount() {
        for (name, declared, fixture, known_gap) in [
            ("buy", BuyAccounts::ACCOUNT_COUNT, "pumpswap/ix_buy.json", 3),
            (
                "sell",
                SellAccounts::ACCOUNT_COUNT,
                "pumpswap/ix_sell.json",
                2,
            ),
        ] {
            let onchain = InstructionFixture::load(fixture).pubkeys().len();
            assert!(
                onchain >= declared,
                "{name}: chain has {onchain} accounts, fewer than the {declared} we declare — \
                 parsing would now fail, not just building"
            );
            assert_eq!(
                onchain - declared,
                known_gap,
                "{name}: the account list moved — chain {onchain}, declared {declared}. \
                 Recapture the fixture, identify the new accounts, extend the struct."
            );
        }
    }
}
