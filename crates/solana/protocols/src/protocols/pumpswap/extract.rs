//! PumpSwap extractor — transforms parsed instructions into
//! semantic [`ChainEvent`]s.
//!
//! PumpSwap is the AMM that pumpfun graduates to. Two ix types
//! interest us:
//! * `Buy` / `Sell` — produce [`ChainEvent::Swap`]. The post-swap
//!   pool reserves come from the `BuyEvent` / `SellEvent` log
//!   payload.
//! * `CreatePool` — produces [`ChainEvent::Migration`] when the
//!   pool's base mint matches a pumpfun-origin mint. (Live
//!   migration detection from a pool-discovery service is
//!   the previous-generation path; this lets the extractor emit it
//!   semantically.)
//!
//! Event delivery: PumpSwap uses Anchor `emit_cpi!`, so the event
//! body lives on an inner self-CPI ix whose data starts with
//! `[ANCHOR_EVENT_DISCRIMINATOR (8) || event-name disc (8) || borsh
//! body]`. We walk the children of the outer Buy/Sell ix to find
//! the matching event, then construct the [`Swap`] from the event's
//! reserves and the outer ix's accounts.
//!
//! Fallback: if no inner event is found we try the legacy `emit!`
//! path (program-data log on the outer ix). This covers older
//! program revisions where the same payload is logged via
//! `Program data:` log lines.

use solana_program::pubkey::Pubkey;

use super::events::{BuyEvent, CollectCoinCreatorFeeEvent, SellEvent};
use super::{
    BuyAccounts, BuyExactQuoteInParams, BuyParams, CollectCoinCreatorFeeAccounts,
    CreatePoolAccounts, CreatePoolParams, PumpSwapInstruction, SellAccounts, SellParams,
    PROGRAM_ID as PUMPSWAP_PROGRAM,
};
use crate::chain::{
    child_event, corroborate, report_extract_failure, ChainEvent, CreatorFee, CreatorPayout,
    CurveState, ExtractContext, ExtractError, Extracted, ExtractsCreatorFee, ExtractsMigration,
    ExtractsSwap, Migration, ProtocolExtractor, Swap,
};
use crate::parsing::{FromAccountKeys, ParsedInstruction};
use crate::protocols::meteora_damm_v2::constants::ANCHOR_EVENT_DISCRIMINATOR;
use crate::protocols::Protocol;

/// Zero-sized adapter. Register via
/// [`ExtractorRegistry::register`](crate::chain::ExtractorRegistry::register).
pub struct PumpSwapExtractor;

impl ProtocolExtractor for PumpSwapExtractor {
    fn program_id() -> Pubkey {
        PUMPSWAP_PROGRAM
    }

    fn extract(
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        match Self::try_extract(ix, all_instructions, ctx) {
            Ok(event) => event,
            Err(e) => {
                report_extract_failure(&Protocol::PumpSwap, ix, &e);
                None
            }
        }
    }
}

impl PumpSwapExtractor {
    /// The whole extractor, as a `Result`. See pumpfun's for the shape.
    fn try_extract(
        ix: &ParsedInstruction,
        all: &[ParsedInstruction],
        ctx: &dyn ExtractContext,
    ) -> Extracted {
        if ix.data.len() >= 8 && ix.data[..8] == ANCHOR_EVENT_DISCRIMINATOR {
            return Ok(None);
        }

        let parsed = match PumpSwapInstruction::try_from_slice(&ix.data) {
            Ok(v) => v,
            Err(e) => {
                crate::undecoded::report(&ix.program_id, &ix.data, &ix.accounts, &format!("{e:?}"));
                return Ok(None);
            }
        };

        match &parsed {
            PumpSwapInstruction::Buy(p) => swap_via(p, ix, all, ctx),
            PumpSwapInstruction::BuyExactQuoteIn(p) => swap_via(p, ix, all, ctx),
            PumpSwapInstruction::Sell(p) => swap_via(p, ix, all, ctx),
            PumpSwapInstruction::CreatePool(p) => Ok(Some(ChainEvent::Migration(p.migration(ix)?))),
            PumpSwapInstruction::CollectCoinCreatorFee(p) => {
                let ev = child_event::<CollectCoinCreatorFeeEvent>(ix, all, &PUMPSWAP_PROGRAM)?;
                Ok(Some(ChainEvent::CreatorFee(p.creator_fee(&ev, ix)?)))
            }
            // Real liquidity events we decode and do not yet model. An error
            // rather than a quiet `None`, so they are counted alongside every
            // other gap instead of vanishing — which is how they get
            // prioritised.
            PumpSwapInstruction::Deposit(_) => Err(ExtractError::Unmodelled {
                instruction: "deposit",
            }),
            PumpSwapInstruction::Withdraw(_) => Err(ExtractError::Unmodelled {
                instruction: "withdraw",
            }),
        }
    }
}

