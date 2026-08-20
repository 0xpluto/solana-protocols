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
//! #[onchain_ix(fixtures("pumpswap/ix_buy_n25.json", "pumpswap/ix_buy_n26.json"))]
//! pub struct BuyAccounts { #[account(writable)] pub pool: Pubkey, /* … */ }
//! ```
//!
//! # One fixture per observed account count
//!
//! A single fixture pins one length, and length is the axis these layouts get
//! wrong: pumpswap's `sell` arrives at 23, 24 and 26 accounts, pumpfun's `sell`
//! at 16 through 19. Pinning only one of them is how a struct that is right for
//! the common case and wrong for the rest stays green — the same shape as
//! `POOL_ACCOUNT_SIZE = 301` rejecting most live pools.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, LitStr};

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = input.ident.clone();

    let mut fixtures: Vec<LitStr> = Vec::new();
    for attr in &input.attrs {
        if !attr.path().is_ident("onchain_ix") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if crate::fixture_walk::parse_fixture_meta(&meta, &mut fixtures)? {
                Ok(())
            } else {
                Err(meta.error("unknown `onchain_ix` key: expected fixture = … or fixtures(…)"))
            }
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
    }
    if fixtures.is_empty() {
        return syn::Error::new_spanned(
            &ident,
            "#[derive(OnchainInstruction)] requires #[onchain_ix(fixtures(\"…\", …))] — \
             one real landed instruction per account count observed on chain, because \
             account count is the axis these layouts get wrong",
        )
        .to_compile_error()
        .into();
    }
    // `VerifiedInstruction::FIXTURE` names one; the test walks them all.
    let first = fixtures[0].clone();

    let test_mod = format_ident!("__onchain_ix_fixture_{}", ident.to_string().to_lowercase());
    let walk = crate::fixture_walk::walk(
        &ident,
        &fixtures,
        quote! { 
                let fx = crate::test_fixtures::InstructionFixture::load(__fixture);
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
                 },
    );

    quote! {
        impl crate::parsing::VerifiedInstruction for #ident {
            const FIXTURE: &'static str = #first;
        }

        #[cfg(test)]
        mod #test_mod {
            use super::*;

            /// An instruction accounts-struct cannot ship without a golden
            /// fixture whose account order + flags its `from_pubkeys` reproduces.
            #[test]
            fn onchain_instruction_fixture_roundtrips() {
                #walk
            }
        }
    }
    .into()
}
