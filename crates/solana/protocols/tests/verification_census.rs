//! What is proven, what is excused, and what nobody has said anything about.
//!
//! Every layout in this crate can carry a proof: a golden fixture captured off
//! the firehose, a build replay against a real landed instruction, an IDL
//! check. Each of those can also be waived with a stated reason. The state that
//! must not exist is the third one -- **unstated**: no proof, no waiver, no
//! reason, nothing to grep for. An absence reads as "fine" from every angle,
//! which is how a decoder drifts from the program with a green suite.
//!
//! So this counts all three, per protocol, and fails when the unstated count
//! rises. It is deliberately a census rather than a gate: the exempt column is
//! large and shrinking it is a project, while the unstated column is a bug the
//! moment it is non-zero.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The most unstated gaps each protocol is allowed to carry.
///
/// A ratchet, not a target: these may only be edited **down**. Raising one is a
/// deliberate act that shows up in review, which is the whole point -- the
/// alternative is a number nobody notices moving.
const BUDGET: &[(&str, usize)] = &[
    ("meteora_damm_v2", 0),
    ("meteora_dbc", 0),
    ("meteora_dlmm", 0),
    ("pumpfun", 0),
    ("pumpswap", 0),
    ("raydium_clmm", 0),
    ("raydium_cpmm", 0),
    ("raydium_launchpad", 0),
    ("raydium_v4", 0),
    ("spl_token", 0),
];

#[derive(Default, Debug, Clone, Copy)]
struct Tally {
    accounts: usize,
    accounts_pinned: usize,
    accounts_excused: usize,
    /// Field names checked against the program's own IDL.
    named: usize,
    /// Layout decoded from bytes the chain actually wrote.
    bytes: usize,
    /// Every account slot proven to be the account we call it, by rebuilding
    /// the instruction's PDAs and ATAs and comparing to a landed one.
    slots: usize,
    layouts: usize,
    build: usize,
    build_replayed: usize,
    build_excused: usize,
    state: usize,
    state_pinned: usize,
    state_excused: usize,
    params: usize,
    params_pinned: usize,
    params_excused: usize,
}

impl Tally {
    /// Layouts carrying neither a proof nor a stated reason for not having one.
    fn unstated(self) -> usize {
        (self.accounts - self.accounts_pinned - self.accounts_excused)
            + (self.build - self.build_replayed - self.build_excused)
            + (self.state - self.state_pinned - self.state_excused)
            + (self.params - self.params_pinned - self.params_excused)
    }
}

fn rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out
}

/// Net bracket depth a line opens: `#[accounts(` opens two, `)]` closes two.
fn balance(s: &str) -> i32 {
    s.chars().fold(0, |d, c| match c {
        '(' | '[' => d + 1,
        ')' | ']' => d - 1,
        _ => d,
    })
}

/// Walk items, carrying the attribute block that precedes each one.
///
/// Depth-tracked rather than guessed from the last character: a multi-line
/// attribute's closing `)]` line starts with neither `#[` nor a continuation
/// marker, and treating it as ordinary code dropped the whole block. That bug
/// counted 9 of 112 accounts structs and reported a clean census.
fn tally(src: &str, t: &mut Tally) {
    let mut attrs = String::new();
    let mut depth = 0i32;
    for line in src.lines() {
        let s = line.trim_start();
        if depth > 0 {
            attrs.push_str(s);
            depth += balance(s);
            continue;
        }
        if s.starts_with("#[") {
            attrs.push_str(s);
            depth += balance(s);
            continue;
        }
        // Doc comments sit inside the block without ending it.
        if s.starts_with("///") || s.starts_with("//") || s.is_empty() {
            continue;
        }
        if !s.starts_with("pub struct") {
            attrs.clear();
            continue;
        }
        let pinned_ix = attrs.contains("onchain_ix(fixtures");
        let excused = attrs.contains("unverified");

        // The three proofs are independent and prove different things. A struct
        // can pass `named` and `bytes` and still call slot 5 by the wrong name:
        // the fixture round-trip is `from_pubkeys` then `to_account_metas`,
        // both positional, so it is an identity over any list of sufficient
        // length. Only rebuilding the derived accounts tests slot identity.
        let is_layout = attrs.contains("AccountMetas")
            || attrs.contains("OnchainState")
            || attrs.contains("instruction_data(discriminator");
        if is_layout {
            t.layouts += 1;
            t.named += usize::from(attrs.contains("idl(program"));
            t.bytes += usize::from(pinned_ix || attrs.contains("fixtures("));
            t.slots += usize::from(attrs.contains("build(fixture"));
        }
        if attrs.contains("AccountMetas") {
            t.accounts += 1;
            t.accounts_pinned += usize::from(pinned_ix);
            t.accounts_excused += usize::from(!pinned_ix && excused);
        }
        if attrs.contains("BuildAccounts") {
            t.build += 1;
            let replayed = attrs.contains("build(fixture");
            t.build_replayed += usize::from(replayed);
            t.build_excused += usize::from(!replayed && attrs.contains("unreplayed"));
        }
        if attrs.contains("OnchainState") {
            t.state += 1;
            let p = attrs.contains("fixtures") || attrs.contains("fixture =");
            t.state_pinned += usize::from(p);
            t.state_excused += usize::from(!p && excused);
        }
        if attrs.contains("instruction_data(discriminator") {
            t.params += 1;
            let p = attrs.contains("fixtures(");
            t.params_pinned += usize::from(p);
            t.params_excused += usize::from(!p && excused);
        }
        attrs.clear();
    }
}

