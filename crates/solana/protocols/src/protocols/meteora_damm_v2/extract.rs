//! Meteora DAMM v2 extractor.
//!
//! Dispatches on DAMM v2 program_id. Each tx typically produces two
//! DAMM v2 dispatches — the outer `swap` ix and the inner
//! event-authority self-CPI carrying `EvtSwap2`. We emit a single
//! [`Swap`] off the inner event ix (where the payload lives) and
//! return `None` for the outer swap ix. That keeps one chain event
//! per on-chain swap regardless of how the log parser attributes
//! things.
//!
//! # Swap mapping
//!
//! DAMM v2 pools are generic A/B pairs. The [`Swap`] type is
//! token-agnostic — we map `token_a`/`token_b` straight onto
//! `token_in`/`token_out` based on `EvtSwap2.trade_direction`. No
//! special-casing of WSOL; stablecoin/stablecoin and token/token
//! pairs extract the same way. The trader is `SwapAccounts.payer` on
//! the outer ix.

use solana_program::pubkey::Pubkey;
use tracing::{trace, warn};

use super::constants::{
    ANCHOR_EVENT_DISCRIMINATOR, EVT_SWAP2_DISCRIMINATOR, PROGRAM_ID as DAMM_V2_PROGRAM,
};
use super::events::EvtSwap2;
use super::instructions::SwapAccounts;
use crate::chain::{ChainEvent, CurveState, ExtractContext, ProtocolExtractor, Swap};
use crate::parsing::{FromAccountKeys, ParsedInstruction};
use crate::protocols::Protocol;

/// Zero-sized adapter. Register via
/// [`ExtractorRegistry::register`](crate::chain::ExtractorRegistry::register).
pub struct MeteoraDammV2Extractor;

impl ProtocolExtractor for MeteoraDammV2Extractor {
    fn program_id() -> Pubkey {
        DAMM_V2_PROGRAM
    }

    fn extract(
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        _ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        // We only emit on the inner event self-CPI — its ix.data
        // starts with the Anchor event envelope. The outer swap ix
        // (starts with SWAP_DISCRIMINATOR) produces nothing; the
        // event self-CPI carries the full post-swap state.
        if ix.data.len() < 16 || ix.data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
            trace!("damm-v2: not an Anchor event ix, skipping");
            return None;
        }
        if ix.data[8..16] != EVT_SWAP2_DISCRIMINATOR {
            // Some future non-swap event — skip without warning.
            trace!("damm-v2: Anchor event but not EvtSwap2, skipping");
            return None;
        }

        let event = match EvtSwap2::from_self_cpi_data(&ix.data) {
            Some(e) => e,
            None => {
                // from_self_cpi_data already warns on malformed bodies.
                return None;
            }
        };

        // Walk up to the outer swap ix for mint / trader context. The
        // inner event ix's own accounts only carry event-authority +
        // program, neither of which helps.
        let outer = resolve_outer_swap(ix, all_instructions)?;
        let outer_accounts = match SwapAccounts::from_account_keys(&outer.accounts) {
            Ok(a) => a,
            Err(e) => {
                warn!(?e, "damm-v2: outer swap accounts parse failed");
                return None;
            }
        };

        // Cross-check: the pool in the event must match the pool on
        // the outer instruction. Mismatch = wrong parent or tampered
        // log.
        if event.pool != outer_accounts.pool {
            warn!(
                event_pool = %event.pool,
                outer_pool = %outer_accounts.pool,
                "damm-v2: EvtSwap2 pool mismatches outer ix pool"
            );
            return None;
        }

        // Map A/B → token_in/token_out using only the event's
        // trade_direction. The result is pair-agnostic — SOL/token,
        // stable/stable, token/token all work the same way.
        let (token_in, token_out, reserve_in, reserve_out) = if event.is_a_to_b() {
            (
                outer_accounts.token_a_mint,
                outer_accounts.token_b_mint,
                event.reserve_a_amount,
                event.reserve_b_amount,
            )
        } else {
            (
                outer_accounts.token_b_mint,
                outer_accounts.token_a_mint,
                event.reserve_b_amount,
                event.reserve_a_amount,
            )
        };

