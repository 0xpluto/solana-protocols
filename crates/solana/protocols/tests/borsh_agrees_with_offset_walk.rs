//! borsh and the generated offset walk must decode identically.
//!
//! The crate is migrating instruction arguments onto borsh, on the grounds that
//! Solana programs serialize with borsh and anything else is a second
//! implementation of the producer's codec. That argument is only worth acting on
//! if the two agree *today* — otherwise the migration is not a refactor, it is a
//! behaviour change wearing one.
//!
//! So this decodes real mainnet instruction data both ways and compares. It is
//! scaffolding for the migration: once `from_instruction_data` is borsh, the
//! comparison becomes tautological and this file should be deleted rather than
//! left to assert that borsh equals itself.

use borsh::BorshDeserialize;
use solana_protocols::parsing::FromInstructionData;

/// Instruction data from a landed mainnet instruction, discriminator stripped.
fn args(fixture: &str, disc_len: usize) -> Vec<u8> {
    let raw = std::fs::read_to_string(format!("{}/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR")))
        .expect("fixture");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let hex = v["data"].as_str().or_else(|| v["data_hex"].as_str());
    let bytes: Vec<u8> = match hex {
        Some(h) => (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
            .collect(),
        None => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(v["data_b64"].as_str().expect("data_b64 or data"))
                .expect("base64")
        }
    };
    bytes[disc_len..].to_vec()
}

/// Compare both decoders on the same bytes.
macro_rules! agree {
    ($ty:ty, $fixture:literal) => {{
        let data = args($fixture, 8);
        let walked = <$ty>::from_instruction_data(&data).unwrap_or_else(|e| {
            panic!(
                "{} offset walk failed on {}: {e}",
                stringify!($ty),
                $fixture
            )
        });
        let borshed = <$ty>::try_from_slice(&data).unwrap_or_else(|e| {
            panic!(
                "{} borsh failed on {} ({} bytes): {e}",
                stringify!($ty),
                $fixture,
                data.len()
            )
        });
        assert_eq!(
            borsh::to_vec(&walked).expect("reserialize"),
            borsh::to_vec(&borshed).expect("reserialize"),
            "{} decodes differently under borsh than under the offset walk",
            stringify!($ty)
        );
    }};
}

#[test]
fn pumpfun_buy_decodes_the_same_both_ways() {
    agree!(solana_protocols::pumpfun::BuyParams, "pumpfun/ix_buy.json");
}

#[test]
fn pumpfun_sell_decodes_the_same_both_ways() {
    agree!(
        solana_protocols::pumpfun::SellParams,
        "pumpfun/ix_sell.json"
    );
}

#[test]
fn pumpswap_buy_decodes_the_same_both_ways() {
    agree!(
        solana_protocols::pumpswap::BuyParams,
        "pumpswap/ix_buy.json"
    );
}

#[test]
fn pumpswap_sell_decodes_the_same_both_ways() {
    agree!(
        solana_protocols::pumpswap::SellParams,
        "pumpswap/ix_sell.json"
    );
}
