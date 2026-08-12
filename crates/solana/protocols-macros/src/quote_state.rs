//! `#[derive(QuoteState)]` — generate a protocol's quote-state **assembly** and
//! its **dependency declaration** from one field list, so the two cannot drift.
//!
//! Most AMMs cannot be priced from a single account. PumpSwap needs the pool
//! plus two vault token accounts plus the fee config; a CLMM needs tick arrays;
//! a bonding curve needs its global. Two separate facts follow from that, and
//! before this macro they were written in two places:
//!
//! * **Assembly** — read those accounts out of a cache and combine them.
//! * **Declaration** — tell the ingest layer which accounts to keep live, so
//!   they are *in* the cache when assembly runs.
//!
//! Nothing checked that the two agreed. An account added to assembly but not to
//! the declaration makes `assemble` return `None` forever: the pool silently
//! stops being quotable, with no error anywhere. Generating both from one
//! annotated struct makes that disagreement unrepresentable.
//!
//! # Per-field `#[dep(...)]`
//!
//! - `root` — the pool/curve account itself, read at the pubkey passed to
//!   `assemble`. Exactly one field must be the root, and it must come first
//!   (later key expressions read from it).
//! - `key = EXPR, expect = …` — an account whose address is `EXPR`, typically
//!   read out of an earlier field (`root.pool_base_token_account`).
//! - `singleton` — a [`CacheSingleton`] value (fee config, global). Not keyed
//!   by pubkey, so it is never part of the dependency declaration.
//! - `computed = EXPR` — not an account: derived from earlier fields once they
//!   are read (e.g. selecting a fee tier out of a config account).
//!
//! `expect` is **mandatory** on every keyed field, deliberately. The delivery
//! class is what routes an account to a subscription versus an RPC fetch, and
//! misclassifying it has already cost us twice — once quoting on vaults that
//! were never subscribed, once collapsing ingestion ~20x by RPC-fetching a
//! class that was already streaming. A default would make the wrong answer the
//! quiet one; requiring it makes every account's delivery a stated decision.
//!
//! # What is generated
//!
//! ```text
//! impl PumpSwapQuote {
//!     pub fn assemble<C>(cache: &C, pool: &Pubkey, slot: u64) -> Option<Self>
//!     where C: CacheGet<Pubkey, PumpSwapPool>
//!            + CacheGet<Pubkey, TokenAccount>
//!            + CacheSingleton<PumpSwapFeeConfig> { … }
//!
//!     pub fn dependent_accounts(root: &PumpSwapPool) -> Vec<Dependency> { … }
//! }
//! ```
//!
//! The `where` clause is synthesised from the field types, which makes it the
//! account manifest: a caller that cannot supply every account a quote reads
//! does not compile. That is what lets a replay harness reuse the production
//! math — it satisfies the same bound from a tape row instead of a live cache.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, Ident, Path, Type};

/// How one field of the quote state is obtained.
enum Source {
    /// The pool account itself, at the pubkey passed to `assemble`.
    Root,
    /// An account at `key`, delivered per `expect`.
    Keyed { key: Expr, expect: Ident },
    /// A `CacheSingleton` value — no pubkey, so not a declared dependency.
    Singleton,
    /// Derived from earlier fields; reads no account.
    Computed(Expr),
}

