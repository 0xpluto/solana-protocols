//! `#[derive(OnchainState)]` — an account struct *is* its on-chain layout.
//!
//! The rule this enforces: a struct that models an on-chain account carries the
//! account's fields, in the account's order, and nothing else. Behaviour hangs
//! off it in an `impl`, exactly as Anchor programs do. Fields that are not in
//! the account are the bug class this removes — `PumpSwapPool` grew
//! `base_reserves`/`quote_reserves` that no decode could fill, so they arrived
//! through a `set_reserves` mutator callers had to remember, and a zero could
//! mean either "empty pool" or "nobody called it".
//!
//! The derive walks the field list computing a running byte offset, so the
//! fields *are* the layout: a field the account does not have shifts every
//! field after it, and the golden fixture fails immediately.
//!
//! # The one legitimate difference from chain
//!
//! Accounts grow. A program upgrade appends a field, and old accounts written
//! before it simply end early — deserializing them with the new layout hits
//! EOF. That is the only way our struct may diverge from the current on-chain
//! type, and it is expressed as trailing `Legacy<T>`:
//!
//! ```ignore
//! #[derive(OnchainState)]
//! #[state(discriminator = BONDING_CURVE_DISCRIMINATOR)]
//! pub struct BondingCurve {
//!     pub virtual_token_reserves: u64,
//!     pub creator: Pubkey,
//!     #[state(added_in = "cashback")] pub is_mayhem_mode:   Legacy<bool>,
//!     #[state(added_in = "cashback")] pub is_cashback_coin: Legacy<bool>,
//! }
//! ```
//!
//! `Absent` means **this account predates the field**. It is deliberately not
//! collapsed to a default: `Absent` and `Present(false)` are different facts,
//! and a decoder that answers `false` for both has destroyed the distinction
//! before any caller could act on it.
//!
//! `Option<T>` is **rejected** here rather than merely discouraged. Its
//! combinators (`unwrap_or`, `unwrap_or_default`) are ergonomic paths to
//! exactly that collapse, and offering the convenient collapse *is* choosing
//! it. `Legacy<T>` ships no combinators, which leaves `match` as the only
//! door.
//!
//! # Guarantees
//!
//! * **Optional fields must be trailing.** An `Option` before a required field
//!   is a layout that cannot be parsed; it is a compile error.
//! * **Version groups are all-or-nothing.** Fields sharing an `added_in` name
//!   arrived in one upgrade, so an account holding some of their bytes but not
//!   all is truncated, not an intermediate version — it is rejected rather than
//!   half-read. A group's fields must also be contiguous.
//! * **The minimum length is derived**, never transcribed. `REQUIRED_LEN` is
//!   the discriminator plus every non-optional field, summed at expansion. The
//!   hand-written equivalent is what rejected the majority of real PumpSwap
//!   pools (`POOL_ACCOUNT_SIZE = 301` against a true field span of 244).

use std::collections::BTreeMap;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Expr, Fields, GenericArgument, Ident, PathArguments, Type,
};

