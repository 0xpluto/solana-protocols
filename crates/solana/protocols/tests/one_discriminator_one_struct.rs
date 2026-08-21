//! Every on-chain discriminator owns its own accounts struct, in its own file.
//!
//! The derive already requires a `<Instruction>Accounts` to exist for every
//! `<Instruction>Params` carrying a discriminator, so a missing struct is a
//! build failure. It cannot see two things this test can:
//!
//! * a `pub type FooAccounts = BarAccounts;` alias, which satisfies the name
//!   while sharing the layout -- two of these existed and were the reason the
//!   rule needed enforcing at all;
//! * two discriminators declared in one file, which is what makes a reader open
//!   an instruction's file and not find its accounts struct.
//!
//! Sharing is not merely untidy. `#[idl(instruction = "buy")]` checks a struct
//! against **`buy`'s** IDL entry; a second discriminator riding the same struct
//! has its own IDL entry checked by nothing, and would keep validating against
//! the wrong one if the two ever diverged.

use std::path::{Path, PathBuf};

fn instruction_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocols");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.parent().is_some_and(|d| d.ends_with("instructions"))
                && p.extension().is_some_and(|x| x == "rs")
                && p.file_name().is_some_and(|n| n != "mod.rs")
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn rel(p: &Path) -> String {
    p.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocols"))
        .unwrap_or(p)
        .display()
        .to_string()
}

#[test]
fn no_accounts_struct_is_an_alias_for_another() {
    let mut aliases = Vec::new();
    for f in instruction_files() {
        let src = std::fs::read_to_string(&f).expect("readable");
        for line in src.lines() {
            let t = line.trim_start();
            if t.starts_with("pub type") && t.contains("Accounts") && t.contains('=') {
                aliases.push(format!("{}: {t}", rel(&f)));
            }
        }
    }
    assert!(
        aliases.is_empty(),
        "an alias satisfies the derive's name check while sharing the layout, so \
         the borrowing discriminator's IDL entry and fixtures are never its own:\n{aliases:#?}"
    );
}

#[test]
fn one_discriminator_per_instruction_file() {
    // Protocols whose whole vertical is still below the pumpfun/pumpswap
    // standard. Their accounts structs are already one-per-discriminator; what
    // is outstanding is splitting the file. Listed rather than skipped so the
    // work is visible, and so a *modelled* protocol regressing fails here.
    const NOT_YET_SPLIT: &[&str] = &[
        "meteora_dbc/instructions/swap.rs",
        "raydium_cpmm/instructions/swap.rs",
        "raydium_v4/instructions/swap.rs",
        "raydium_launchpad/instructions/swap.rs",
    ];

    let mut crowded = Vec::new();
    let mut checked = 0usize;
    for f in instruction_files() {
        let src = std::fs::read_to_string(&f).expect("readable");
        let n = src.matches("#[instruction_data(discriminator").count();
        checked += 1;
        let name = rel(&f).replace('\\', "/");
        if n > 1 && !NOT_YET_SPLIT.iter().any(|k| name.ends_with(k)) {
            crowded.push(format!("{name}: {n} discriminators"));
        }
    }
    assert!(
        checked > 50,
        "only {checked} instruction files -- not being read?"
    );
    assert!(
        crowded.is_empty(),
        "each discriminator gets its own file, so opening it shows its accounts \
         struct:\n{crowded:#?}"
    );

    // The exemptions must stay real: one that no longer holds several
    // discriminators should be deleted from the list, not left to rot.
    for k in NOT_YET_SPLIT {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/protocols")
            .join(k);
        let src = std::fs::read_to_string(&p).unwrap_or_default();
        assert!(
            src.matches("#[instruction_data(discriminator").count() > 1,
            "{k} is listed as not-yet-split but no longer holds several \
             discriminators -- remove it from the list"
        );
    }
}