#[test]
fn no_layout_is_silently_unproven() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocols");
    let mut by_proto: BTreeMap<String, Tally> = BTreeMap::new();

    for f in rs_files(&root) {
        let proto = f
            .strip_prefix(&root)
            .ok()
            .and_then(|r| {
                r.components()
                    .next()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "-".into());
        // A file directly under `protocols/` is shared plumbing, not a protocol.
        if Path::new(&proto).extension().is_some() {
            continue;
        }
        let src = std::fs::read_to_string(&f).expect("readable");
        tally(&src, by_proto.entry(proto).or_default());
    }

    println!(
        "\n{:<18} {:>9} {:>7} {:>7} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7} {:>9}",
        "protocol",
        "accounts",
        "pinned",
        "excused",
        "build",
        "stated",
        "state",
        "pinned",
        "params",
        "pinned",
        "UNSTATED"
    );
    let mut total = Tally::default();
    let mut over: Vec<String> = Vec::new();
    for (proto, t) in &by_proto {
        println!(
            "{:<18} {:>9} {:>7} {:>7} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7} {:>9}",
            proto,
            t.accounts,
            t.accounts_pinned,
            t.accounts_excused,
            t.build,
            t.build_replayed + t.build_excused,
            t.state,
            t.state_pinned + t.state_excused,
            t.params,
            t.params_pinned + t.params_excused,
            t.unstated()
        );
        total.accounts += t.accounts;
        total.accounts_pinned += t.accounts_pinned;
        total.accounts_excused += t.accounts_excused;
        total.layouts += t.layouts;
        total.named += t.named;
        total.bytes += t.bytes;
        total.slots += t.slots;
        total.build += t.build;
        total.build_replayed += t.build_replayed;
        total.build_excused += t.build_excused;
        total.state += t.state;
        total.state_pinned += t.state_pinned;
        total.state_excused += t.state_excused;
        total.params += t.params;
        total.params_pinned += t.params_pinned;
        total.params_excused += t.params_excused;

        let budget = BUDGET
            .iter()
            .find(|(p, _)| p == proto)
            .map_or(0, |(_, b)| *b);
        if t.unstated() > budget {
            over.push(format!(
                "{proto}: {} unstated, budget {budget}",
                t.unstated()
            ));
        }
    }
    println!(
        "{:<18} {:>9} {:>7} {:>7} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7} {:>9}\n",
        "TOTAL",
        total.accounts,
        total.accounts_pinned,
        total.accounts_excused,
        total.build,
        total.build_replayed + total.build_excused,
        total.state,
        total.state_pinned + total.state_excused,
        total.params,
        total.params_pinned + total.params_excused,
        total.unstated()
    );

    println!(
        "\nproof strength -- each proves something the others do not\n\
         {:<18} {:>8} {:>8} {:>8} {:>8}",
        "protocol", "layouts", "named", "bytes", "slots"
    );
    for (proto, t) in &by_proto {
        println!(
            "{:<18} {:>8} {:>8} {:>8} {:>8}",
            proto, t.layouts, t.named, t.bytes, t.slots
        );
    }
    println!(
        "{:<18} {:>8} {:>8} {:>8} {:>8}",
        "TOTAL", total.layouts, total.named, total.bytes, total.slots
    );
    println!(
        "  named = field names agree with the program's IDL\n\
         bytes = decodes a real account/instruction captured off the firehose\n\
         slots = each account slot rebuilt from its PDA/ATA and matched to a landed ix\n"
    );

    // A parse bug would count nothing and pass every budget, so the census
    // asserts it actually saw the crate before it asserts anything about it.
    assert!(
        total.accounts > 100,
        "only {} accounts structs seen -- census not reading the crate?",
        total.accounts
    );
    assert!(
        total.params > 20,
        "only {} params structs seen",
        total.params
    );
    assert!(over.is_empty(), "unstated gaps above budget:\n{over:#?}");
}
