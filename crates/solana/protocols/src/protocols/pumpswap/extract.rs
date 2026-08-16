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
use tracing::warn;

use super::events::{BuyEvent, SellEvent, BUY_EVENT_DISCRIMINATOR, SELL_EVENT_DISCRIMINATOR};
use super::{
    BuyAccounts, CreatePoolAccounts, PumpSwapInstruction, SellAccounts,
    PROGRAM_ID as PUMPSWAP_PROGRAM,
};
use crate::chain::{ChainEvent, CurveState, ExtractContext, Migration, ProtocolExtractor, Swap};
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
        _ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        // Skip inner event-CPI ixs — they're walked from the outer
        // Buy/Sell ix below. We identify them by the Anchor event
        // envelope at byte 0.
        if ix.data.len() >= 8 && ix.data[..8] == ANCHOR_EVENT_DISCRIMINATOR {
            return None;
        }

        let pumpswap_ix = match PumpSwapInstruction::try_from_slice(&ix.data) {
            Ok(v) => v,
            Err(e) => {
                // Not "skipping" — this is an instruction on a program we CLAIM
                // to decode, so failing to parse it is a gap in us, not a
                // non-event. Retained with its bytes so the parser can be
                // fixed later; this branch used to `trace!` and vanish.
                crate::undecoded::report(&ix.program_id, &ix.data, &ix.accounts, &format!("{e:?}"));
                return None;
            }
        };

        match pumpswap_ix {
            // Both buy forms emit the same BuyEvent and differ only in which
            // side the trader pinned, so extraction is shared. Which one ran
            // is preserved on the row via `Swap.instruction`, because the
            // rounding direction differs and the quote math must not be
            // graded across the two together.
            PumpSwapInstruction::Buy(_) | PumpSwapInstruction::BuyExactQuoteIn(_) => {
                extract_buy(ix, all_instructions)
            }
            PumpSwapInstruction::Sell(_) => extract_sell(ix, all_instructions),
            PumpSwapInstruction::CreatePool(_) => extract_create_pool(ix),
            // Deposit / Withdraw don't produce trade events we model.
            PumpSwapInstruction::Deposit(_) | PumpSwapInstruction::Withdraw(_) => None,
        }
    }
}

fn extract_buy(ix: &ParsedInstruction, all: &[ParsedInstruction]) -> Option<ChainEvent> {
    let accounts = match BuyAccounts::from_account_keys(&ix.accounts) {
        Ok(a) => a,
        Err(e) => {
            warn!(
                ?e,
                ix_index = ix.instruction_index,
                "pumpswap buy: account parse failed"
            );
            return None;
        }
    };

    let event = find_buy_event(ix, all)?;
    if event.pool != accounts.pool {
        warn!(
            event_pool = %event.pool,
            outer_pool = %accounts.pool,
            "pumpswap buy: BuyEvent pool mismatches outer ix"
        );
        return None;
    }

    // PumpSwap pools are quote/base. For pumpfun-origin pools the
    // quote is WSOL and base is the memecoin; we map that straight
    // onto token_in (paid) / token_out (received) for buys.
    let amount_in = event.gross_quote_in();
    let amount_out = event.base_amount_out;
    let fee_amount = event.lp_fee + event.protocol_fee + event.coin_creator_fee.unwrap_or(0);

    Some(ChainEvent::Swap(Swap {
        instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
        protocol: Protocol::PumpSwap,
        pool: accounts.pool,
        trader: accounts.user,
        token_in: accounts.quote_mint,
        amount_in,
        token_out: accounts.base_mint,
        amount_out,
        fee_amount,
        fee_mint: accounts.quote_mint,
        state_after: None,
        // Measured PRE-swap: see `Swap::state_before`.
        state_before: Some(CurveState::Reserves {
            // Buy: token_in = quote, token_out = base.
            in_side: event.pool_quote_token_reserves,
            out_side: event.pool_base_token_reserves,
        }),
    }))
}

