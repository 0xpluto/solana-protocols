//! One-shot slot identifier for the pumpfun v2 swap layouts.
//!
//! Names each account index by *derivation* against 1,152 real instructions
//! captured from the firehose, rather than by reading an IDL we do not have.
//! Run: `cargo test -p solana-protocols --test v2_slots -- --nocapture`
use solana_program::pubkey::Pubkey;
use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(serde::Deserialize)]
struct Rec {
    ix: String,
    n: usize,
    mint: String,
    user: String,
    accounts: Vec<String>,
}

fn bc(mint: &Pubkey, pf: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], pf).0
}
fn ata(owner: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), tp.as_ref(), mint.as_ref()],
        &spl_associated_token_account::id(),
    )
    .0
}

#[test]
fn identify_v2_slots() {
    let Ok(txt) = std::fs::read_to_string("fixtures/pumpfun/v2recs.json") else {
        eprintln!("no capture present — skipping");
        return;
    };
    let recs: Vec<Rec> = serde_json::from_str(&txt).expect("parse");
    use solana_protocols::protocols::pumpfun::*;
    let pf = PROGRAM_ID;

    for target in ["buy_v2", "buy_exact_quote_in_v2", "sell_v2"] {
        let rs: Vec<&Rec> = recs.iter().filter(|r| r.ix == target).collect();
        if rs.is_empty() {
            continue;
        }
        let min_n = rs.iter().map(|r| r.n).min().unwrap();
        println!(
            "\n### {target}  ({} samples, {min_n} required slots)",
            rs.len()
        );
        for i in 0..min_n {
            let mut roles: BTreeMap<&str, usize> = BTreeMap::new();
            for r in &rs {
                let a = Pubkey::from_str(&r.accounts[i]).unwrap();
                let mint = Pubkey::from_str(&r.mint).unwrap();
                let user = Pubkey::from_str(&r.user).unwrap();
                let role = if a == mint {
                    "mint"
                } else if a == user {
                    "user"
                } else if a == GLOBAL_PDA {
                    "GLOBAL_PDA"
                } else if a == FEE_CONFIG_PDA {
                    "FEE_CONFIG_PDA"
                } else if a == EVENT_AUTHORITY_PDA {
                    "EVENT_AUTHORITY_PDA"
                } else if a == GLOBAL_VOLUME_ACCUMULATOR_PDA {
                    "GLOBAL_VOL_ACC"
                } else if a == PUMP_FEES_PROGRAM_ID {
                    "PUMP_FEES_PROGRAM"
                } else if a == pf {
                    "pumpfun program"
                } else if a == solana_program::system_program::id() {
                    "system"
                } else if a == spl_token::id() {
                    "spl_token"
                } else if a == spl_token_2022::id() {
                    "token_2022"
                } else if a == spl_associated_token_account::id() {
                    "ata_program"
                } else if a == solana_protocols::tokens::WSOL {
                    "WSOL"
                } else if a
                    == Pubkey::find_program_address(&[BONDING_CURVE_SEED, mint.as_ref()], &pf).0
                {
                    "bonding_curve PDA"
                } else if a
                    == Pubkey::find_program_address(
                        &[USER_VOLUME_ACCUMULATOR_SEED, user.as_ref()],
                        &pf,
                    )
                    .0
                {
                    "user_vol_acc PDA"
                } else if a == ata(&bc(&mint, &pf), &mint, &spl_token_2022::id()) {
                    "assoc_bonding_curve(T22)"
                } else if a == ata(&bc(&mint, &pf), &mint, &spl_token::id()) {
                    "assoc_bonding_curve(SPL)"
                } else if a == ata(&user, &mint, &spl_token_2022::id()) {
                    "user_base_ata(T22)"
                } else if a == ata(&user, &mint, &spl_token::id()) {
                    "user_base_ata(SPL)"
                } else if a == ata(&user, &solana_protocols::tokens::WSOL, &spl_token::id()) {
                    "user_wsol_ata"
                } else if a
                    == ata(
                        &bc(&mint, &pf),
                        &solana_protocols::tokens::WSOL,
                        &spl_token::id(),
                    )
                {
                    "curve_wsol_ata"
                } else if a == Pubkey::find_program_address(&[b"creator_vault", a.as_ref()], &pf).0
                {
                    "self?"
                } else {
                    "?"
                };
                *roles.entry(role).or_default() += 1;
            }
            let mut v: Vec<_> = roles.into_iter().collect();
            v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
            let top: Vec<String> = v.iter().take(2).map(|(r, c)| format!("{r} x{c}")).collect();
            println!("  {i:2}  {}", top.join(" | "));
        }
    }
}
