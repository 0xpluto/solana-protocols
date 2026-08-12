//! Compile-time Anchor discriminator derivation.
//!
//! Anchor derives an account / instruction / event discriminator as the first 8
//! bytes of `sha256("<namespace>:<name>")`. These macros compute that at
//! macro-expansion and emit a `[u8; 8]` literal — so there is **no hand-typed
//! byte array to get wrong and no `[0u8; 8]` placeholder to leave un-filled**
//! (the exact bug this replaces: a placeholder `POOL_DISCRIMINATOR` that matched
//! zero real accounts). The `sha2` dependency is build-time only; the emitted
//! literal costs nothing in the release binary.
//!
//! Composable anywhere a `[u8; 8]` is: as a `const` initializer, or inside a
//! `#[state_parser(discriminator = anchor_account_discriminator!("Pool"))]`.
//!
//! Namespaces are the Anchor conventions: `account:` for on-chain account state,
//! `global:` for instructions, `event:` for CPI/log events.

use proc_macro::TokenStream;
use quote::quote;
use sha2::{Digest, Sha256};
use syn::{parse_macro_input, LitStr};

/// First 8 bytes of `sha256("<namespace>:<name>")`.
pub(crate) fn discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let digest = Sha256::digest(format!("{namespace}:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

/// Parse the single string-literal argument and emit the derived `[u8; 8]`.
fn emit(namespace: &str, input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as LitStr).value();
    let bytes = discriminator(namespace, &name).into_iter();
    quote! { [ #(#bytes),* ] }.into()
}

/// `anchor_account_discriminator!("Pool") -> [u8; 8]` (namespace `account:`).
pub fn account(input: TokenStream) -> TokenStream {
    emit("account", input)
}

/// `anchor_instruction_discriminator!("buy") -> [u8; 8]` (namespace `global:`).
pub fn instruction(input: TokenStream) -> TokenStream {
    emit("global", input)
}

/// `anchor_event_discriminator!("TradeEvent") -> [u8; 8]` (namespace `event:`).
pub fn event(input: TokenStream) -> TokenStream {
    emit("event", input)
}
