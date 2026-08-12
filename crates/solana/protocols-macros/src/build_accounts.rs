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
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    // Struct-level fixture (optional).
    let mut fixture: Option<LitStr> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("build") {
            let _ = attr.parse_nested_meta(|m| {
                if m.path.is_ident("fixture") {
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
    for (index, field) in fields.iter().enumerate() {
        let ident = field.ident.clone().unwrap();
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

    let field_names: Vec<_> = specs.iter().map(|s| &s.ident).collect();

    // Optional replay test: rebuild the real instruction from its own inputs.
    let replay = fixture.map(|fixture| {
        let input_args = specs
            .iter()
            .filter(|s| matches!(s.derivation, Derivation::Input))
            .map(|s| {
                let idx = s.index;
                quote! { real[#idx] }
            });
        let test_mod = format_ident!("__build_replay_{}", name.to_string().to_lowercase());
        quote! {
            #[cfg(test)]
            mod #test_mod {
                use super::*;
                /// Rebuild a real landed instruction from the inputs carried in
                /// its own accounts — every derived PDA/ATA must match the chain.
                #[test]
                fn build_matches_real_instruction() {
                    let fx = crate::test_fixtures::InstructionFixture::load(#fixture);
                    let real = fx.pubkeys();
                    let built = #name::derive(#(#input_args),*);
                    let built_keys: ::std::vec::Vec<_> =
                        built.to_account_metas().into_iter().map(|m| m.pubkey).collect();
                    assert_eq!(
                        built_keys.as_slice(),
                        &real[..built_keys.len()],
                        "built accounts must match the real instruction {}",
                        fx.signature,
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
