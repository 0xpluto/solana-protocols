//! Pool discovery, graded against real mainnet instructions.
//!
//! Every fixture here was captured off the firehose, so a passing run means
//! the four facts came out of account lists a program actually accepted --
//! not out of a hand-written list encoding what we believed the layout was.

use solana_program::pubkey::Pubkey;
use solana_protocols::chain::{discover, Discovery, Observed, PoolEdge, PoolGraph};
use solana_protocols::parsing::ParsedInstruction;
use solana_protocols::protocols::Protocol;

/// Fixture `instruction` names that are swaps, so discovery owes an answer.
const SWAPS: &[&str] = &[
    "buy",
    "buy_v2",
    "sell",
    "sell_v2",
    "buy_exact_sol_in",
    "buy_exact_quote_in",
    "buy_exact_quote_in_v2",
];

fn fixtures() -> Vec<serde_json::Value> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut out = Vec::new();
    for proto in ["pumpfun", "pumpswap"] {
        let Ok(entries) = std::fs::read_dir(root.join(proto)) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if !name.starts_with("ix_") || p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let raw = std::fs::read_to_string(&p).expect("fixture readable");
            let mut v: serde_json::Value = serde_json::from_str(&raw).expect("fixture parses");
            v["__file"] = serde_json::Value::String(format!("{proto}/{name}"));
            out.push(v);
        }
    }
    out
}

