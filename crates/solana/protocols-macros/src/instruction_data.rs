//! `#[derive(InstructionData)]` — an instruction's arguments.
//!
//! # What this generates, and what it deliberately does not
//!
//! Decoding is **not** generated. It comes from `FromInstructionData`'s default,
//! which is borsh. Solana programs serialize their arguments with borsh, so a
//! generated offset walk is a second implementation of the producer's codec —
//! the same defect class as a transcribed discriminator, and one that agrees
//! until it does not.
//!
//! That second implementation used to live here: ~400 lines computing field
//! offsets and reading them back. It could not express `String` or `Vec`, so any
//! struct containing one was silently left with no decoder at all; it sized
//! arrays by element *count* rather than bytes; and its length check was a
//! *minimum*, so trailing bytes were ignored rather than refused — which is how
//! an undeclared `track_volume` rode along on `buy_exact_quote_in_v2` unnoticed.
//! borsh has none of those bugs because borsh is what wrote the bytes.
//!
//! Encoding is generated, and is also borsh: `to_data()` is the discriminator
//! followed by `borsh::to_vec(self)`. Both directions going through one codec is
//! what makes the round trip a fact rather than a hope.
//!
//! # Golden fixtures, because "it decodes" is only true of bytes we have seen
//!
//! `#[instruction_data(fixtures(…))]` emits a test that decodes each real landed
//! instruction's data and re-encodes it. This is a different axis from the
//! accounts derive: an instruction can have a perfectly good account list and
//! params that will not decode. Pumpswap's `sell` refuses two real mainnet
//! instructions with "Not all bytes read" — a trailing field nothing models —
//! and its accounts struct's own test could never have seen that, because it
//! only ever looked at the account list.
//!
//! So what remains here is the part borsh has no opinion about:
//!
//! * the `DISCRIMINATOR` / `DISCRIMINATOR_SIZE` associated constants, which are
//!   identity rather than layout, and
//! * `to_data()`, which has to prepend that discriminator.
//!
//! # Examples
//!
//! ```ignore
//! #[derive(BorshDeserialize, BorshSerialize, InstructionData)]
//! #[instruction_data(discriminator = BUY_DISCRIMINATOR)]
//! pub struct BuyParams { pub amount: u64, pub max_sol_cost: u64 }
//!
//! // A one-byte instruction index, for programs that predate Anchor.
//! #[derive(BorshDeserialize, BorshSerialize, InstructionData)]
//! #[instruction_data(discriminator = [SWAP_BASE_IN_IX], discriminator_size = 1)]
//! pub struct SwapBaseInParams { pub amount_in: u64, pub minimum_amount_out: u64 }
//! ```

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut discriminator: Option<Expr> = None;
    let mut disc_size: usize = 8;
    let mut fixtures: Vec<syn::LitStr> = Vec::new();
    let mut unverified: Option<String> = None;
    let mut idl: Option<(String, String)> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("instruction_data") {
            continue;
        }
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("discriminator") {
                discriminator = Some(m.value()?.parse()?);
            } else if m.path.is_ident("discriminator_size") {
                let lit: syn::LitInt = m.value()?.parse()?;
                disc_size = lit.base10_parse()?;
            } else if crate::fixture_walk::parse_fixture_meta(&m, &mut fixtures)? {
            } else if m.path.is_ident("unverified") {
                unverified = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("idl") {
                let content;
                syn::parenthesized!(content in m.input);
                let (mut prog, mut ixn) = (None, None);
                while !content.is_empty() {
                    let key: syn::Ident = content.parse()?;
                    let _: syn::Token![=] = content.parse()?;
                    let val: syn::LitStr = content.parse()?;
                    if key == "program" {
                        prog = Some(val.value());
                    } else if key == "instruction" {
                        ixn = Some(val.value());
                    }
                    if !content.is_empty() {
                        let _: syn::Token![,] = content.parse()?;
                    }
                }
                if let (Some(p), Some(i)) = (prog, ixn) {
                    idl = Some((p, i));
                }
            } else {
                return Err(m.error(
                    "expected discriminator = …, discriminator_size = …, fixtures(…) \
                     or unverified = …",
                ));
            }
            Ok(())
        }) {
            return e.to_compile_error().into();
        }
    }

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f.named.clone(),
            Fields::Unit => Default::default(),
            Fields::Unnamed(_) => {
                return syn::Error::new_spanned(name, "InstructionData needs named fields")
                    .to_compile_error()
                    .into()
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "InstructionData applies to structs")
                .to_compile_error()
                .into()
        }
    };

    // A trailing `OptionBool` consumes the rest of the buffer, so anything after
    // it is unreachable. borsh would discover that at runtime, on some
    // transaction, as a decode failure nobody can place; the compiler can say it
    // now. This is the one field-level rule the derive still enforces, because
    // it is a property of the *struct*, not of any type in it.
    let n = fields.len();
    for (idx, field) in fields.iter().enumerate() {
        let is_option_bool = field
            .ty
            .to_token_stream()
            .to_string()
            .replace(' ', "")
            .ends_with("OptionBool");
        if is_option_bool && idx + 1 != n {
            return syn::Error::new_spanned(
                &field.ty,
                "a trailing OptionBool must be the final field: it consumes the \
                 remainder of the instruction data, so any field after it is \
                 unreachable",
            )
            .to_compile_error()
            .into();
        }
    }

    let Some(discriminator) = discriminator else {
        // No discriminator: the arguments are the whole payload.
        return quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                /// Discriminator size (0 = none).
                pub const DISCRIMINATOR_SIZE: usize = 0;

                /// Argument bytes, borsh-encoded.
                ///
                /// # Panics
                ///
                /// If the value cannot be serialized, which for a type built
                /// from borsh-derived fields means an allocation failure.
                #[must_use]
                pub fn to_data(&self) -> Vec<u8> {
                    ::borsh::to_vec(self).expect("borsh serialization of a derived type")
                }
            }

            impl #impl_generics crate::parsing::InstructionParams for #name #ty_generics
                #where_clause {}
        }
        .into();
    };

    // The field list against the program's own declared arguments. This had no
    // check at all while accounts and events both had one, and the gap cost a
    // whole argument: `CreateParams` modelled three of `create`'s four for its
    // entire life, discarding the `creator` that seeds the coin's fee vault.
    if let Some((prog, ixn)) = &idl {
        let arg_fields: Vec<crate::idl_check::EventField> = fields
            .iter()
            .map(|f| crate::idl_check::EventField {
                name: f.ident.as_ref().map(ToString::to_string).unwrap_or_default(),
                undeclared: f.attrs.iter().find_map(|a| {
                    if !a.path().is_ident("idl") {
                        return None;
                    }
                    let mut reason = None;
                    let _ = a.parse_nested_meta(|m| {
                        if m.path.is_ident("undeclared") {
                            reason = Some(m.value()?.parse::<syn::LitStr>()?.value());
                        }
                        Ok(())
                    });
                    reason
                }),
            })
            .collect();
        if let Err(msg) = crate::idl_check::check_args(prog, ixn, &arg_fields) {
            return syn::Error::new_spanned(name, msg).to_compile_error().into();
        }
    }

    // Params must be pinned against a real landed instruction, or say why not.
    //
    // Unpinned params are how a hand-rolled decoder hid for months: nothing
    // compared the struct to bytes the chain actually carried. Requiring the
    // fixture makes "we never checked" a build failure instead of an absence
    // nobody can see. The opt-out is deliberate and costs a stated reason —
    // usually "this instruction has not been witnessed on the firehose yet".
    if fixtures.is_empty() && unverified.as_ref().is_none_or(|r| r.trim().len() < 12) {
        return syn::Error::new_spanned(
            name,
            "#[instruction_data(…)] needs fixtures(\"…\") — one real landed instruction \
             per params shape observed on chain — or unverified = \"why not\". Params \
             that are never compared against real bytes are exactly where a decoder \
             drifts from the program without anything failing",
        )
        .to_compile_error()
        .into();
    }

    let params_test = if fixtures.is_empty() {
        quote!()
    } else {
        let test_mod =
            quote::format_ident!("__params_fixture_{}", name.to_string().to_lowercase());
        let walk = crate::fixture_walk::walk(
            name,
            &fixtures,
            quote! {
                let fx = crate::test_fixtures::InstructionFixture::load(__fixture);
                // The args, not the whole payload: `from_instruction_data`
                // decodes arguments and the discriminator is not one. Handing it
                // the full data appeared to work on any struct ending in an
                // `OptionBool`, because that field absorbs whatever is left —
                // which is exactly the silent-garbage outcome this test exists
                // to catch, so it is worth stating rather than just slicing.
                let args = &fx.data()[#name::DISCRIMINATOR_SIZE..];
                let decoded = <#name as crate::parsing::FromInstructionData>::from_instruction_data(
                    args,
                )
                .unwrap_or_else(|e| panic!("params of the real instruction: {e}"));
                // Re-encode: the same codec both directions, so a field we
                // decoded but do not carry shows up as a shorter payload rather
                // than passing silently.
                let reencoded = decoded.to_data();
                ::std::assert_eq!(
                    reencoded.as_slice(),
                    fx.data(),
                    "re-encoding must reproduce the bytes the chain carried",
                );
            },
        );
        quote! {
            #[cfg(test)]
            mod #test_mod {
                use super::*;

                /// Params decode from real landed instruction data, and
                /// re-encode to the same bytes.
                #[test]
                fn params_fixture_roundtrips() {
                    #walk
                }
            }
        }
    };

    let disc_ty = quote! { [u8; #disc_size] };
    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// This instruction's discriminator.
            pub const DISCRIMINATOR: #disc_ty = #discriminator;

            /// Discriminator width in bytes.
            pub const DISCRIMINATOR_SIZE: usize = #disc_size;

            /// The discriminator this instruction is dispatched on.
            #[must_use]
            pub const fn discriminator() -> #disc_ty {
                Self::DISCRIMINATOR
            }

            /// Full instruction data: discriminator followed by borsh-encoded
            /// arguments.
            ///
            /// The same codec both directions, so `to_data` and
            /// `from_instruction_data` cannot drift apart the way a generated
            /// writer and a generated reader could.
            ///
            /// # Panics
            ///
            /// If the value cannot be serialized, which for a type built from
            /// borsh-derived fields means an allocation failure.
            #[must_use]
            pub fn to_data(&self) -> Vec<u8> {
                let mut data = Self::DISCRIMINATOR.to_vec();
                data.extend_from_slice(
                    &::borsh::to_vec(self).expect("borsh serialization of a derived type"),
                );
                data
            }
        }

        impl #impl_generics crate::parsing::InstructionParams for #name #ty_generics
            #where_clause {}

        #params_test
    }
    .into()
}