fn extract_sell(ix: &ParsedInstruction, all: &[ParsedInstruction]) -> Option<ChainEvent> {
    let accounts = match SellAccounts::from_account_keys(&ix.accounts) {
        Ok(a) => a,
        Err(e) => {
            warn!(
                ?e,
                ix_index = ix.instruction_index,
                "pumpswap sell: account parse failed"
            );
            return None;
        }
    };

    let event = find_sell_event(ix, all)?;
    if event.pool != accounts.pool {
        warn!(
            event_pool = %event.pool,
            outer_pool = %accounts.pool,
            "pumpswap sell: SellEvent pool mismatches outer ix"
        );
        return None;
    }

    // Sell: token_in = base (mint), token_out = quote (WSOL).
    // user_quote_amount_out is the net SOL the user received after
    // all fees — that's the realized amount_out.
    let amount_in = event.base_amount_in;
    let amount_out = event.user_quote_amount_out;
    let fee_amount = event.lp_fee + event.protocol_fee + event.coin_creator_fee.unwrap_or(0);

    Some(ChainEvent::Swap(Swap {
        instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
        protocol: Protocol::PumpSwap,
        pool: accounts.pool,
        trader: accounts.user,
        token_in: accounts.base_mint,
        amount_in,
        token_out: accounts.quote_mint,
        amount_out,
        fee_amount,
        fee_mint: accounts.quote_mint,
        state_after: None,
        // Measured PRE-swap: see `Swap::state_before`.
        state_before: Some(CurveState::Reserves {
            // Sell: token_in = base, token_out = quote.
            in_side: event.pool_base_token_reserves,
            out_side: event.pool_quote_token_reserves,
        }),
    }))
}

fn extract_create_pool(ix: &ParsedInstruction) -> Option<ChainEvent> {
    let accounts = match CreatePoolAccounts::from_account_keys(&ix.accounts) {
        Ok(a) => a,
        Err(e) => {
            warn!(?e, "pumpswap create_pool: account parse failed");
            return None;
        }
    };

    // Emit a Migration event. We don't know the source bonding-curve
    // pool or the exact migrated amounts from the create_pool ix
    // alone — consumers that care can cross-reference with prior
    // pumpfun creation events for `accounts.base_mint`. Migrated
    // amounts default to 0; downstream code shouldn't rely on
    // create-time amounts (the pool's first swap reflects real state).
    Some(ChainEvent::Migration(Migration {
        from_protocol: Protocol::Pumpfun,
        to_protocol: Protocol::PumpSwap,
        mint: accounts.base_mint,
        from_pool: Pubkey::default(),
        to_pool: accounts.pool,
        migrated_sol: 0,
        migrated_tokens: 0,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Event lookup — walk children for the matching emit_cpi! event.
// Falls back to the outer ix's program-data log for legacy emit!.
// ─────────────────────────────────────────────────────────────────────────────

fn find_buy_event(ix: &ParsedInstruction, all: &[ParsedInstruction]) -> Option<BuyEvent> {
    if let Some(body) = find_inner_event_body(ix, all, &BUY_EVENT_DISCRIMINATOR) {
        if let Some(ev) = BuyEvent::from_body(body) {
            return Some(ev);
        }
    }
    // Legacy emit! — body lives on a program-data log under the
    // event-name discriminator.
    if let Some(payload) = ix.find_data_log_with_discriminator(&BUY_EVENT_DISCRIMINATOR) {
        if let Some(ev) = BuyEvent::from_body(payload) {
            return Some(ev);
        }
    }
    warn!("pumpswap: BuyEvent not found via emit_cpi! or emit! paths");
    None
}

fn find_sell_event(ix: &ParsedInstruction, all: &[ParsedInstruction]) -> Option<SellEvent> {
    if let Some(body) = find_inner_event_body(ix, all, &SELL_EVENT_DISCRIMINATOR) {
        if let Some(ev) = SellEvent::from_body(body) {
            return Some(ev);
        }
    }
    if let Some(payload) = ix.find_data_log_with_discriminator(&SELL_EVENT_DISCRIMINATOR) {
        if let Some(ev) = SellEvent::from_body(payload) {
            return Some(ev);
        }
    }
    warn!("pumpswap: SellEvent not found via emit_cpi! or emit! paths");
    None
}

/// Walk the children of `outer_ix` looking for an inner self-CPI ix
/// whose data is `[ANCHOR_EVENT_DISCRIMINATOR (8) || expected_disc
/// (8) || body]`. Returns the body slice (no prefix).
fn find_inner_event_body<'a>(
    outer_ix: &ParsedInstruction,
    all: &'a [ParsedInstruction],
    expected_event_disc: &[u8; 8],
) -> Option<&'a [u8]> {
    let outer_idx = outer_ix.instruction_index;
    for child in all.iter() {
        if child.parent_index != Some(outer_idx) {
            continue;
        }
        if child.program_id != PUMPSWAP_PROGRAM {
            continue;
        }
        if child.data.len() < 16 {
            continue;
        }
        if child.data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
            continue;
        }
        if &child.data[8..16] != expected_event_disc {
            continue;
        }
        return Some(&child.data[16..]);
    }
    None
}
