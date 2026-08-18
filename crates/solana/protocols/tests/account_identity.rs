//! Discriminators collide across programs, so decoding must check identity.
//!
//! `account:PoolState` is shared by Raydium CPMM, CLMM and Launchpad — three
//! different layouts behind eight identical bytes. Before these types moved onto
//! `OnchainState` none of them checked the discriminator at all, so the only
//! thing standing between a caller and a silently wrong decode was passing the
//! right account in the first place.
//!
//! This asserts the check exists, using real mainnet bytes from each program.

use solana_protocols::meteora_dbc::VirtualPool;
#[allow(unused_imports)]
use solana_protocols::parsing::state::OnchainState;
use solana_protocols::raydium_cpmm::CpmmPoolState;

fn fixture(p: &str) -> Vec<u8> {
    let s = std::fs::read_to_string(format!("{}/fixtures/{p}", env!("CARGO_MANIFEST_DIR")))
        .expect("fixture");
    let v: serde_json::Value = serde_json::from_str(&s).expect("json");
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(v["data_b64"].as_str().expect("data_b64"))
        .expect("base64")
}

/// A different program's account is refused, even though its first eight bytes
/// are byte-identical: `meteora_dbc` uses `account:VirtualPool`, so it differs
/// here — the interesting case is that the *check runs at all*.
#[test]
fn a_foreign_account_is_refused() {
    let dbc = fixture("meteora_dbc/pool_account.json");
    assert!(
        CpmmPoolState::from_account_data(&dbc).is_err(),
        "a VirtualPool must not decode as a CPMM pool"
    );
    let cpmm = fixture("raydium_cpmm/pool_account.json");
    assert!(
        VirtualPool::from_account_data(&cpmm).is_err(),
        "a CPMM pool must not decode as a VirtualPool"
    );
}

/// The colliding case, stated explicitly so nobody "simplifies" identity down to
/// the discriminator: CPMM and Launchpad pools share all eight bytes, so the
/// discriminator alone cannot tell them apart. Only the owning program can, and
/// that check lives in the registry — not here.
#[test]
fn the_collision_is_real_and_documented() {
    let cpmm = fixture("raydium_cpmm/pool_account.json");
    let launchpad = fixture("raydium_launchpad/pool_account.json");
    assert_eq!(
        cpmm[..8],
        launchpad[..8],
        "these two programs really do share account:PoolState"
    );
}
