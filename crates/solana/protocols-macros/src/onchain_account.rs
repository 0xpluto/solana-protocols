//! `#[derive(OnchainAccount)]` — generate a verified account-state handler.
//!
//! Attached to a handler struct, it generates the mechanical, forgery-prone half
//! of a `solana_account_traits::ProtocolStateHandler` (program id, discriminator,
//! deserialize) plus the proof that the hand-written form lacked:
//!
//! * the discriminator is **compile-time-derived** from the Anchor account name
//!   (`anchor_account = "Pool"`) or an explicit pinned const
//!   (`discriminator_const = POOL_DISCRIMINATOR`) — never a hand-typed `[0u8; 8]`;
//! * an `impl solana_account_traits::VerifiedDecoder` carrying the fixture path; and
//! * a `#[cfg(test)]` that loads the golden fixture and decodes it, so a handler
//!   cannot ship without a chain-verified layout test.
//!
//! The author still writes the struct, its `new()`, and the cache-specific
//! `StorageHandler::apply` — only the parse-only half is generated.
//!
//! ```ignore
//! #[derive(OnchainAccount)]
//! #[onchain(program = PROGRAM_ID, state = PumpSwapPool,
//!           anchor_account = "Pool", decode = PumpSwapPool::from_account_data,
//!           fixture = "pumpswap/pool_v3_full_301.json")]
//! pub struct PumpSwapPoolHandler;
//! ```
//!
//! `decode` must be `fn(&[u8]) -> Result<State, E>` where `E: ToString` (map a
//! fallible Option decoder through a tiny `-> Result<_, &'static str>` wrapper).

use heck::ToShoutySnakeCase;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, LitStr, Path};

#[derive(Default)]
struct Args {
    program: Option<Path>,
    state: Option<Path>,
    decode: Option<Path>,
    fixture: Option<LitStr>,
    anchor_account: Option<LitStr>,
    discriminator_const: Option<Path>,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let handler = input.ident.clone();

    let mut args = Args::default();
    for attr in &input.attrs {
        if !attr.path().is_ident("onchain") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("program") {
                args.program = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("state") {
                args.state = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("decode") {
                args.decode = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("fixture") {
                args.fixture = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("anchor_account") {
                args.anchor_account = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("discriminator_const") {
                args.discriminator_const = Some(meta.value()?.parse()?);
            } else {
                return Err(meta.error("unknown `onchain` key"));
            }
            Ok(())
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
    }

    macro_rules! required {
        ($opt:expr, $name:literal) => {
            match $opt {
                Some(v) => v,
                None => {
                    return syn::Error::new_spanned(
                        &handler,
                        concat!("#[derive(OnchainAccount)] requires `", $name, " = …`"),
                    )
                    .to_compile_error()
                    .into()
                }
            }
        };
    }
    let program = required!(args.program, "program");
    let state = required!(args.state, "state");
    let decode = required!(args.decode, "decode");
    let fixture = required!(args.fixture, "fixture");

    // Discriminator: derived from the Anchor name, or an explicit pinned const.
    // Exactly one of the two must be given.
    let (disc_const_def, disc_ref) = match (&args.anchor_account, &args.discriminator_const) {
        (Some(name), None) => {
            let bytes = crate::discriminator::discriminator("account", &name.value());
            let bytes = bytes.iter();
            let disc_ident = format_ident!("{}_DERIVED_DISCRIMINATOR", handler.to_string().to_shouty_snake_case());
            (
                quote! { const #disc_ident: [u8; 8] = [ #(#bytes),* ]; },
                quote! { #disc_ident },
            )
        }
        (None, Some(path)) => (quote! {}, quote! { #path }),
        _ => {
            return syn::Error::new_spanned(
                &handler,
                "#[derive(OnchainAccount)] needs exactly one of `anchor_account = \"Name\"` or `discriminator_const = PATH`",
            )
            .to_compile_error()
            .into()
        }
    };

    let test_mod = format_ident!("__onchain_fixture_{}", handler.to_string().to_lowercase());

    quote! {
        #disc_const_def

        impl ::solana_account_traits::ProtocolStateHandler for #handler {
            type State = #state;

            fn program_id(&self) -> ::solana_program::pubkey::Pubkey {
                #program
            }

            fn discriminator(&self) -> ::core::option::Option<&'static [u8]> {
                ::core::option::Option::Some(&#disc_ref)
            }

            fn deserialize(
                &self,
                data: &[u8],
            ) -> ::core::result::Result<Self::State, ::solana_account_traits::HandlerError> {
                #decode(data).map_err(|e| ::solana_account_traits::HandlerError::Deserialize {
                    data_len: data.len(),
                    reason: ::std::string::ToString::to_string(&e),
                })
            }
        }

        impl ::solana_account_traits::VerifiedDecoder for #handler {
            const FIXTURE: &'static str = #fixture;
        }

        #[cfg(test)]
        mod #test_mod {
            use super::*;

            /// A handler cannot ship without a golden fixture that decodes: this
            /// is emitted by the derive, so it can't be forgotten.
            #[test]
            fn onchain_fixture_decodes() {
                let fx = crate::test_fixtures::AccountFixture::load(#fixture);
                #decode(fx.data()).expect("golden fixture must decode");
            }
        }
    }
    .into()
}
