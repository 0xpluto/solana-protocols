//! Shared scaffolding for a generated golden-fixture test.
//!
//! Both the accounts derive and the params derive want the same loop: judge
//! every fixture, keep going after a failure, and report them together. Written
//! twice it drifted immediately — the accounts version already aborted at the
//! first bad fixture, which made a struct broken at five account lengths
//! indistinguishable from one broken at a single length, and turned the count of
//! red tests into a count of nothing.
//!
//! One definition, so the reporting cannot differ between them.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, LitStr};

/// Wrap a per-fixture body in the collect-all-failures loop.
///
/// `body` is evaluated with `__fixture` bound to each path in turn and is free
/// to panic; a panic is recorded against that fixture and the walk continues.
pub fn walk(subject: &Ident, fixtures: &[LitStr], body: TokenStream2) -> TokenStream2 {
    quote! {
        let mut __failures: ::std::vec::Vec<::std::string::String> = ::std::vec::Vec::new();
        for __fixture in [#(#fixtures),*] {
            let __verdict = ::std::panic::catch_unwind(|| { #body });
            if let ::std::result::Result::Err(e) = __verdict {
                let msg = e
                    .downcast_ref::<::std::string::String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
                    .unwrap_or_else(|| "panicked".to_string());
                __failures.push(format!("  {__fixture}: {msg}"));
            }
        }
        ::std::assert!(
            __failures.is_empty(),
            "{} of {} fixtures failed for {}:\n{}",
            __failures.len(),
            [#(#fixtures),*].len(),
            ::std::stringify!(#subject),
            __failures.join("\n"),
        );
    }
}

/// Parse `fixture = "…"` / `fixtures("…", "…")` out of an attribute's nested
/// meta, appending to `out`.
///
/// # Errors
///
/// Propagates a malformed list.
pub fn parse_fixture_meta(
    meta: &syn::meta::ParseNestedMeta<'_>,
    out: &mut Vec<LitStr>,
) -> syn::Result<bool> {
    if meta.path.is_ident("fixture") {
        out.push(meta.value()?.parse()?);
        Ok(true)
    } else if meta.path.is_ident("fixtures") {
        let content;
        syn::parenthesized!(content in meta.input);
        out.extend(content.parse_terminated(<LitStr as syn::parse::Parse>::parse, syn::Token![,])?);
        Ok(true)
    } else {
        Ok(false)
    }
}
