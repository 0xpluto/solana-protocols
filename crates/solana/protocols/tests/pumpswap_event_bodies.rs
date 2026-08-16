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

/// The 25 bytes past the IDL are now named fields, so the tail behind them is
/// empty — and this is the assertion that tells us when the program grows
/// again. If it fails with a non-zero length, pump added something new; the
/// tail is what keeps decoding working meanwhile.
#[test]
fn the_named_undeclared_fields_account_for_the_whole_body() {
    let buy = BuyEvent::from_event_body(&body("BuyEvent_457")).expect("decode");
    let sell = SellEvent::from_event_body(&body("SellEvent_409")).expect("decode");
    assert!(
        buy.undeclared_tail.is_empty() && sell.undeclared_tail.is_empty(),
        "buy tail {} / sell tail {} — the program emitted more than we model",
        buy.undeclared_tail.len(),
        sell.undeclared_tail.len()
    );
}

/// The undeclared block carries data, not padding — which is why it is modelled
/// rather than skipped.
///
/// `undeclared_flag` is typed `bool` deliberately: borsh refuses any byte
/// outside {0, 1}, so if the 8/8/1/8 split is wrong, the next real body fails
/// loudly instead of yielding plausible garbage.
#[test]
fn the_undeclared_block_is_data() {
    let buy = BuyEvent::from_event_body(&body("BuyEvent_457")).expect("decode");
    assert_eq!(buy.undeclared_0, 17_584_505_290);
    assert_eq!(buy.undeclared_1, 0);
    assert!(buy.undeclared_flag, "set on both captured buys");
    assert!(buy.undeclared_2 > 0, "moves with the trade");

    let sell = SellEvent::from_event_body(&body("SellEvent_409")).expect("decode");
    assert!(!sell.undeclared_flag, "clear on the captured sell");
}

/// The two fields the *vendored* IDL was missing until it was refreshed from
/// chain. Kept as a test because they are checkable: the buyback is 5000bp of
/// the protocol fee, so the arithmetic only works at the right offsets.
#[test]
fn buyback_is_declared_and_checks_out() {
    let ev = SellEvent::from_event_body(&body("SellEvent_409")).expect("decode");
    assert_eq!(ev.buyback_fee_basis_points, 5_000);
    assert_eq!(
        ev.buyback_fee,
        ev.protocol_fee * ev.buyback_fee_basis_points / 10_000
    );
    // These are IDL-declared, so the layout derive checks them and they are
    // *not* exempt. Count is the guard: if someone marks one undeclared to make
    // a build pass, this drops.
    assert_eq!(
        SellEvent::UNDECLARED_FIELDS,
        5,
        "four unknown fields plus the growth tail"
    );
    assert_eq!(SellEvent::IDL_DECLARED_FIELDS, 27);
    assert_eq!(BuyEvent::IDL_DECLARED_FIELDS, 34);
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
