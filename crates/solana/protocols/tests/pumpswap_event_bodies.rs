//! Decode PumpSwap event bodies the chain actually emitted.
//!
//! The previous conversion of these two events to borsh took pumpswap swaps to
//! zero and shipped green, because its tests serialized a synthetic body with
//! the same struct that decoded it. That proves the struct agrees with itself
//! and nothing else.
//!
//! These fixtures are raw bodies harvested from the live firehose
//! (`CAPTURE_EVENT_BODIES=<dir>` on the state node), one per observed length.
//! A struct that cannot read them is wrong no matter what a round-trip says.

use std::path::PathBuf;

use solana_protocols::parsing::event::ProtocolEvent;
use solana_protocols::pumpswap::events::{BuyEvent, SellEvent};

fn body(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/pumpswap/event_bodies")
        .join(format!("{name}.hex"));
    let hex =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    (0..hex.trim().len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Both observed `BuyEvent` lengths decode, and the variable-length `ix_name`
/// is what makes them differ — which is the field a fixed-offset reader could
/// never have handled.
#[test]
fn real_buy_bodies_decode() {
    let short = BuyEvent::from_event_body(&body("BuyEvent_457")).expect("457-byte body");
    assert_eq!(short.ix_name, "buy");

    let long = BuyEvent::from_event_body(&body("BuyEvent_472")).expect("472-byte body");
    assert_eq!(long.ix_name, "buy_exact_quote_in");

    // The two bodies differ by exactly the length of their instruction names.
    assert_eq!(
        472 - 457,
        long.ix_name.len() - short.ix_name.len(),
        "the whole size difference is the name"
    );

    for ev in [&short, &long] {
        assert!(ev.timestamp > 1_700_000_000, "a plausible unix timestamp");
        assert!(ev.base_amount_out > 0);
        assert!(ev.pool_base_token_reserves > 0);
    }
}

#[test]
fn a_real_sell_body_decodes() {
    let ev = SellEvent::from_event_body(&body("SellEvent_409")).expect("409-byte body");
    assert!(ev.timestamp > 1_700_000_000);
    assert_eq!(ev.base_amount_in, 1_407_158_964);
    assert_eq!(ev.lp_fee_basis_points, 25);
    assert_eq!(ev.protocol_fee_basis_points, 5);
}

/// The fee split the old 23-field table could not see at all.
///
/// `buyback_fee` is exactly half of `protocol_fee` at 5000bp, which is what
/// identified these fields in the first place: the arithmetic only works if the
/// offsets are right, so it doubles as a layout check.
#[test]
fn the_buyback_split_is_half_the_protocol_fee() {
    let ev = SellEvent::from_event_body(&body("SellEvent_409")).expect("decode");
    assert_eq!(ev.buyback_fee_basis_points, 5_000);
    assert_eq!(ev.protocol_fee, 374_812_397);
    assert_eq!(ev.buyback_fee, 187_406_198);
    assert_eq!(
        ev.buyback_fee,
        ev.protocol_fee * ev.buyback_fee_basis_points / 10_000,
        "5000bp of the protocol fee, floored"
    );
}

/// Every real body carries bytes past the last field *either* IDL declares.
///
/// This is the fact that broke the previous attempt: borsh refuses trailing
/// bytes, so a struct faithful to the IDL rejects every body the program sends.
/// If this assertion ever fails with zero, the IDL caught up and the tail field
/// can be replaced by the real ones.
#[test]
fn every_real_body_runs_past_the_published_idl() {
    let buy = BuyEvent::from_event_body(&body("BuyEvent_457")).expect("decode");
    let sell = SellEvent::from_event_body(&body("SellEvent_409")).expect("decode");
    assert_eq!(buy.undeclared_tail.len(), 25, "buy tail");
    assert_eq!(sell.undeclared_tail.len(), 25, "sell tail");

    // Not padding. The buy tails carry values, and their shape across samples
    // is u64, u64, bool, u64 - the bool reads 1 on both buys and 0 on the sell,
    // and the trailing u64 moves with the trade. Naming these would be a guess,
    // so they stay bytes; asserting they are non-zero is what stops the next
    // reader from writing them off as alignment slack and dropping them.
    assert!(
        buy.undeclared_tail.as_slice().iter().any(|b| *b != 0),
        "the buy tail carries data, not padding"
    );
    assert_eq!(
        buy.undeclared_tail.as_slice()[16],
        1,
        "byte 16 of the tail reads as a set flag on both captured buys"
    );
}

/// Re-serializing reproduces the body byte for byte, tail included. Without
/// this the fixtures pin decoding but not the encoding a builder would emit.
#[test]
fn bodies_round_trip_byte_for_byte() {
    for name in ["BuyEvent_457", "BuyEvent_472"] {
        let raw = body(name);
        let ev = BuyEvent::from_event_body(&raw).expect("decode");
        assert_eq!(borsh::to_vec(&ev).expect("serialize"), raw, "{name}");
    }
    let raw = body("SellEvent_409");
    let ev = SellEvent::from_event_body(&raw).expect("decode");
    assert_eq!(borsh::to_vec(&ev).expect("serialize"), raw);
}
