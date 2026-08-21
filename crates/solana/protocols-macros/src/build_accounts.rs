//! `#[derive(BuildAccounts)]` — generate an instruction accounts-struct's
//! **builder** from declarative per-field derivations, and (given a fixture) a
//! replay test that rebuilds a real on-chain instruction from its own inputs.
//!
//! This is the boilerplate-killer for the build side: instead of a hand-written
//! `fn new(keys, user)` that imperatively derives 16 PDAs/ATAs/consts, each
//! field declares *how* it is produced and the macro emits `derive(inputs…)`.
//! The annotation table is the account-provenance documentation — and it can't
//! drift from the code because it is the code.
//!
//! Per-field `#[build(...)]`:
//! - `input` — a parameter of `derive()` (declaration order = parameter order).
//! - `key = EXPR` — a constant / expression (`GLOBAL_PDA`, `spl_token::id()`).
//! - `pda(program = EXPR, seeds(A, B, …))` — `find_program_address`; a seed that
//!   names a field/param is passed `.as_ref()`, any other path is used as-is
//!   (a `&[u8]` seed const).
//! - `ata(owner = FIELD, mint = FIELD)` — `get_associated_token_address`.
//!
//! Struct-level `#[build(fixture = "…")]` (optional) emits the replay test:
//! extract each `input` field from the real instruction's account at the field's
//! position, `derive(...)`, and assert every produced key matches the chain.
//! That verifies the PDA/ATA derivations end-to-end — the highest-value test,
//! auto-generated.

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, LitStr, Path};

enum Derivation {
    Input,
    Key(Expr),
    Pda {
        program: Expr,
        seeds: Vec<Path>,
    },
    Ata {
        owner: Path,
        mint: Path,
        program: Option<Path>,
    },
}

struct FieldSpec {
    ident: syn::Ident,
    index: usize,
    derivation: Derivation,
    /// How the struct stores it. Appended accounts are `Conditional`, so the
    /// derived pubkey is wrapped rather than assigned bare.
    wrapper: Wrapper,
}

