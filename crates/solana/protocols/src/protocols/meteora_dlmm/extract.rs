//! [`ProtocolExtractor`] impl for Meteora DLMM.
//!
//! Surfaces [`ChainEvent::Swap`] for every successful DLMM swap (six
//! ix variants — `swap`, `swap2`, `swap_exact_out`, `swap_exact_out2`,
//! `swap_with_price_impact`, `swap_with_price_impact2`). Uniquely among
//! our protocols, both sides of the curve state are published: the event's
//! `start_bin_id` / `end_bin_id` give the price before *and* after, so price
//! impact is directly observable rather than derived.
//!
//! Non-swap ixs (liquidity ops, position lifecycle, admin) parse
//! cleanly via [`super::instructions::parse`] but don't produce a
//! [`ChainEvent`] today — the firehose currently models market data
//! (swaps, creations, migrations) rather than LP-level events. If a
//! consumer needs LP visibility the right shape is to add new
//! [`ChainEvent`] variants and re-route here.
//!
//! Inner self-CPI ixs (the event envelope) are short-circuited at the
//! top of `extract` so they don't try to parse as outer
//! instructions.

use solana_program::pubkey::Pubkey;
use tracing::warn;

use crate::chain::{ChainEvent, CurveState, ExtractContext, ProtocolExtractor, Swap};
use crate::parsing::ParsedInstruction;
use crate::protocols::meteora_damm_v2::constants::ANCHOR_EVENT_DISCRIMINATOR;
use crate::protocols::Protocol;

use super::events::{
    find_event_body,
    swap::{SwapEvent, SWAP_EVENT_DISCRIMINATOR},
};
use super::instructions::{
    swap::SwapAccounts as SwapV1Accounts, swap2::Swap2Accounts,
    swap_exact_out::SwapExactOutAccounts, swap_exact_out2::SwapExactOut2Accounts,
    swap_with_price_impact::SwapWithPriceImpactAccounts,
    swap_with_price_impact2::SwapWithPriceImpact2Accounts,
};
use super::{parse_instruction, MeteoraDlmmInstruction, PROGRAM_ID};

/// Zero-sized handle. Function pointer goes into the registry.
pub struct MeteoraDlmmExtractor;

impl ProtocolExtractor for MeteoraDlmmExtractor {
    fn program_id() -> Pubkey {
        PROGRAM_ID
    }

