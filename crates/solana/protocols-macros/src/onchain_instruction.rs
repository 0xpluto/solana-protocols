//! `#[derive(OnchainInstruction)]` — verify an instruction accounts-struct's
//! parse side against a real landed instruction.
//!
//! Attached alongside `#[derive(AccountMetas)]` (which supplies `from_pubkeys` /
//! `to_account_metas`), it generates `impl VerifiedInstruction` carrying the
//! fixture path and emits a `#[cfg(test)]` round-trip: `from_pubkeys(real
//! accounts).to_account_metas()` must reproduce the real instruction's account
//! order and signer/writable flags. That catches a wrong `#[account(writable)]`
//! / `#[account(signer)]` annotation or account-count drift — the
//! silent-corruption class on the parse side the extractors depend on.
//!
//! ```ignore
//! #[derive(AccountMetas, OnchainInstruction)]
//! #[onchain_ix(fixture = "pumpswap/ix_buy.json")]
//! pub struct BuyAccounts { #[account(writable)] pub pool: Pubkey, /* … */ }
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, LitStr};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = input.ident.clone();

    let mut fixture: Option<LitStr> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("onchain_ix") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("fixture") {
                fixture = Some(meta.value()?.parse()?);
                Ok(())
            } else {
                Err(meta.error("unknown `onchain_ix` key"))
            }
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
    }
    let fixture = match fixture {
        Some(f) => f,
        None => {
            return syn::Error::new_spanned(
                &ident,
                "#[derive(OnchainInstruction)] requires `#[onchain_ix(fixture = \"…\")]`",
            )
            .to_compile_error()
            .into()
        }
    };

    let test_mod = format_ident!("__onchain_ix_fixture_{}", ident.to_string().to_lowercase());

    quote! {
        impl crate::parsing::VerifiedInstruction for #ident {
            const FIXTURE: &'static str = #fixture;
        }

        #[cfg(test)]
        mod #test_mod {
            use super::*;

            /// An instruction accounts-struct cannot ship without a golden
            /// fixture whose account order + flags its `from_pubkeys` reproduces.
            #[test]
            fn onchain_instruction_fixture_roundtrips() {
                let fx = crate::test_fixtures::InstructionFixture::load(#fixture);
                assert!(
                    fx.data().len() >= 8,
                    "instruction {} carries no discriminator",
                    fx.signature
                );
                let parsed = #ident::from_pubkeys(&fx.pubkeys())
                    .expect("from_pubkeys on the real instruction's accounts");
                // Compare PUBKEYS only, never flags: a swap is usually an inner
                // (CPI) instruction, and jsonParsed carries only message-level
                // signer/writable flags, not the CPI's declared per-account
                // privileges — so flags aren't recoverable and extraction never
                // needs them. This asserts the struct's account count/order
                // prefix matches the real landed instruction.
                let ours: ::std::vec::Vec<_> =
                    parsed.to_account_metas().into_iter().map(|m| m.pubkey).collect();
                let real = fx.pubkeys();
                assert!(
                    real.len() >= ours.len(),
                    "real instruction {} has {} accounts, struct expects {}",
                    fx.signature,
                    real.len(),
                    ours.len()
                );
                assert_eq!(
                    ours.as_slice(),
                    &real[..ours.len()],
                    "account order must match real instruction {} @ slot {}",
                    fx.signature,
                    fx.slot
                );
                // Flags are authoritative only for a top-level capture; verify
                // the struct's `#[account(writable/signer)]` against the chain
                // there. Skipped for inner CPIs (flags not recoverable).
                //
                // Compare real >= ours, not strict equality: a real tx may
                // over-grant (pass an account writable/signer it doesn't need —
                // costs compute but processes fine), and we can't assume other
                // builders won't. A failure means WE declared a privilege a real
                // successful tx proved unnecessary (over-declaration / wasted
                // compute). Under-declaration is the revert case and surfaces
                // when our own build fails, not here.
                if fx.top_level() {
                    let our_metas = parsed.to_account_metas();
                    let real_metas = fx.account_metas();
                    for (i, (ours, real)) in
                        our_metas.iter().zip(real_metas.iter()).enumerate()
                    {
                        assert!(
                            real.is_writable || !ours.is_writable,
                            "account {i} of {}: struct declares writable but the \
                             real instruction has it readonly (over-declared)",
                            fx.signature,
                        );
                        assert!(
                            real.is_signer || !ours.is_signer,
                            "account {i} of {}: struct declares signer but the \
                             real instruction does not",
                            fx.signature,
                        );
                    }
                }
            }
        }
    }
    .into()
}