/// The field's storage shape.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Wrapper {
    /// `Conditional`, and not appended by a builder unless the caller asks.
    ConditionalOptional,
    /// `Pubkey` — assign directly.
    Bare,
    /// `Conditional` — a builder always emits what it derives, so `Present`.
    Conditional,
    /// `Vec<Pubkey>` — nothing to derive; a builder supplies these or does not.
    List,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    // Struct-level fixture, or a stated reason there is none.
    //
    // A builder with no replay is a builder nobody has ever compared against a
    // real landed instruction, and an absent attribute is indistinguishable
    // from an author who never considered it. Requiring one or the other turns
    // that into a fact the census can count.
    let mut fixture: Option<LitStr> = None;
    let mut unreplayed: Option<String> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("build") {
            let _ = attr.parse_nested_meta(|m| {
                if m.path.is_ident("unreplayed") {
                    unreplayed = Some(m.value()?.parse::<LitStr>()?.value());
                } else if m.path.is_ident("fixture") {
                    fixture = Some(m.value()?.parse()?);
                }
                Ok(())
            });
        }
    }

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(n) => &n.named,
            _ => {
                return syn::Error::new_spanned(&name, "BuildAccounts needs named fields")
                    .to_compile_error()
                    .into()
            }
        },
        _ => {
            return syn::Error::new_spanned(&name, "BuildAccounts only supports structs")
                .to_compile_error()
                .into()
        }
    };

    let field_idents: HashSet<String> = fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap().to_string())
        .collect();

    let mut specs: Vec<FieldSpec> = Vec::new();
    // `remaining` lists have nothing to derive, but they are still fields of the
    // struct being built, so they have to reach the literal.
    let mut list_fields: Vec<syn::Ident> = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let ident = field.ident.clone().unwrap();
        let ty = quote::ToTokens::to_token_stream(&field.ty)
            .to_string()
            .replace(' ', "");
        let wrapper = if ty.ends_with("Conditional") {
            Wrapper::Conditional
        } else if ty.starts_with("Vec<") {
            Wrapper::List
        } else {
            Wrapper::Bare
        };
        // A `remaining` list has nothing to derive: which accounts appear is the
        // caller's choice, not a function of this instruction.
        if wrapper == Wrapper::List {
            list_fields.push(ident);
            continue;
        }
        let mut optional = false;
        let build_attr = field.attrs.iter().find(|a| a.path().is_ident("build"));
        let Some(attr) = build_attr else {
            return syn::Error::new_spanned(
                &ident,
                "every field needs a `#[build(...)]` derivation (input/key/pda/ata)",
            )
            .to_compile_error()
            .into();
        };
        let mut derivation: Option<Derivation> = None;
        let parsed = attr.parse_nested_meta(|m| {
            if m.path.is_ident("input") {
                derivation = Some(Derivation::Input);
            } else if m.path.is_ident("key") {
                derivation = Some(Derivation::Key(m.value()?.parse()?));
            } else if m.path.is_ident("pda") {
                let mut program: Option<Expr> = None;
                let mut seeds: Vec<Path> = Vec::new();
                m.parse_nested_meta(|p| {
                    if p.path.is_ident("program") {
                        program = Some(p.value()?.parse()?);
                    } else if p.path.is_ident("seeds") {
                        p.parse_nested_meta(|s| {
                            seeds.push(s.path.clone());
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
                let program =
                    program.ok_or_else(|| m.error("pda(...) requires `program = EXPR`"))?;
                derivation = Some(Derivation::Pda { program, seeds });
            } else if m.path.is_ident("optional") {
                optional = true;
            } else if m.path.is_ident("ata") {
                let mut owner: Option<Path> = None;
                let mut mint: Option<Path> = None;
                let mut program: Option<Path> = None;
                m.parse_nested_meta(|a| {
                    if a.path.is_ident("owner") {
                        owner = Some(a.value()?.parse()?);
                    } else if a.path.is_ident("mint") {
                        mint = Some(a.value()?.parse()?);
                    } else if a.path.is_ident("program") {
                        // The token program (classic vs Token-2022). Names a
                        // field/param resolved at runtime from the mint owner.
                        program = Some(a.value()?.parse()?);
                    }
                    Ok(())
                })?;
                derivation = Some(Derivation::Ata {
                    owner: owner.ok_or_else(|| m.error("ata(...) requires `owner = FIELD`"))?,
                    mint: mint.ok_or_else(|| m.error("ata(...) requires `mint = FIELD`"))?,
                    program,
                });
            }
            Ok(())
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
        let Some(derivation) = derivation else {
            return syn::Error::new_spanned(&ident, "unrecognised `#[build(...)]` derivation")
                .to_compile_error()
                .into();
        };
        specs.push(FieldSpec {
            ident,
            index,
            wrapper: if optional && wrapper == Wrapper::Conditional {
                Wrapper::ConditionalOptional
            } else {
                wrapper
            },
            derivation,
        });
    }

    // Parameters = input fields in declaration order.
    let params: Vec<_> = specs
        .iter()
        .filter(|s| matches!(s.derivation, Derivation::Input))
        .map(|s| {
            let id = &s.ident;
            quote! { #id: ::solana_program::pubkey::Pubkey }
        })
        .collect();

    // Per-field binding (`let name = …;`), in declaration order so a later field
    // can reference an earlier one (bonding_curve before its ATA).
    let bindings: Vec<TokenStream2> = specs
        .iter()
        .map(|s| {
            let id = &s.ident;
            match &s.derivation {
                // Inputs arrive as parameters — already bound by that name.
                Derivation::Input => quote! {},
                Derivation::Key(expr) => quote! { let #id = #expr; },
                Derivation::Pda { program, seeds } => {
                    let seed_toks = seeds.iter().map(|seed| {
                        let is_field = seed
                            .get_ident()
                            .is_some_and(|i| field_idents.contains(&i.to_string()));
                        if is_field {
                            quote! { #seed.as_ref() }
                        } else {
                            quote! { #seed }
                        }
                    });
                    quote! {
                        let #id = ::solana_program::pubkey::Pubkey::find_program_address(
                            &[ #(#seed_toks),* ],
                            &#program,
                        ).0;
                    }
                }
                Derivation::Ata { owner, mint, program } => match program {
                    // ATA derivation folds the token program into the seeds, so a
                    // Token-2022 mint yields a different ATA than a classic one.
                    Some(prog) => quote! {
                        let #id = ::spl_associated_token_account::get_associated_token_address_with_program_id(
                            &#owner, &#mint, &#prog,
                        );
                    },
                    None => quote! {
                        let #id = ::spl_associated_token_account::get_associated_token_address(
                            &#owner, &#mint,
                        );
                    },
                },
            }
        })
        .collect();

    // Wrap where the field is `Conditional`: a builder emits what it derives, so
    // the derived pubkey is always `Present`. `remaining` lists are skipped
    // entirely and default, because which of them appear is caller policy.
    let field_names: Vec<_> = specs
        .iter()
        .map(|s| {
            let id = &s.ident;
            match s.wrapper {
                Wrapper::Bare => quote! { #id },
                Wrapper::Conditional => quote! {
                    #id: ::solana_protocols::parsing::accounts::Conditional::Present(#id)
                },
                // Derivable, but appending it changes what the program does —
                // a cashback accumulator on a coin without cashback is an
                // account the program did not ask for. The caller opts in.
                Wrapper::ConditionalOptional => quote! {
                    #id: {
                        // The derivation still runs, so a wrong seed set is a
                        // compile error rather than dead annotation, but the
                        // builder does not append the account.
                        let _ = #id;
                        ::solana_protocols::parsing::accounts::Conditional::Absent
                    }
                },
                Wrapper::List => quote! { #id: ::std::vec::Vec::new() },
            }
        })
        .chain(
            list_fields
                .iter()
                .map(|id| quote! { #id: ::std::vec::Vec::new() }),
        )
        .collect();

    // Optional replay test: rebuild the real instruction from its own inputs.
    // One or the other: a replay, or a stated reason there is none.
    if fixture.is_none() && unreplayed.as_ref().is_none_or(|r| r.trim().len() < 12) {
        return syn::Error::new_spanned(
            name,
            "#[derive(BuildAccounts)] needs #[build(fixture = \"…\")] — one real landed \
             instruction to rebuild and compare — or #[build(unreplayed = \"why not\")]. \
             A builder never compared against a landed instruction is exactly where a \
             derivation drifts from the program, and an absent attribute cannot be told \
             apart from an author who never considered it",
        )
        .to_compile_error()
        .into();
    }

    let struct_label = name.to_string();
    // Appended slots that carry a derivation -- the ones a replay can check.
    let tail_specs: Vec<&FieldSpec> = specs
        .iter()
        .filter(|s| {
            matches!(
                s.wrapper,
                Wrapper::Conditional | Wrapper::ConditionalOptional
            ) && !matches!(s.derivation, Derivation::Input)
        })
        .collect();
    let has_derivable_tail = !tail_specs.is_empty();
    let replay = fixture.map(|fixture| {
        let input_args = specs
            .iter()
            .filter(|s| matches!(s.derivation, Derivation::Input))
            .map(|s| {
                let idx = s.index;
                quote! { real[#idx] }
            });
        // One assertion per appended slot that has a derivation.
        //
        // Compared against the derivation, never against `built.field`: a
        // `ConditionalOptional` is deliberately *not* appended by the builder
        // (a cashback accumulator on a coin without cashback is an account the
        // program did not ask for), so reading it back off `built` would test
        // the builder's opt-in policy and pass whatever the seeds said. The
        // derivation is what a caller who opts in would place, and it is what
        // has to be right.
        let input_binds: Vec<TokenStream2> = specs
            .iter()
            .filter(|s| matches!(s.derivation, Derivation::Input))
            .map(|s| {
                let id = &s.ident;
                let idx = s.index;
                quote! { let #id = real[#idx]; }
            })
            .collect();
        let all_bindings = bindings.clone();
        let tail_checks: Vec<TokenStream2> = tail_specs
            .iter()
            .map(|s| {
                let id = &s.ident;
                let label = id.to_string();
                quote! {
                    if let crate::parsing::accounts::Conditional::Present(on_chain) = parsed.#id {
                        assert_eq!(
                            #id, on_chain,
                            "appended account `{}` derives to the wrong address for {}: \
                             the chain carried a different one, so an instruction built \
                             with our derivation would name an account the program does \
                             not expect",
                            #label, fx.signature,
                        );
                        checked += 1;
                    }
                }
            })
            .collect();

        let test_mod = format_ident!("__build_replay_{}", name.to_string().to_lowercase());
        quote! {
            #[cfg(test)]
            mod #test_mod {
                use super::*;
                /// Rebuild a real landed instruction from the inputs carried in
                /// its own accounts — every derived PDA/ATA must match the chain.
                #[test]
                // Every field's derivation is re-run here to check the appended
                // slots; the fixed ones are checked positionally and their
                // bindings go unread.
                #[allow(unused_variables)]
                fn build_matches_real_instruction() {
                    let fx = crate::test_fixtures::InstructionFixture::load(#fixture);
                    let real = fx.pubkeys();
                    let built = #name::derive(#(#input_args),*);
                    let built_keys: ::std::vec::Vec<_> =
                        built.to_account_metas().into_iter().map(|m| m.pubkey).collect();
                    // The fixed slots compare positionally.
                    let n = #name::ACCOUNT_COUNT.min(built_keys.len()).min(real.len());
                    assert_eq!(
                        &built_keys[..n],
                        &real[..n],
                        "built accounts must match the real instruction {}",
                        fx.signature,
                    );

                    // Appended slots cannot be compared by position: a caller who
                    // sends no conditionals but two buyback vaults puts a vault
                    // where our build puts a PDA, and a positional check would
                    // call that a defect. But they are still derived, and a
                    // builder that derives them wrongly emits an instruction the
                    // program rejects — so presence is taken from the real
                    // instruction and the *value* is asserted against ours.
                    //
                    // The tail was previously skipped wholesale, excused by the
                    // buyback roster being caller policy. That is true of the
                    // roster and false of the PDAs beside it.
                    let parsed = <#name as crate::parsing::FromAccountKeys>::from_account_keys(
                        &real,
                    )
                    .expect("the fixture's own account list must parse");
                    #(#input_binds)*
                    #(#all_bindings)*
                    let mut checked = 0usize;
                    #(#tail_checks)*
                    println!(
                        "{}: {} appended slot(s) verified against {}",
                        #struct_label, checked, fx.signature,
                    );
                    // Which appended slots a capture carries is the caller's
                    // choice, but *which capture we replay* is ours. A struct
                    // with derivable appended accounts replayed against an
                    // instruction that has none proves nothing about them, and
                    // says so only in a println nobody reads. This fired on
                    // `buy`, whose fixture was an 18-account instruction while
                    // a 19-account one sat beside it in the same directory.
                    assert!(
                        !(#has_derivable_tail && checked == 0),
                        "{} derives appended accounts, but the replay fixture {} \
                         carries none of them, so none is checked. Point \
                         #[build(fixture = ...)] at a capture whose account list \
                         runs past the fixed slots",
                        #struct_label,
                        #fixture,
                    );
                }
            }
        }
    });

    quote! {
        impl #name {
            /// Build every account from the minimal `#[build(input)]` set,
            /// deriving all PDAs/ATAs/consts. Generated by `#[derive(BuildAccounts)]`.
            #[must_use]
            pub fn derive(#(#params),*) -> Self {
                #(#bindings)*
                Self { #(#field_names),* }
            }
        }

        #replay
    }
    .into()
}