    fn extract(
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        _ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        // Inner event-CPI ixs share program id with outer ixs but
        // start with the Anchor event envelope. They're walked from
        // the outer ix below, not parsed as outer ixs.
        if ix.data.len() >= 8 && ix.data[..8] == ANCHOR_EVENT_DISCRIMINATOR {
            return None;
        }

        let parsed = match parse_instruction(ix) {
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

        // Pull the four fields every swap variant shares (lb_pair,
        // user, token_x_mint, token_y_mint) without committing to
        // one variant at a time. Each branch reads its own typed
        // accounts struct (already parsed by the dispatcher) and
        // narrows to the common shape.
        let common = match &parsed {
            MeteoraDlmmInstruction::Swap(b) => Some(common_from_v1(&b.accounts)),
            MeteoraDlmmInstruction::Swap2(b) => Some(common_from_v2(&b.accounts)),
            MeteoraDlmmInstruction::SwapExactOut(b) => Some(common_from_exact_out(&b.accounts)),
            MeteoraDlmmInstruction::SwapExactOut2(b) => Some(common_from_exact_out2(&b.accounts)),
            MeteoraDlmmInstruction::SwapWithPriceImpact(b) => Some(common_from_pi(&b.accounts)),
            MeteoraDlmmInstruction::SwapWithPriceImpact2(b) => Some(common_from_pi2(&b.accounts)),
            _ => None,
        }?;

        extract_swap(ix, all_instructions, common)
    }
}

/// Subset of accounts every swap variant exposes — the bits we need
/// to populate a [`ChainEvent::Swap`].
struct SwapCommon {
    lb_pair: Pubkey,
    user: Pubkey,
    token_x_mint: Pubkey,
    token_y_mint: Pubkey,
}

fn common_from_v1(a: &SwapV1Accounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn common_from_v2(a: &Swap2Accounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn common_from_exact_out(a: &SwapExactOutAccounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn common_from_exact_out2(a: &SwapExactOut2Accounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn common_from_pi(a: &SwapWithPriceImpactAccounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn common_from_pi2(a: &SwapWithPriceImpact2Accounts) -> SwapCommon {
    SwapCommon {
        lb_pair: a.lb_pair,
        user: a.user,
        token_x_mint: a.token_x_mint,
        token_y_mint: a.token_y_mint,
    }
}

fn extract_swap(
    outer: &ParsedInstruction,
    all: &[ParsedInstruction],
    common: SwapCommon,
) -> Option<ChainEvent> {
    use borsh::BorshDeserialize;

    let body = find_event_body(outer, all, &SWAP_EVENT_DISCRIMINATOR)?;
    let event = match SwapEvent::try_from_slice(body) {
        Ok(ev) => ev,
        Err(e) => {
            warn!(?e, "dlmm: SwapEvent borsh decode failed");
            return None;
        }
    };

    // Sanity check: the event's lb_pair must agree with the outer
    // ix's pool. Mismatch = malformed firehose; bail rather than
    // emit a wrong attribution.
    let event_pool = Pubkey::new_from_array(event.lb_pair.to_bytes());
    if event_pool != common.lb_pair {
        warn!(
            event_pool = %event_pool,
            outer_pool = %common.lb_pair,
            "dlmm swap: event pool mismatches outer ix"
        );
        return None;
    }

    // `swap_for_y` from the event tells us which side is in / out:
    // true = token_x → token_y, false = token_y → token_x.
    let (token_in, token_out) = if event.swap_for_y {
        (common.token_x_mint, common.token_y_mint)
    } else {
        (common.token_y_mint, common.token_x_mint)
    };

    Some(ChainEvent::Swap(Swap {
        // No such argument on this protocol.
        track_volume: crate::protocols::OptionBool::None,
        instruction: crate::swap_instruction::resolve(&outer.program_id, &outer.data),
        protocol: Protocol::MeteoraDlmm,
        pool: common.lb_pair,
        trader: common.user,
        token_in,
        amount_in: event.amount_in,
        token_out,
        amount_out: event.amount_out,
        // DLMM fee is denominated in the input token.
        fee_amount: event.fee,
        fee_mint: token_in,
        // DLMM is the one protocol that publishes BOTH sides: price is a pure
        // function of the active bin index, and the event carries the bin the
        // swap started in and the one it ended in. No reserves are needed — a
        // bin-stepped price does not come from a reserve ratio.
        //
        // Reserves are therefore correctly absent, not missing: the bin ids
        // *are* the curve state. Pricing them needs the pair's `bin_step`
        // (`LbPair::price_at`), which lives in the cache, not the event.
        state_before: Some(CurveState::Bin(event.start_bin_id)),
        state_after: Some(CurveState::Bin(event.end_bin_id)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{ChainEvent, NoContext};
    use crate::parsing::ParsedInstructionBuilder;
    use solana_program::pubkey::Pubkey as ProgramPubkey;

    fn pk(b: u8) -> ProgramPubkey {
        ProgramPubkey::new_from_array([b; 32])
    }

    fn build_swap_outer_ix(
        instruction_idx: usize,
        accounts: Vec<ProgramPubkey>,
    ) -> ParsedInstruction {
        let mut data = super::super::instructions::swap::SWAP_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&[0u8; 16]); // amount_in + min_amount_out
        ParsedInstructionBuilder::new()
            .program_id(PROGRAM_ID)
            .accounts(accounts)
            .data(data)
            .instruction_index(instruction_idx)
            .build()
    }

    fn build_inner_event_ix(
        parent_idx: usize,
        instruction_idx: usize,
        event: &SwapEvent,
    ) -> ParsedInstruction {
        let mut data = ANCHOR_EVENT_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&SWAP_EVENT_DISCRIMINATOR);
        data.extend(borsh::to_vec(event).expect("borsh"));
        ParsedInstructionBuilder::new()
            .program_id(PROGRAM_ID)
            .accounts(vec![pk(0)])
            .data(data)
            .instruction_index(instruction_idx)
            .parent_index(parent_idx)
            .build()
    }

    #[test]
    fn swap_emits_chain_event() {
        // Build an outer Swap ix with 15 accounts (the SDK swap layout).
        let lb_pair = pk(1);
        let user = pk(11);
        let token_x = pk(7);
        let token_y = pk(8);

        let mut accts = vec![ProgramPubkey::default(); 15];
        accts[0] = lb_pair; // lb_pair
        accts[1] = PROGRAM_ID; // bin_array_bitmap_extension (sentinel)
        accts[2] = pk(3); // reserve_x
        accts[3] = pk(4); // reserve_y
        accts[4] = pk(5); // user_token_in
        accts[5] = pk(6); // user_token_out
        accts[6] = token_x; // token_x_mint
        accts[7] = token_y; // token_y_mint
        accts[8] = pk(9); // oracle
        accts[9] = PROGRAM_ID; // host_fee_in (sentinel)
        accts[10] = user; // user
        accts[11] = pk(12); // token_x_program
        accts[12] = pk(13); // token_y_program
        accts[13] = pk(14); // event_authority
        accts[14] = PROGRAM_ID; // program

        let outer = build_swap_outer_ix(0, accts);

        let event = SwapEvent {
            lb_pair: lb_pair.to_bytes().into(),
            from: user.to_bytes().into(),
            start_bin_id: -10,
            end_bin_id: -5,
            amount_in: 1_000_000_000,
            amount_out: 950_000_000,
            swap_for_y: true,
            fee: 5_000_000,
            protocol_fee: 1_000_000,
            fee_bps: 30,
            host_fee: 0,
        };
        let inner = build_inner_event_ix(0, 1, &event);

        let result = MeteoraDlmmExtractor::extract(&outer, &[outer.clone(), inner], &NoContext);
        let swap = match result {
            Some(ChainEvent::Swap(s)) => s,
            other => panic!("expected ChainEvent::Swap, got {other:?}"),
        };
        assert_eq!(swap.protocol, Protocol::MeteoraDlmm);
        assert_eq!(swap.pool, lb_pair);
        assert_eq!(swap.trader, user);
        // swap_for_y = true → token_x → token_y
        assert_eq!(swap.token_in, token_x);
        assert_eq!(swap.token_out, token_y);
        assert_eq!(swap.amount_in, 1_000_000_000);
        assert_eq!(swap.amount_out, 950_000_000);
        assert_eq!(swap.fee_amount, 5_000_000);
        // Both sides come straight out of the event — DLMM needs no derivation.
        assert_eq!(swap.state_before, Some(CurveState::Bin(event.start_bin_id)));
        assert_eq!(swap.state_after, Some(CurveState::Bin(event.end_bin_id)));
    }

    #[test]
    fn inner_event_ix_skipped_when_visited_directly() {
        // The extractor is dispatched on every ix matching the
        // program id — including inner event ixs. They must short-
        // circuit to None so each swap produces exactly one event.
        let event = SwapEvent {
            lb_pair: pk(1).to_bytes().into(),
            from: pk(2).to_bytes().into(),
            start_bin_id: 0,
            end_bin_id: 0,
            amount_in: 100,
            amount_out: 100,
            swap_for_y: true,
            fee: 0,
            protocol_fee: 0,
            fee_bps: 0,
            host_fee: 0,
        };
        let inner = build_inner_event_ix(0, 1, &event);
        let result =
            MeteoraDlmmExtractor::extract(&inner, std::slice::from_ref(&inner), &NoContext);
        assert!(result.is_none());
    }

    #[test]
    fn non_swap_ix_returns_none() {
        // ClaimFee parses fine but isn't a swap → no ChainEvent.
        let mut data = super::super::instructions::claim_fee::CLAIM_FEE_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&[0u8; 0]);
        let outer = ParsedInstructionBuilder::new()
            .program_id(PROGRAM_ID)
            .accounts(vec![pk(1); 14])
            .data(data)
            .instruction_index(0)
            .build();
        let result =
            MeteoraDlmmExtractor::extract(&outer, std::slice::from_ref(&outer), &NoContext);
        assert!(result.is_none());
    }
}