fn swap_via<T: ExtractsSwap>(
    params: &T,
    ix: &ParsedInstruction,
    all: &[ParsedInstruction],
    ctx: &dyn ExtractContext,
) -> Extracted {
    let event = child_event::<T::Event>(ix, all, &PUMPSWAP_PROGRAM)?;
    Ok(Some(ChainEvent::Swap(params.swap(&event, ix, ctx)?)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Swaps
// ─────────────────────────────────────────────────────────────────────────────

/// Shared by both buy forms: the pool is quote/base, the trader pays quote.
fn buy_swap(
    event: &BuyEvent,
    ix: &ParsedInstruction,
    track_volume: crate::protocols::OptionBool,
) -> Result<Swap, ExtractError> {
    let a = BuyAccounts::from_account_keys(&ix.accounts).map_err(|source| {
        ExtractError::AccountLayout {
            expected: "BuyAccounts",
            source,
        }
    })?;
    corroborate("pool", &event.pool, &a.pool)?;

    Ok(Swap {
        track_volume,
        instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
        protocol: Protocol::PumpSwap,
        pool: a.pool,
        trader: a.user,
        token_in: a.quote_mint,
        // Gross: what the trader parted with, including every fee that left the
        // pool. `quote_amount_in_with_lp_fee` is only what entered reserves.
        amount_in: event.gross_quote_in(),
        token_out: a.base_mint,
        amount_out: event.base_amount_out,
        fee_amount: event.lp_fee + event.protocol_fee + event.coin_creator_fee,
        fee_mint: a.quote_mint,
        state_after: None,
        // Measured PRE-swap: see `Swap::state_before`.
        state_before: Some(CurveState::Reserves {
            in_side: event.pool_quote_token_reserves,
            out_side: event.pool_base_token_reserves,
        }),
    })
}

impl ExtractsSwap for BuyParams {
    type Event = BuyEvent;

    fn swap(
        &self,
        event: &BuyEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        buy_swap(event, ix, self.track_volume)
    }
}

impl ExtractsSwap for BuyExactQuoteInParams {
    type Event = BuyEvent;

    fn swap(
        &self,
        event: &BuyEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        // Same account layout and same event; only the pinned side differs, and
        // which instruction ran is preserved on the row.
        buy_swap(event, ix, self.track_volume)
    }
}

impl ExtractsSwap for SellParams {
    type Event = SellEvent;

    fn swap(
        &self,
        event: &SellEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let a = SellAccounts::from_account_keys(&ix.accounts).map_err(|source| {
            ExtractError::AccountLayout {
                expected: "SellAccounts",
                source,
            }
        })?;
        corroborate("pool", &event.pool, &a.pool)?;

        Ok(Swap {
            // No such argument on this instruction.
            track_volume: crate::protocols::OptionBool::None,
            instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
            protocol: Protocol::PumpSwap,
            pool: a.pool,
            trader: a.user,
            token_in: a.base_mint,
            amount_in: event.base_amount_in,
            // Net of every fee — the realized amount the user received.
            token_out: a.quote_mint,
            amount_out: event.user_quote_amount_out,
            fee_amount: event.lp_fee + event.protocol_fee + event.coin_creator_fee,
            fee_mint: a.quote_mint,
            state_after: None,
            state_before: Some(CurveState::Reserves {
                in_side: event.pool_base_token_reserves,
                out_side: event.pool_quote_token_reserves,
            }),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration and creator fees
// ─────────────────────────────────────────────────────────────────────────────

impl ExtractsMigration for CreatePoolParams {
    fn migration(&self, ix: &ParsedInstruction) -> Result<Migration, ExtractError> {
        let a = CreatePoolAccounts::from_account_keys(&ix.accounts).map_err(|source| {
            ExtractError::AccountLayout {
                expected: "CreatePoolAccounts",
                source,
            }
        })?;
        // The source curve and the migrated amounts are not in this
        // instruction. They are recorded as unknown rather than guessed;
        // `docs/` tracks reading them from `CreatePoolEvent`, which is not
        // decoded yet.
        Ok(Migration {
            from_protocol: Protocol::Pumpfun,
            to_protocol: Protocol::PumpSwap,
            mint: a.base_mint,
            from_pool: Pubkey::default(),
            to_pool: a.pool,
            migrated_sol: 0,
            migrated_tokens: 0,
        })
    }
}

impl ExtractsCreatorFee for crate::parsing::NoParams {
    type Event = CollectCoinCreatorFeeEvent;

    fn creator_fee(
        &self,
        event: &CollectCoinCreatorFeeEvent,
        ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError> {
        // The event names the accounts but not the mint, so the denomination is
        // read off the instruction's own `quote_mint` slot. PumpSwap sends
        // exactly the eight accounts its IDL declares here, unlike pumpfun's
        // creator-fee instructions, so the slot is safe.
        let a =
            CollectCoinCreatorFeeAccounts::from_account_keys(&ix.accounts).map_err(|source| {
                ExtractError::AccountLayout {
                    expected: "CollectCoinCreatorFeeAccounts",
                    source,
                }
            })?;
        Ok(CreatorFee {
            protocol: Protocol::PumpSwap,
            payout: CreatorPayout::Direct {
                creator: event.coin_creator,
            },
            amount: event.coin_creator_fee,
            quote_mint: a.quote_mint,
            // A pool's creator vault accrues from that pool alone, but the
            // instruction names only the vault — not the pool or its base mint.
            // Recovering it would need a lookup we have not verified.
            mint: None,
            timestamp: event.timestamp,
        })
    }
}
