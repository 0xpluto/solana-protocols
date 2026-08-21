//! `#[derive(OnchainState)]` — an account's stored layout.
//!
//! # What this generates, and what it deliberately does not
//!
//! Field decoding is **not** generated. It comes from borsh, which is what wrote
//! the bytes: Anchor programs serialize account state with borsh, so a generated
//! offset walk is a second implementation of the producer's codec — it agrees
//! until it does not, and nothing says when.
//!
//! That second implementation used to live here, ~400 lines of offset
//! arithmetic, and it earned its removal: it sized arrays by element *count*
//! rather than bytes (`[u64; 16]` read as 16), it could not express a nested
//! struct until composition was bolted on, and it refused `String` and `Vec`
//! outright. borsh has none of those bugs.
//!
//! # Accounts get a prefix read, not `try_from_slice`
//!
//! Solana allocates an account at or above its data size, so the tail is
//! padding: PumpSwap pools arrive at 261, 300 and 301 bytes over one 244-byte
//! field span. Refusing trailing bytes — right for instruction data, which is
//! exactly what the sender wrote — would reject the majority of live pools here.
//! The trait's `borsh_fields` reads a prefix and leaves the remainder.
//!
//! # So what is left
//!
//! The parts borsh has no opinion about:
//!
//! * the **discriminator check**, which is identity rather than layout, and
//!   which must live inside the decode — "the registry validates upstream" was
//!   already false, and a check the caller has to remember is not a check;
//! * the **golden-fixture test**, emitted so a layout cannot ship unproven. That
//!   is the gap that let `POOL_ACCOUNT_SIZE = 301` reject most live pools while
//!   the suite stayed green.
//!
//! # Version-added fields
//!
//! `Legacy<T>` still works, and now through borsh: its impl reads the field if
//! bytes remain and yields `Absent` at EOF. That is exactly as correct as the
//! length threshold it replaces — both answer "are there bytes there", and both
//! are fooled by an old account that was allocated with padding. The equivalence
//! is pinned by `tests/borsh_agrees_on_accounts.rs` rather than argued.
//!
//! What borsh cannot know is which fields shipped *together*. It reads `Legacy`
//! fields left to right, so an account cut between two fields of one upgrade
//! decodes as `Present(x), Absent` — a version that never existed, reported as
//! fact. `#[state(added_in = "name")]` states the grouping and the derive
//! refuses a partial one, which is the guarantee the offset walk had and would
//! otherwise have left with it.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut discriminator: Option<Expr> = None;
    let mut no_discriminator = false;
    let mut fixtures: Vec<String> = Vec::new();
    let mut idl: Option<(String, String)> = None;
    let mut unverified: Option<String> = None;

    for attr in &input.attrs {
        if !attr.path().is_ident("state") {
            continue;
        }
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("discriminator") {
                discriminator = Some(m.value()?.parse()?);
            } else if m.path.is_ident("no_discriminator") {
                no_discriminator = true;
            } else if m.path.is_ident("unverified") {
                unverified = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("fixtures") {
                // `fixtures("a.json", "b.json")` — one per real on-chain size.
                let content;
                syn::parenthesized!(content in m.input);
                let parsed = content
                    .parse_terminated(<syn::LitStr as syn::parse::Parse>::parse, syn::Token![,])?;
                fixtures.extend(parsed.into_iter().map(|l| l.value()));
            } else {
                return Err(m.error(
                    "expected discriminator = …, no_discriminator, fixtures(…) or \
                     unverified = …; added_in belongs on the field it was added with",
                ));
            }
            Ok(())
        }) {
            return e.to_compile_error().into();
        }
    }

    // The field list against the program's own account layout. This was the one
    // layout surface with no such check — instruction accounts, instruction
    // arguments and event fields all had one — and the drift it hid renamed a
    // reserve field and dropped a mint.
    for attr in &input.attrs {
        if !attr.path().is_ident("idl") {
            continue;
        }
        let (mut prog, mut acct) = (None, None);
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("program") {
                prog = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("account") {
                acct = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else {
                return Err(m.error("expected program = \"…\" and account = \"…\""));
            }
            Ok(())
        }) {
            return e.to_compile_error().into();
        }
        if let (Some(p), Some(a)) = (prog, acct) {
            idl = Some((p, a));
        }
    }
    // An account layout must be checked against the program's own, or say why
    // not. This was the last of five layout surfaces with no such check, and the
    // drift it hid renamed `virtual_quote_reserves` to `virtual_sol_reserves` —
    // asserting SOL on coins quoted in USDC — and dropped a trailing mint. The
    // check existed for a day as opt-in, which is the same hole one step later:
    // a new layout that simply omits the attribute is a layout nobody compares.
    // Bytes, not just names: the IDL gate below proves the field *names* match
    // the program's, and a golden fixture proves the *layout* decodes real
    // account data. Neither substitutes for the other -- a struct can agree
    // with the IDL and still misread an account whose on-chain size the IDL
    // does not describe. Three nested structs reached this state before the
    // census found them: no fixture, no reason, nothing to grep.
    if fixtures.is_empty() && unverified.as_ref().is_none_or(|r| r.trim().len() < 12) {
        return syn::Error::new_spanned(
            name,
            "an account layout needs fixtures(\"…\") — one real account per on-chain \
             size observed — or #[state(unverified = \"why not\")]. A layout never \
             decoded from bytes the chain actually wrote is unproven however well it \
             matches the IDL",
        )
        .to_compile_error()
        .into();
    }

    if idl.is_none() && unverified.as_ref().is_none_or(|r| r.trim().len() < 12) {
        return syn::Error::new_spanned(
            name,
            "an account layout needs #[idl(program = \"…\", account = \"…\")] so its \
             field names are checked against the program's own, or \
             #[state(unverified = \"why not\")]. A layout nobody compares to the IDL \
             is a layout that drifts silently — which is exactly how a reserve field \
             ended up named for the wrong asset",
        )
        .to_compile_error()
        .into();
    }
    if let Some((prog, acct)) = &idl {
        let state_fields: Vec<crate::idl_check::EventField> = match &input.data {
            syn::Data::Struct(st) => st
                .fields
                .iter()
                .map(|f| crate::idl_check::EventField {
                    name: f
                        .ident
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
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
                .collect(),
            _ => Vec::new(),
        };
        if let Err(msg) = crate::idl_check::check_state_fields(prog, acct, &state_fields) {
            return syn::Error::new_spanned(name, msg).to_compile_error().into();
        }
    }

    if discriminator.is_some() && no_discriminator {
        return syn::Error::new_spanned(name, "pick one of discriminator = … or no_discriminator")
            .to_compile_error()
            .into();
    }
    if discriminator.is_none() && !no_discriminator {
        return syn::Error::new_spanned(
            name,
            "declare #[state(discriminator = …)] — account identity is (owner program, \
             discriminator, PDA), and `account:PoolState` alone is shared by three programs \
             in this crate. Use #[state(no_discriminator)] for a nested layout that is never \
             an account of its own.",
        )
        .to_compile_error()
        .into();
    }
    // A type with no discriminator is not independently addressable on chain, so
    // there is no account of it to capture and nothing to pin it against; its
    // width is proven by the fixture of whatever embeds it.
    if fixtures.is_empty() && !no_discriminator {
        return syn::Error::new_spanned(
            name,
            "declare #[state(fixtures(\"path/to.json\", …))] — the field list *is* the byte \
             layout, so it is only as true as the real accounts it decodes. List one fixture \
             per observed on-chain size variant: a layout that decodes the newest account can \
             still reject the majority of live ones",
        )
        .to_compile_error()
        .into();
    }

    // Fields added together in one program upgrade must arrive together.
    //
    // borsh alone cannot know this: it reads `Legacy` fields left to right and
    // yields `Absent` at EOF, so an account cut between two fields of the same
    // upgrade decodes as `Present(x), Absent` — a version that never shipped,
    // reported as fact. The old offset walk refused that, and dropping the
    // refusal when the walk went away would have been a silent loss. `added_in`
    // is what states the grouping, so it is checked here rather than documented.
    let mut groups: Vec<(String, Vec<syn::Ident>)> = Vec::new();
    if let syn::Data::Struct(st) = &input.data {
        for field in &st.fields {
            let Some(ident) = field.ident.clone() else {
                continue;
            };
            for attr in &field.attrs {
                if !attr.path().is_ident("state") {
                    continue;
                }
                let mut group: Option<String> = None;
                if let Err(e) = attr.parse_nested_meta(|m| {
                    if m.path.is_ident("added_in") {
                        let lit: syn::LitStr = m.value()?.parse()?;
                        group = Some(lit.value());
                        Ok(())
                    } else {
                        Err(m.error("expected added_in = \"upgrade name\""))
                    }
                }) {
                    return e.to_compile_error().into();
                }
                if let Some(g) = group {
                    match groups.iter_mut().find(|(name, _)| *name == g) {
                        Some((_, members)) => members.push(ident.clone()),
                        None => groups.push((g, vec![ident.clone()])),
                    }
                }
            }
        }
    }
    let group_checks: Vec<_> = groups
        .iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(name, members)| {
            let n = members.len();
            quote! {
                let present = [#(decoded.#members.is_present()),*]
                    .into_iter()
                    .filter(|p| *p)
                    .count();
                if present != 0 && present != #n {
                    return Err(
                        ::solana_protocols::parsing::state::AccountParseError::TruncatedVersion {
                            group: #name,
                            have: present,
                            need: #n,
                        },
                    );
                }
            }
        })
        .collect();

    let disc_len: usize = if no_discriminator { 0 } else { 8 };
    let disc_check = discriminator.map(|expr| {
        quote! {
            let expected: [u8; 8] = #expr;
            if data.len() < 8 {
                return Err(::solana_protocols::parsing::state::AccountParseError::TooShort {
                    len: data.len(),
                    need: 8,
                });
            }
            if data[..8] != expected {
                return Err(::solana_protocols::parsing::state::AccountParseError::Discriminator);
            }
        }
    });

    let test_mod = quote::format_ident!("__onchain_layout_{}", name.to_string().to_lowercase());
    let fixture_tests = if fixtures.is_empty() {
        quote!()
    } else {
        let first = fixtures[0].clone();
        quote! {
            /// Every declared size variant decodes, and its pinned fields match.
            ///
            /// Emitted by the derive, so a layout cannot ship unproven.
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

            /// Truncating an account past its fields must be refused, not
            /// half-read.
            ///
            /// The cut is measured rather than declared: borsh reports how many
            /// bytes the fields actually consumed, so this pins the real span
            /// instead of a constant somebody maintained. Cutting inside the
            /// padding would prove nothing — the fields are all still there.
            #[test]
            fn truncating_into_the_fields_is_refused() {
                use ::borsh::BorshDeserialize;
                use ::solana_protocols::parsing::state::OnchainState;
                let fx = crate::test_fixtures::AccountFixture::load(#first);
                let body = &fx.data()[#disc_len..];
                let mut cursor = body;
                <#name as BorshDeserialize>::deserialize(&mut cursor).expect("fixture decodes");
                let consumed = body.len() - cursor.len();
                let short = &fx.data()[..#disc_len + consumed - 1];
                assert!(
                    <#name as OnchainState>::from_account_data(short).is_err(),
                    "one byte inside the field span must not decode"
                );
            }
        }
    };

    quote! {
        impl ::solana_protocols::parsing::state::OnchainState for #name {
            /// Bytes the discriminator occupies.
            ///
            /// No longer a summed field span: borsh knows the layout and a
            /// variable-length field has no static size. Kept on the trait for
            /// callers that ask "how many bytes before the fields start".
            const REQUIRED_LEN: usize = #disc_len;

            fn from_account_data(
                data: &[u8],
            ) -> ::core::result::Result<
                Self,
                ::solana_protocols::parsing::state::AccountParseError,
            > {
                #disc_check
                let decoded = Self::borsh_fields(&data[#disc_len..])?;
                #(#group_checks)*
                Ok(decoded)
            }
        }

        #[cfg(test)]
        mod #test_mod {
            use super::*;

            #fixture_tests
        }
    }
    .into()
}