struct FieldSpec {
    ident: Ident,
    ty: Type,
    source: Source,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f.named.clone(),
            _ => {
                return err(
                    &name,
                    "QuoteState requires named fields — each field declares how it is sourced",
                )
            }
        },
        _ => return err(&name, "QuoteState applies to structs only"),
    };

    let mut specs: Vec<FieldSpec> = Vec::new();
    for field in &fields {
        let ident = field.ident.clone().expect("named");
        let mut key: Option<Expr> = None;
        let mut expect: Option<Ident> = None;
        let mut computed: Option<Expr> = None;
        let mut is_root = false;
        let mut is_singleton = false;
        let mut saw_dep = false;

        for attr in &field.attrs {
            if !attr.path().is_ident("dep") {
                continue;
            }
            saw_dep = true;
            let parsed = attr.parse_nested_meta(|m| {
                if m.path.is_ident("root") {
                    is_root = true;
                } else if m.path.is_ident("singleton") {
                    is_singleton = true;
                } else if m.path.is_ident("key") {
                    key = Some(m.value()?.parse()?);
                } else if m.path.is_ident("expect") {
                    expect = Some(m.value()?.parse::<Path>()?.require_ident()?.clone());
                } else if m.path.is_ident("computed") {
                    computed = Some(m.value()?.parse()?);
                } else {
                    return Err(m.error("expected root, key, expect, singleton, or computed"));
                }
                Ok(())
            });
            if let Err(e) = parsed {
                return e.to_compile_error().into();
            }
        }

        if !saw_dep {
            return err(
                &ident,
                "every QuoteState field needs #[dep(...)] — an undeclared field is an account \
                 the dependency list would not know about",
            );
        }

        let source = match (is_root, is_singleton, key, computed) {
            (true, false, None, None) => Source::Root,
            (false, true, None, None) => Source::Singleton,
            (false, false, Some(key), None) => match expect {
                Some(expect) => Source::Keyed { key, expect },
                None => {
                    return err(
                        &ident,
                        "keyed field needs `expect = frequent|infrequent|dynamic` — delivery \
                         class decides subscription vs RPC fetch and has no safe default",
                    )
                }
            },
            (false, false, None, Some(expr)) => Source::Computed(expr),
            _ => {
                return err(
                    &ident,
                    "pick exactly one of: root, key = …, singleton, computed = …",
                )
            }
        };
        specs.push(FieldSpec {
            ident,
            ty: field.ty.clone(),
            source,
        });
    }

    let root = match specs.iter().find(|s| matches!(s.source, Source::Root)) {
        Some(s) => s,
        None => {
            return err(
                &name,
                "one field must be #[dep(root)] — the account `assemble`'s pubkey identifies",
            )
        }
    };
    if specs
        .iter()
        .filter(|s| matches!(s.source, Source::Root))
        .count()
        > 1
    {
        return err(&name, "only one field may be #[dep(root)]");
    }
    let root_ty = root.ty.clone();

    // --- where-clause: the account manifest, deduped by rendered type -------
    let mut bounds: Vec<TokenStream2> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut push_bound = |bound: TokenStream2, tag: String| {
        if !seen.contains(&tag) {
            seen.push(tag);
            bounds.push(bound);
        }
    };
    for spec in &specs {
        let ty = &spec.ty;
        match &spec.source {
            Source::Root | Source::Keyed { .. } => push_bound(
                quote!(::solana_account_traits::CacheGet<::solana_program::pubkey::Pubkey, #ty>),
                format!("get:{}", quote!(#ty)),
            ),
            Source::Singleton => push_bound(
                quote!(::solana_account_traits::CacheSingleton<#ty>),
                format!("singleton:{}", quote!(#ty)),
            ),
            Source::Computed(_) => {}
        }
    }

    // --- assemble body: field order is read order --------------------------
    let reads = specs.iter().map(|spec| {
        let ident = &spec.ident;
        let ty = &spec.ty;
        match &spec.source {
            Source::Root => quote! {
                let #ident: #ty = ::solana_account_traits::CacheGet::<
                    ::solana_program::pubkey::Pubkey, #ty
                >::get_at_slot(cache, pool, slot)?;
            },
            Source::Keyed { key, .. } => quote! {
                let #ident: #ty = ::solana_account_traits::CacheGet::<
                    ::solana_program::pubkey::Pubkey, #ty
                >::get_at_slot(cache, &(#key), slot)?;
            },
            Source::Singleton => quote! {
                let #ident: #ty =
                    <C as ::solana_account_traits::CacheSingleton<#ty>>::get(cache)?;
            },
            Source::Computed(expr) => quote! { let #ident = #expr; },
        }
    });
    let field_idents: Vec<&Ident> = specs.iter().map(|s| &s.ident).collect();

    // --- dependency declaration: keyed fields only -------------------------
    let deps = specs.iter().filter_map(|spec| match &spec.source {
        Source::Keyed { key, expect } => {
            let variant = expectation_variant(expect);
            Some(quote! {
                out.push(::solana_account_traits::Dependency::new(#key, #variant));
            })
        }
        _ => None,
    });
    let root_ident = &root.ident;
    let cache_trait = quote::format_ident!("{}Cache", name);
    let cache_doc = format!(
        "A cache that can supply everything [`{name}`] reads.\n\
         \n\
         Generated by `#[derive(QuoteState)]` as a **named** union of this quote's\n\
         account bounds, so a layer above can compose protocols without knowing\n\
         any of their fields — `trait QuoteCache: {name}Cache + …` is enough. The\n\
         blanket impl means nothing implements this by hand: a type either\n\
         supplies the accounts or it does not."
    );

    quote! {
        #[doc = #cache_doc]
        pub trait #cache_trait: #(#bounds)+* {}

        impl<T> #cache_trait for T where T: #(#bounds)+* {}

        impl #name {
            /// Assemble from a cache at `slot`.
            ///
            /// `None` when any account is missing — an unpriceable pool must
            /// refuse rather than quote on a default it mistook for state.
            /// Generated by `#[derive(QuoteState)]`; the bound is the manifest
            /// of every account this quote reads.
            pub fn assemble<C>(
                cache: &C,
                pool: &::solana_program::pubkey::Pubkey,
                slot: u64,
            ) -> Option<Self>
            where
                C: #cache_trait + ?Sized,
            {
                #(#reads)*
                Some(Self { #(#field_idents),* })
            }

            /// Accounts that must be kept live for [`assemble`] to succeed.
            ///
            /// Generated from the same fields `assemble` reads, so the ingest
            /// layer's view and the quoter's needs cannot disagree.
            ///
            /// [`assemble`]: Self::assemble
            #[must_use]
            pub fn dependent_accounts(
                #root_ident: &#root_ty,
            ) -> ::std::vec::Vec<::solana_account_traits::Dependency> {
                // A quote state may legitimately have no keyed dependencies —
                // a bonding curve carries its own reserves — in which case the
                // root is read by nothing here.
                let _ = &#root_ident;
                let mut out = ::std::vec::Vec::new();
                #(#deps)*
                out
            }
        }
    }
    .into()
}

/// Map the `expect = …` ident onto a [`DeliveryExpectation`] variant.
fn expectation_variant(ident: &Ident) -> TokenStream2 {
    match ident.to_string().as_str() {
        "frequent" => quote!(::solana_account_traits::DeliveryExpectation::Frequent),
        "infrequent" => quote!(::solana_account_traits::DeliveryExpectation::Infrequent),
        "dynamic" => quote!(::solana_account_traits::DeliveryExpectation::Dynamic),
        other => {
            let msg = format!(
                "unknown delivery expectation `{other}` \
                 (expected frequent, infrequent, or dynamic)"
            );
            quote!(compile_error!(#msg))
        }
    }
}

fn err<T: quote::ToTokens>(at: &T, msg: &str) -> TokenStream {
    syn::Error::new_spanned(at, msg).to_compile_error().into()
}