struct FieldSpec {
    ident: Ident,
    /// Inner type — `T` for a versioned `Option<T>`, the field type otherwise.
    ty: Type,
    /// Version group this field was added in; `None` for core fields.
    added_in: Option<String>,
    /// Padding to skip *before* this field.
    skip: usize,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    let mut discriminator: Option<Expr> = None;
    let mut no_discriminator = false;
    let mut fixtures: Vec<String> = Vec::new();
    for attr in &input.attrs {
        if !attr.path().is_ident("state") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|m| {
            if m.path.is_ident("discriminator") {
                discriminator = Some(m.value()?.parse()?);
            } else if m.path.is_ident("no_discriminator") {
                no_discriminator = true;
            } else if m.path.is_ident("fixtures") {
                // `fixtures("a.json", "b.json")` — one per real on-chain size
                // variant. Parsed as a list because a layout that decodes the
                // newest account can still reject the majority of live ones,
                // which is exactly what the 301-vs-244 span bug did.
                let content;
                syn::parenthesized!(content in m.input);
                let list = content
                    .parse_terminated(<syn::LitStr as syn::parse::Parse>::parse, syn::Token![,])?;
                fixtures.extend(list.into_iter().map(|l| l.value()));
            } else {
                return Err(m.error("expected discriminator = …, no_discriminator, or fixtures(…)"));
            }
            Ok(())
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
    }
    if discriminator.is_some() && no_discriminator {
        return err(&name, "pick one of discriminator = … or no_discriminator");
    }
    if discriminator.is_none() && !no_discriminator {
        return err(
            &name,
            "declare #[state(discriminator = …)] or #[state(no_discriminator)] — \
             whether the account carries a prefix is a layout fact, not a default",
        );
    }
    if fixtures.is_empty() {
        return err(
            &name,
            "declare #[state(fixtures(\"path/to.json\", …))] — the field list *is* the byte \
             layout, so it is only as true as the real accounts it decodes. List one fixture \
             per observed on-chain size variant: a layout that decodes the newest account can \
             still reject the majority of live ones",
        );
    }

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f.named.clone(),
            _ => return err(&name, "OnchainState requires named fields"),
        },
        _ => return err(&name, "OnchainState applies to structs only"),
    };

    let mut specs: Vec<FieldSpec> = Vec::new();
    for field in &fields {
        let ident = field.ident.clone().expect("named");
        let mut added_in: Option<String> = None;
        let mut skip: usize = 0;
        for attr in &field.attrs {
            if !attr.path().is_ident("state") {
                continue;
            }
            let parsed = attr.parse_nested_meta(|m| {
                if m.path.is_ident("added_in") {
                    added_in = Some(m.value()?.parse::<syn::LitStr>()?.value());
                } else if m.path.is_ident("skip") {
                    skip = m.value()?.parse::<syn::LitInt>()?.base10_parse()?;
                } else {
                    return Err(m.error("expected added_in = \"…\" or skip = N"));
                }
                Ok(())
            });
            if let Err(e) = parsed {
                return e.to_compile_error().into();
            }
        }

        let optional_inner = legacy_inner(&field.ty);
        match (&added_in, optional_inner) {
            (Some(_), Some(inner)) => specs.push(FieldSpec {
                ident,
                ty: inner,
                added_in,
                skip,
            }),
            (Some(_), None) => {
                return err(
                    &ident,
                    "a version-added field must be Legacy<T> — deliberately not Option<T>, \
                     whose unwrap_or/unwrap_or_default collapse \"absent\" into a default, \
                     which is the exact distinction this field exists to keep",
                )
            }
            (None, Some(_)) => {
                return err(
                    &ident,
                    "Legacy<T> without #[state(added_in = \"…\")] — a version-added field must \
                     say which upgrade added it, so its group is validated all-or-nothing",
                )
            }
            (None, None) => specs.push(FieldSpec {
                ident,
                ty: field.ty.clone(),
                added_in,
                skip,
            }),
        }
    }

    // --- optional fields must trail, groups must be contiguous -------------
    if let Some(first_opt) = specs.iter().position(|s| s.added_in.is_some()) {
        if let Some(bad) = specs[first_opt..].iter().find(|s| s.added_in.is_none()) {
            return err(
                &bad.ident,
                "a required field cannot follow a version-added one — bytes for an absent \
                 optional field do not exist, so nothing after it has a known offset",
            );
        }
    }
    let mut group_runs: Vec<String> = Vec::new();
    for spec in &specs {
        if let Some(g) = &spec.added_in {
            if group_runs.last() != Some(g) {
                if group_runs.contains(g) {
                    return err(
                        &spec.ident,
                        "fields of one version group must be contiguous — they arrived in a \
                         single upgrade and are validated as one block",
                    );
                }
                group_runs.push(g.clone());
            }
        }
    }

    // --- sizes --------------------------------------------------------------
    let disc_len: usize = if no_discriminator { 0 } else { 8 };
    let mut required_len = disc_len;
    let mut core_reads: Vec<TokenStream2> = Vec::new();
    for spec in specs.iter().filter(|s| s.added_in.is_none()) {
        required_len += spec.skip;
        let size = match type_size(&spec.ty) {
            Some(n) => n,
            None => return err(&spec.ident, &unsupported(&spec.ty)),
        };
        core_reads.push(read_stmt(&spec.ident, &spec.ty, required_len, size, false));
        required_len += size;
    }

    // --- version groups, in declaration order -------------------------------
    let mut groups: BTreeMap<usize, (String, Vec<&FieldSpec>)> = BTreeMap::new();
    for spec in specs.iter().filter(|s| s.added_in.is_some()) {
        let g = spec.added_in.clone().expect("filtered");
        let idx = group_runs.iter().position(|x| *x == g).expect("recorded");
        groups
            .entry(idx)
            .or_insert_with(|| (g, Vec::new()))
            .1
            .push(spec);
    }

    let mut group_blocks: Vec<TokenStream2> = Vec::new();
    let mut cursor = required_len;
    for (group_name, members) in groups.values() {
        let start = cursor;
        let mut reads: Vec<TokenStream2> = Vec::new();
        let mut nones: Vec<TokenStream2> = Vec::new();
        let mut off = cursor;
        for spec in members {
            off += spec.skip;
            let size = match type_size(&spec.ty) {
                Some(n) => n,
                None => return err(&spec.ident, &unsupported(&spec.ty)),
            };
            reads.push(read_stmt(&spec.ident, &spec.ty, off, size, true));
            let id = &spec.ident;
            nones.push(quote! { #id = ::solana_protocols::parsing::state::Legacy::Absent; });
            off += size;
        }
        let end = off;
        let decls: Vec<TokenStream2> = members
            .iter()
            .map(|s| {
                let (id, ty) = (&s.ident, &s.ty);
                quote! { let #id: ::solana_protocols::parsing::state::Legacy<#ty>; }
            })
            .collect();
        group_blocks.push(quote! {
            #(#decls)*
            if data.len() >= #end {
                #(#reads)*
            } else if data.len() > #start {
                // Some of the group's bytes but not all: a truncated account,
                // not an older one. Refuse rather than half-read it.
                return Err(::solana_protocols::parsing::state::AccountParseError::TruncatedVersion {
                    group: #group_name,
                    have: data.len() - #start,
                    need: #end - #start,
                });
            } else {
                #(#nones)*
            }
        });
        cursor = end;
    }

    let disc_check = discriminator.map(|expr| {
        quote! {
            let expected: [u8; 8] = #expr;
            if data[..8] != expected {
                return Err(::solana_protocols::parsing::state::AccountParseError::Discriminator);
            }
        }
    });

    let all_idents: Vec<&Ident> = specs.iter().map(|s| &s.ident).collect();
    let test_mod = quote::format_ident!("__onchain_layout_{}", name.to_string().to_lowercase());
    let first_fixture = fixtures[0].clone();

    quote! {
        impl ::solana_protocols::parsing::state::OnchainState for #name {
            const REQUIRED_LEN: usize = #required_len;

            fn from_account_data(
                data: &[u8],
            ) -> ::core::result::Result<Self, ::solana_protocols::parsing::state::AccountParseError> {
                if data.len() < Self::REQUIRED_LEN {
                    return Err(::solana_protocols::parsing::state::AccountParseError::TooShort {
                        len: data.len(),
                        need: Self::REQUIRED_LEN,
                    });
                }
                #disc_check
                #(#core_reads)*
                #(#group_blocks)*
                Ok(Self { #(#all_idents),* })
            }
        }

        #[cfg(test)]
        mod #test_mod {
            use super::*;

            /// Every declared size variant decodes, and its pinned fields match.
            ///
            /// Emitted by the derive, so a layout cannot ship unproven — the
            /// gap that let `POOL_ACCOUNT_SIZE = 301` reject most live pools
            /// while the suite stayed green.
            #[test]
            fn onchain_layout_matches_real_accounts() {
                use ::solana_protocols::parsing::state::OnchainState;
                for path in [#(#fixtures),*] {
                    let fx = crate::test_fixtures::AccountFixture::load(path);
                    let decoded = <#name as OnchainState>::from_account_data(fx.data())
                        .unwrap_or_else(|e| panic!("fixture {path} must decode: {e:?}"));
                    fx.assert_matches(&decoded);
                }
            }

            /// One byte short of the declared minimum must be refused, not
            /// half-read. Pins `REQUIRED_LEN` against the field list itself.
            #[test]
            fn one_byte_under_required_len_is_refused() {
                use ::solana_protocols::parsing::state::OnchainState;
                let fx = crate::test_fixtures::AccountFixture::load(#first_fixture);
                let short = &fx.data()[..<#name as OnchainState>::REQUIRED_LEN - 1];
                assert!(<#name as OnchainState>::from_account_data(short).is_err());
            }
        }
    }
    .into()
}

/// `Legacy<T>` → `T`.
fn legacy_inner(ty: &Type) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Legacy" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    }
}

fn type_size(ty: &Type) -> Option<usize> {
    if let Type::Array(arr) = ty {
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(n),
            ..
        }) = &arr.len
        {
            return n.base10_parse::<usize>().ok();
        }
        return None;
    }
    Some(match quote!(#ty).to_string().as_str() {
        "u8" | "i8" | "bool" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" => 4,
        "u64" | "i64" => 8,
        "u128" | "i128" => 16,
        "Pubkey" | "solana_program :: pubkey :: Pubkey" => 32,
        _ => return None,
    })
}