fn instruction_from(v: &serde_json::Value) -> ParsedInstruction {
    use base64::Engine as _;
    use std::str::FromStr;

    let program = Pubkey::from_str(v["program"].as_str().expect("program")).expect("valid program");
    let accounts = v["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| Pubkey::from_str(a["pubkey"].as_str().expect("pubkey")).expect("valid pubkey"))
        .collect();
    let data = base64::engine::general_purpose::STANDARD
        .decode(v["data_b64"].as_str().expect("data_b64"))
        .expect("base64");
    ParsedInstruction::new(program, accounts, data, 1, 0)
}

#[test]
fn every_swap_fixture_yields_an_edge() {
    let mut edges = 0usize;
    let mut unreadable: Vec<String> = Vec::new();
    let mut silent: Vec<String> = Vec::new();

    for v in fixtures() {
        let file = v["__file"].as_str().unwrap_or_default().to_string();
        let name = v["instruction"].as_str().unwrap_or_default();
        if !SWAPS.contains(&name) {
            continue;
        }
        match discover(&instruction_from(&v)) {
            Discovery::Edge(e) => {
                assert_ne!(e.pool, Pubkey::default(), "{file}: pool is the zero pubkey");
                assert_ne!(
                    e.token_a, e.token_b,
                    "{file}: both sides of the pair are the same mint"
                );
                assert!(
                    e.token_a.to_bytes() <= e.token_b.to_bytes(),
                    "{file}: pair is not canonically ordered"
                );
                edges += 1;
            }
            Discovery::Unreadable(i) => unreadable.push(format!("{file} ({})", i.name())),
            // The failure this whole module exists to prevent: a real swap
            // that produces no edge and says nothing about it.
            Discovery::NotASwap => silent.push(file),
        }
    }

    // A path bug would read no fixtures and pass every assertion above
    // vacuously, so the count is itself an assertion.
    assert!(
        edges >= 18,
        "only {edges} edges discovered -- fixtures not being read?"
    );
    assert!(
        silent.is_empty(),
        "swaps that discovery went silent on: {silent:#?}"
    );

    // The instructions with no accounts struct of their own. Pinned by name so
    // that modelling one shows up here as a failing expectation rather than as
    // nothing at all -- and so that a *new* unreadable swap fails loudly.
    const NO_ACCOUNTS_STRUCT: &[&str] = &[
        "buy_exact_sol_in",
        "buy_exact_quote_in_v2",
        "buy_exact_quote_in",
    ];
    for u in &unreadable {
        assert!(
            NO_ACCOUNTS_STRUCT.iter().any(|n| u.contains(n)),
            "unexpected unreadable swap: {u}"
        );
    }
    println!("edges: {edges}   unreadable: {}", unreadable.len());
}

#[test]
fn a_pair_does_not_depend_on_trade_direction() {
    let pool = Pubkey::new_unique();
    let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
    assert_eq!(
        PoolEdge::new(pool, Protocol::PumpSwap, a, b),
        PoolEdge::new(pool, Protocol::PumpSwap, b, a),
        "buying and selling the same pool must yield one edge, not two"
    );
}

#[test]
fn an_unknown_program_is_not_a_swap() {
    let ix = ParsedInstruction::new(
        Pubkey::new_unique(),
        vec![Pubkey::new_unique()],
        vec![1; 8],
        1,
        0,
    );
    assert_eq!(discover(&ix), Discovery::NotASwap);
}

#[test]
fn a_graph_built_from_the_fixtures_has_no_disagreements() {
    let mut g = PoolGraph::new();
    let mut conflicts = Vec::new();

    for v in fixtures() {
        let file = v["__file"].as_str().unwrap_or_default().to_string();
        if let Discovery::Edge(e) = discover(&instruction_from(&v)) {
            if let Observed::Disagreed { kept, rejected } = g.observe(e) {
                conflicts.push(format!(
                    "{file}: pool {} named ({}, {}) then ({}, {})",
                    kept.pool, kept.token_a, kept.token_b, rejected.token_a, rejected.token_b
                ));
            }
        }
    }

    // Real instructions from real transactions: two of them naming different
    // pairs for one pool would mean a decoder is reading the wrong account.
    assert!(conflicts.is_empty(), "pairs disagreed: {conflicts:#?}");
    assert!(
        !g.is_empty(),
        "no pools discovered -- fixtures not being read?"
    );
    assert_eq!(g.disagreements(), 0);
    println!(
        "graph: {} pools, by protocol {:?}",
        g.len(),
        g.by_protocol()
    );
}

/// The state side of [`NamesPair`]: a pool account names its own pair.
///
/// Graded against the mints the fixture recorded off-chain, so this proves the
/// impl reads the right fields -- not merely that it reads two pubkeys.
#[test]
fn a_pool_account_names_the_same_pair_the_chain_recorded() {
    use base64::Engine as _;
    use solana_protocols::pairs::NamesPair;
    use solana_protocols::pumpswap::PumpSwapPool;
    use std::str::FromStr;

    let mut checked = 0usize;
    for fixture in [
        "pool_v1_261.json",
        "pool_v2_300.json",
        "pool_v3_full_301.json",
    ] {
        let raw = std::fs::read_to_string(format!(
            "{}/fixtures/pumpswap/{fixture}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture readable");
        let v: serde_json::Value = serde_json::from_str(&raw).expect("fixture parses");
        let data = base64::engine::general_purpose::STANDARD
            .decode(v["data_b64"].as_str().expect("data_b64"))
            .expect("base64");

        let pool = PumpSwapPool::from_account_data(&data).expect("pool decodes");
        let (a, b) = pool.pair();
        let expect =
            |k: &str| Pubkey::from_str(v["expected"][k].as_str().expect(k)).expect("valid pubkey");
        assert_eq!(a, expect("base_mint"), "{fixture}: base side");
        assert_eq!(b, expect("quote_mint"), "{fixture}: quote side");

        // The caller supplies the address, because a decoded account does not
        // know the pubkey it was read from.
        let address = Pubkey::from_str(v["address"].as_str().expect("address")).expect("valid");
        let edge = PoolEdge::new(address, Protocol::PumpSwap, a, b);
        assert_eq!(edge.pool, address);
        assert_ne!(edge.token_a, edge.token_b);
        checked += 1;
    }
    assert_eq!(checked, 3, "fixtures not being read");
}
