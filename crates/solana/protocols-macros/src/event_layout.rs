//! `#[derive(EventLayout)]` — compile-time verification of an event struct
//! against the program's own IDL.
//!
//! Same argument as the account check next door: an event's **field order** is
//! data we cannot derive, so the IDL is the authority and disagreeing with it
//! must not compile. What is different here is that the IDL is sometimes
//! *behind the program*.
//!
//! Measured on pump's AMM 2026-08-15: both `BuyEvent` and `SellEvent` bodies
//! run 25 bytes past the last field either the vendored or the live on-chain
//! IDL declares. Those bytes carry data — one of them reads as a set flag on
//! every captured buy — and borsh refuses trailing bytes, so a struct faithful
//! to the IDL fails to decode *every real body*. Dropping them is not an option
//! either; that is how a fee split went unnoticed for months.
//!
//! So a field may opt out with `#[idl(undeclared = "...")]`, and the reason is
//! mandatory. `unknown` is a legitimate reason — it says we have the bytes and
//! not the meaning. Anything else should say what was proven and how, so the
//! next person inherits the evidence rather than the guess.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

use crate::idl_check::{check_event_fields, EventField};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let mut program = None;
    let mut event = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("idl") {
            continue;
        }
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("program") {
                program = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("event") {
                event = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else {
                return Err(m.error("expected program = \"…\" and event = \"…\""));
            }
            Ok(())
        }) {
            return e.to_compile_error().into();
        }
    }

    let (Some(program), Some(event)) = (program, event) else {
        return syn::Error::new_spanned(
            &input.ident,
            "#[derive(EventLayout)] needs #[idl(program = \"…\", event = \"…\")]",
        )
        .to_compile_error()
        .into();
    };

    let syn::Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(&input.ident, "EventLayout applies to structs")
            .to_compile_error()
            .into();
    };

    let mut fields = Vec::new();
    for f in &data.fields {
        let Some(ident) = f.ident.as_ref() else {
            continue;
        };
        let mut undeclared = None;
        for attr in &f.attrs {
            if !attr.path().is_ident("idl") {
                continue;
            }
            if let Err(e) = attr.parse_nested_meta(|m| {
                if m.path.is_ident("undeclared") {
                    undeclared = Some(m.value()?.parse::<syn::LitStr>()?.value());
                } else {
                    return Err(m.error("expected undeclared = \"…\""));
                }
                Ok(())
            }) {
                return e.to_compile_error().into();
            }
        }
        fields.push(EventField {
            name: ident.to_string(),
            undeclared,
        });
    }

    if let Err(msg) = check_event_fields(&program, &event, &fields) {
        return syn::Error::new_spanned(&input.ident, msg)
            .to_compile_error()
            .into();
    }

    // The verification is the whole point; there is nothing to generate. Emit a
    // marker impl so the derive is visible in the type's docs and so a future
    // consumer can bound on "this layout was checked".
    let name = &input.ident;
    let declared = fields.iter().filter(|f| f.undeclared.is_none()).count();
    let undeclared = fields.len() - declared;
    let expanded = quote! {
        impl #name {
            /// Fields this struct shares with the program's IDL.
            pub const IDL_DECLARED_FIELDS: usize = #declared;
            /// Fields the program emits that its IDL does not declare.
            pub const UNDECLARED_FIELDS: usize = #undeclared;
        }
    };
    expanded.into()
}