fn read_stmt(
    ident: &Ident,
    ty: &Type,
    offset: usize,
    size: usize,
    wrap_some: bool,
) -> TokenStream2 {
    let end = offset + size;
    let raw = match quote!(#ty).to_string().as_str() {
        "u8" => quote!(data[#offset]),
        "i8" => quote!(data[#offset] as i8),
        "bool" => quote!(data[#offset] != 0),
        "Pubkey" | "solana_program :: pubkey :: Pubkey" => quote! {
            ::solana_program::pubkey::Pubkey::new_from_array(
                data[#offset..#end].try_into().expect("length checked above"),
            )
        },
        _ if matches!(ty, Type::Array(_)) => quote! {
            data[#offset..#end].try_into().expect("length checked above")
        },
        _ => quote! {
            <#ty>::from_le_bytes(
                data[#offset..#end].try_into().expect("length checked above"),
            )
        },
    };
    let value = if wrap_some {
        quote!(::solana_protocols::parsing::state::Legacy::Present(#raw))
    } else {
        raw
    };
    if wrap_some {
        quote! { #ident = #value; }
    } else {
        quote! { let #ident = #value; }
    }
}

fn unsupported(ty: &Type) -> String {
    format!(
        "unsupported field type `{}` — OnchainState reads fixed-width layouts; \
         a variable-length field (Vec, String) needs borsh, not an offset walk",
        quote!(#ty)
    )
}

fn err<T: quote::ToTokens>(at: &T, msg: &str) -> TokenStream {
    syn::Error::new_spanned(at, msg).to_compile_error().into()
}