        Some(ChainEvent::Swap(Swap {
            // Not a bonding-curve protocol.
            completed_curve: false,
            // No such argument on this protocol.
            track_volume: crate::protocols::OptionBool::None,
            instruction: crate::swap_instruction::resolve(&outer.program_id, &outer.data),
            protocol: Protocol::MeteoraDammV2,
            pool: outer_accounts.pool,
            trader: outer_accounts.payer,
            token_in,
            amount_in: event.input_amount_gross(),
            token_out,
            amount_out: event.output_amount(),
            // V1: total fee is in the event but its denomination
            // depends on `collect_fee_mode` (BothToken vs OnlyB).
            // Leave at 0 until a consumer asks; fee_mint defaults to
            // token_in as a safe placeholder.
            fee_amount: 0,
            fee_mint: token_in,
            state_before: None,
            state_after: Some(CurveState::Reserves {
                in_side: reserve_in,
                out_side: reserve_out,
            }),
        }))
    }
}

/// Walk up via `parent_index` until we find the outer DAMM v2 swap ix
/// this event belongs to. Stops as soon as it finds a DAMM v2
/// instruction whose data starts with a non-event prefix — i.e., the
/// real `swap` ix. Returns `None` if no ancestor qualifies.
fn resolve_outer_swap<'a>(
    event_ix: &ParsedInstruction,
    all: &'a [ParsedInstruction],
) -> Option<&'a ParsedInstruction> {
    let mut idx = event_ix.parent_index?;
    loop {
        let candidate = all.get(idx)?;
        if candidate.program_id == DAMM_V2_PROGRAM
            && candidate.data.len() >= 8
            && candidate.data[..8] != ANCHOR_EVENT_DISCRIMINATOR
        {
            return Some(candidate);
        }
        idx = candidate.parent_index?;
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::NoContext;
    use crate::parsing::ParsedInstructionBuilder;
    use borsh::BorshSerialize;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    /// Build a 14-slot `SwapAccounts` layout. Only pool / token_a_mint /
    /// token_b_mint / payer matter for extraction; other slots are
    /// placeholders to satisfy `from_account_keys`.
    fn swap_accounts(
        pool: Pubkey,
        token_a_mint: Pubkey,
        token_b_mint: Pubkey,
        payer: Pubkey,
    ) -> Vec<Pubkey> {
        vec![
            pk(0x01),     // 0  pool_authority
            pool,         // 1  pool
            pk(0x03),     // 2  input_token_account
            pk(0x04),     // 3  output_token_account
            pk(0x05),     // 4  token_a_vault
            pk(0x06),     // 5  token_b_vault
            token_a_mint, // 6  token_a_mint
            token_b_mint, // 7  token_b_mint
            payer,        // 8  payer
            pk(0x0A),     // 9  token_a_program
            pk(0x0B),     // 10 token_b_program
            pk(0x0C),     // 11 referral_token_account
            pk(0x0D),     // 12 event_authority
            pk(0x0E),     // 13 program
        ]
    }

    // Encoders mirroring events::tests but private to this module — we
    // need them to assemble a full self-CPI ix.data blob without
    // exposing BorshSerialize on the production structs.
    #[derive(BorshSerialize)]
    struct SwapParameters2Ser {
        amount_0: u64,
        amount_1: u64,
        swap_mode: u8,
    }
    #[derive(BorshSerialize)]
    struct SwapResult2Ser {
        included_fee_input_amount: u64,
        excluded_fee_input_amount: u64,
        amount_left: u64,
        output_amount: u64,
        next_sqrt_price: u128,
        trading_fee: u64,
        protocol_fee: u64,
        partner_fee: u64,
        referral_fee: u64,
    }
    #[derive(BorshSerialize)]
    struct EvtSwap2Ser {
        pool: [u8; 32],
        trade_direction: u8,
        collect_fee_mode: u8,
        has_referral: bool,
        params: SwapParameters2Ser,
        swap_result: SwapResult2Ser,
        included_transfer_fee_amount_in: u64,
        included_transfer_fee_amount_out: u64,
        excluded_transfer_fee_amount_out: u64,
        current_timestamp: u64,
        reserve_a_amount: u64,
        reserve_b_amount: u64,
    }

    fn event_ix_data(
        pool: Pubkey,
        trade_direction: u8,
        input_gross: u64,
        output: u64,
        reserve_a: u64,
        reserve_b: u64,
    ) -> Vec<u8> {
        let body = EvtSwap2Ser {
            pool: pool.to_bytes(),
            trade_direction,
            collect_fee_mode: 0,
            has_referral: false,
            params: SwapParameters2Ser {
                amount_0: input_gross,
                amount_1: output,
                swap_mode: 0,
            },
            swap_result: SwapResult2Ser {
                included_fee_input_amount: input_gross,
                excluded_fee_input_amount: input_gross,
                amount_left: 0,
                output_amount: output,
                next_sqrt_price: 0,
                trading_fee: 0,
                protocol_fee: 0,
                partner_fee: 0,
                referral_fee: 0,
            },
            included_transfer_fee_amount_in: 0,
            included_transfer_fee_amount_out: 0,
            excluded_transfer_fee_amount_out: output,
            current_timestamp: 1_700_000_000,
            reserve_a_amount: reserve_a,
            reserve_b_amount: reserve_b,
        };
        let borsh_body = borsh::to_vec(&body).unwrap();
        let mut data = Vec::with_capacity(16 + borsh_body.len());
        data.extend_from_slice(&ANCHOR_EVENT_DISCRIMINATOR);
        data.extend_from_slice(&EVT_SWAP2_DISCRIMINATOR);
        data.extend_from_slice(&borsh_body);
        data
    }

    /// Outer swap ix + inner event self-CPI, laid out the way the
    /// parser would flatten them (outer at idx=0, inner at idx=1 with
    /// parent_index=Some(0)).
    #[allow(clippy::too_many_arguments)]
    fn tx_with_event(
        pool: Pubkey,
        token_a_mint: Pubkey,
        token_b_mint: Pubkey,
        payer: Pubkey,
        trade_direction: u8,
        input_gross: u64,
        output: u64,
        reserve_a: u64,
        reserve_b: u64,
    ) -> Vec<ParsedInstruction> {
        use crate::protocols::meteora_damm_v2::SWAP_DISCRIMINATOR;
        let mut outer_data = Vec::with_capacity(8 + 16);
        outer_data.extend_from_slice(&SWAP_DISCRIMINATOR);
        outer_data.extend_from_slice(&input_gross.to_le_bytes());
        outer_data.extend_from_slice(&output.to_le_bytes());

        let outer = ParsedInstructionBuilder::new()
            .program_id(DAMM_V2_PROGRAM)
            .accounts(swap_accounts(pool, token_a_mint, token_b_mint, payer))
            .data(outer_data)
            .stack_height(1)
            .instruction_index(0)
            .build();

        let mut inner = ParsedInstructionBuilder::new()
            .program_id(DAMM_V2_PROGRAM)
            .accounts(vec![pk(0x0D), pk(0x0E)]) // event_authority, program — unused
            .data(event_ix_data(
                pool,
                trade_direction,
                input_gross,
                output,
                reserve_a,
                reserve_b,
            ))
            .stack_height(2)
            .instruction_index(1)
            .build();
        inner.parent_index = Some(0);

        vec![outer, inner]
    }

    #[test]
    fn a_to_b_swap_maps_a_as_token_in() {
        // trade_direction = 0 (A→B): user paid token_a, received token_b.
        let pool = pk(0xAA);
        let token_a = pk(0xBB);
        let token_b = pk(0xCC);
        let trader = pk(0xDD);
        let ixs = tx_with_event(
            pool,
            token_a,
            token_b,
            trader,
            0,
            1_000_000_000,
            33_000_000,
            31_000_000_000,
            900_000_000,
        );

        let result =
            MeteoraDammV2Extractor::extract(&ixs[1], &ixs, &NoContext).expect("expected Swap");
        let swap = match result {
            ChainEvent::Swap(s) => s,
            _ => panic!("expected Swap variant"),
        };
        assert_eq!(swap.protocol, Protocol::MeteoraDammV2);
        assert_eq!(swap.pool, pool);
        assert_eq!(swap.trader, trader);
        assert_eq!(swap.token_in, token_a);
        assert_eq!(swap.token_out, token_b);
        assert_eq!(swap.amount_in, 1_000_000_000);
        assert_eq!(swap.amount_out, 33_000_000);
        // in_side tracks token_in's side, out_side tracks token_out's.
        assert_eq!(swap.state_before, None);
        assert_eq!(
            swap.state_after,
            Some(CurveState::Reserves {
                in_side: 31_000_000_000,
                out_side: 900_000_000
            })
        );
    }

    #[test]
    fn b_to_a_swap_maps_b_as_token_in() {
        // trade_direction = 1 (B→A): user paid token_b, received token_a.
        let pool = pk(0xAA);
        let token_a = pk(0xBB);
        let token_b = pk(0xCC);
        let trader = pk(0xDD);
        let ixs = tx_with_event(
            pool,
            token_a,
            token_b,
            trader,
            1,
            33_000_000,
            990_000_000,
            32_000_000_000,
            870_000_000,
        );

        let result =
            MeteoraDammV2Extractor::extract(&ixs[1], &ixs, &NoContext).expect("expected Swap");
        let swap = match result {
            ChainEvent::Swap(s) => s,
            _ => panic!("expected Swap variant"),
        };
        assert_eq!(swap.token_in, token_b);
        assert_eq!(swap.token_out, token_a);
        assert_eq!(swap.amount_in, 33_000_000);
        assert_eq!(swap.amount_out, 990_000_000);
        // in_side is reserve_b (B is now token_in), out_side is reserve_a.
        assert_eq!(
            swap.state_after,
            Some(CurveState::Reserves {
                in_side: 870_000_000,
                out_side: 32_000_000_000
            })
        );
    }

    #[test]
    fn non_sol_pairs_are_extracted() {
        // Token-agnostic: stablecoin/stablecoin, token/token — all
        // extract the same way. The Swap type doesn't care which side
        // (if any) is SOL.
        let pool = pk(0xAA);
        let usdc = pk(0xBB);
        let usdt = pk(0xCC);
        let trader = pk(0xDD);
        let ixs = tx_with_event(
            pool,
            usdc,
            usdt,
            trader,
            0,
            100_000_000,
            99_990_000,
            500_000_000_000,
            499_900_000_000,
        );
        let result = MeteoraDammV2Extractor::extract(&ixs[1], &ixs, &NoContext)
            .expect("non-SOL pair should extract");
        let swap = match result {
            ChainEvent::Swap(s) => s,
            _ => panic!("expected Swap"),
        };
        assert_eq!(swap.token_in, usdc);
        assert_eq!(swap.token_out, usdt);
    }

    #[test]
    fn outer_swap_ix_produces_no_event() {
        // Firing on the outer ix (idx=0) — data starts with
        // SWAP_DISCRIMINATOR, not the Anchor event envelope. Return
        // None cleanly.
        let pool = pk(0xAA);
        let ixs = tx_with_event(pool, pk(0xBB), pk(0xCC), pk(0xDD), 0, 1, 1, 0, 0);

        assert!(
            MeteoraDammV2Extractor::extract(&ixs[0], &ixs, &NoContext).is_none(),
            "outer swap ix must not emit an event"
        );
    }

    #[test]
    fn mismatched_pool_is_rejected() {
        // Outer ix references pool P1, but the event body encodes
        // pool P2. That's an invariant violation — skip with a warn.
        let outer_pool = pk(0xAA);
        let token_a = pk(0xBB);
        let token_b = pk(0xCC);
        let trader = pk(0xDD);
        let mut ixs = tx_with_event(
            outer_pool,
            token_a,
            token_b,
            trader,
            0,
            1_000_000_000,
            33_000_000,
            0,
            0,
        );
        // Rebuild inner with a different pool in the event body.
        let inner_data = event_ix_data(pk(0x99), 0, 1_000_000_000, 33_000_000, 0, 0);
        ixs[1].data = inner_data;

        assert!(MeteoraDammV2Extractor::extract(&ixs[1], &ixs, &NoContext).is_none());
    }

    #[test]
    fn event_without_outer_swap_returns_none() {
        // Standalone event ix with no parent_index — nothing to walk
        // up to. Should skip gracefully.
        let pool = pk(0xAA);
        let data = event_ix_data(pool, 0, 1, 1, 0, 0);
        let orphan = ParsedInstructionBuilder::new()
            .program_id(DAMM_V2_PROGRAM)
            .accounts(vec![])
            .data(data)
            .stack_height(2)
            .instruction_index(0)
            .build();

        assert!(MeteoraDammV2Extractor::extract(
            &orphan,
            std::slice::from_ref(&orphan),
            &NoContext
        )
        .is_none());
    }
}
