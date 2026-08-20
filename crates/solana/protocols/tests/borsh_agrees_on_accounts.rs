//! borsh and the generated offset walk must decode real accounts identically.
//!
//! Scaffolding for the account half of the borsh migration, and the same
//! argument as its instruction-side twin: the migration is only a refactor if
//! the two agree *today*. Delete this once `from_account_data` is borsh, rather
//! than leave it asserting that borsh equals itself.
//!
//! The fixtures are live mainnet accounts, several of them padded — PumpSwap
//! pools at 261, 300 and 301 bytes over one 244-byte field span — which is
//! exactly the case that forces a prefix read rather than `try_from_slice`.

use borsh::BorshDeserialize;
use solana_protocols::parsing::state::OnchainState;

fn account(fixture: &str) -> Vec<u8> {
    let raw = std::fs::read_to_string(format!("{}/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR")))
        .expect("fixture");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(v["data_b64"].as_str().expect("data_b64"))
        .expect("base64")
}

/// Decode both ways and compare. Equality is by `Debug`, because these types do
/// not all implement `PartialEq` and the point is that every field agrees.
macro_rules! agree {
    ($ty:ty, $fixture:literal) => {{
        let data = account($fixture);
        let walked = <$ty as OnchainState>::from_account_data(&data).unwrap_or_else(|e| {
            panic!(
                "{} offset walk failed on {}: {e}",
                stringify!($ty),
                $fixture
            )
        });
        let mut cursor = &data[8..];
        let borshed = <$ty as BorshDeserialize>::deserialize(&mut cursor).unwrap_or_else(|e| {
            panic!(
                "{} borsh failed on {} ({} bytes): {e}",
                stringify!($ty),
                $fixture,
                data.len()
            )
        });
        assert_eq!(
            format!("{walked:?}"),
            format!("{borshed:?}"),
            "{} decodes differently under borsh than under the offset walk",
            stringify!($ty)
        );
    }};
}

#[test]
fn pumpswap_pool_agrees_at_every_observed_size() {
    // Three allocations, one field span. The padded ones are why accounts get a
    // prefix read.
    agree!(
        solana_protocols::pumpswap::PumpSwapPool,
        "pumpswap/pool_v1_261.json"
    );
    agree!(
        solana_protocols::pumpswap::PumpSwapPool,
        "pumpswap/pool_v2_300.json"
    );
    agree!(
        solana_protocols::pumpswap::PumpSwapPool,
        "pumpswap/pool_v3_full_301.json"
    );
}

#[test]
fn raydium_pools_agree() {
    agree!(
        solana_protocols::raydium_cpmm::CpmmPoolState,
        "raydium_cpmm/pool_account.json"
    );
    agree!(
        solana_protocols::raydium_clmm::PoolState,
        "raydium_clmm/pool_account.json"
    );
    agree!(
        solana_protocols::raydium_launchpad::LaunchpadPoolState,
        "raydium_launchpad/pool_account.json"
    );
}

#[test]
fn meteora_dbc_virtual_pool_agrees() {
    agree!(
        solana_protocols::meteora_dbc::VirtualPool,
        "meteora_dbc/pool_account.json"
    );
}

/// The version-added case, which is the one borsh cannot see: `Present` vs
/// `Absent` is decided by account length, and padding is indistinguishable from
/// data. Both mechanisms answer "are there bytes there" and agree here — that
/// equivalence is what makes the migration safe, and it is worth pinning
/// because it is the least obvious claim in it.
#[test]
fn pumpfun_bonding_curve_agrees_including_its_legacy_fields() {
    agree!(
        solana_protocols::pumpfun::BondingCurve,
        "pumpfun/bonding_curve_150.json"
    );
}
