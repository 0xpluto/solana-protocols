//! Typed handler traits.

use solana_pubkey::Pubkey;

use super::types::{HandleResult, HandlerError};

/// Parse-only half of a protocol account handler.
///
/// Describes which on-chain accounts this handler owns (via `program_id`
/// and `discriminator` for Anchor accounts, or `matches_account` for the
/// fallback path) and how to turn raw account bytes into a typed state.
///
/// Deliberately cache-agnostic — protocol crates implementing this trait
/// don't need to depend on any storage backend.
pub trait ProtocolStateHandler: Send + Sync + 'static {
    /// Deserialized state produced from an account's raw bytes.
    type State: Send + 'static;

    /// Program ID that owns accounts this handler handles.
    fn program_id(&self) -> Pubkey;

    /// Leading discriminator bytes this handler's accounts start with, if any.
    ///
    /// Exactly **8** bytes puts the handler on the O(1) dispatch path, keyed on
    /// `(program_id, data[..8])` — the Anchor convention.
    ///
    /// Any *other* length routes it to the per-program fallback list, where the
    /// declared bytes are still checked as a prefix before
    /// [`matches_account`](Self::matches_account) is consulted. Non-Anchor
    /// programs live here: Raydium V5's single leading byte, for example.
    ///
    /// `None` means the account carries no discriminator at all — the handler
    /// goes to the fallback list and `matches_account` becomes the *entire*
    /// test, so a handler returning `None` must override it.
    fn discriminator(&self) -> Option<&'static [u8]>;

    /// Fallback-path predicate: does this handler own `data`?
    ///
    /// Only reached for handlers whose discriminator is not exactly 8 bytes,
    /// and only after any declared prefix has already matched.
    ///
    /// The default `true` is correct for two cases and dangerous in a third:
    ///
    /// * **8-byte discriminator** — never called; the registry dispatches by key.
    /// * **Short/long prefix** — the prefix check has already run, so `true`
    ///   means "the prefix is sufficient", which is usually right.
    /// * **`discriminator() == None`** — nothing has been checked, so the
    ///   default claims *every account owned by the program*. Override it with
    ///   a size or sentinel-byte check; a discriminator-less handler that keeps
    ///   the default is a catch-all, and will shadow every handler registered
    ///   after it for the same program.
    fn matches_account(&self, _data: &[u8]) -> bool {
        true
    }

    /// Parse `data` into the handler's state type. Receives the full
    /// account bytes including any discriminator prefix.
    fn deserialize(&self, data: &[u8]) -> Result<Self::State, HandlerError>;

    /// Whether this handler's [`program_id`](Self::program_id) should drive a
    /// **wholesale owner subscription** — "send me every account this program
    /// owns".
    ///
    /// `true` for protocol handlers (a pool program owns thousands of accounts,
    /// all of interest). `false` for handlers that decode accounts we subscribe
    /// to **individually**, by pubkey: a token-account handler is registered
    /// under the SPL Token program, and subscribing to every account it owns
    /// would be subscribing to most of Solana. Those accounts arrive because a
    /// protocol handler named them in
    /// [`accounts_to_subscribe`](super::types::HandleResult::accounts_to_subscribe).
    ///
    /// Consumers build subscription filters from
    /// [`HandlerRegistry::subscribable_program_ids`](super::registry::HandlerRegistry::subscribable_program_ids),
    /// so opting out here is what keeps a dependency-only handler from
    /// accidentally widening the firehose.
    fn subscribe_program_accounts(&self) -> bool {
        true
    }
}

/// Marker for a handler produced by `#[derive(OnchainAccount)]`.
///
/// A `VerifiedDecoder` carries two guarantees the derive enforces and a
/// hand-written handler does not:
///
/// * its discriminator is **compile-time-derived** (`anchor_account_discriminator!`)
///   or an explicit pinned constant — never a hand-typed placeholder like the
///   `[0u8; 8]` that once matched zero real PumpSwap pools; and
/// * it has a **golden on-chain fixture** ([`FIXTURE`](Self::FIXTURE)) that the
///   derive turns into a `#[test]` decoding real account bytes.
///
/// The registry deliberately does **not** require this bound on `register` —
/// non-Anchor handlers (single-byte prefixes, size-match, runtime-configured)
/// are a legitimate, fixtureless category the fallback path exists to serve.
/// Completeness is enforced by a test asserting every *Anchor* account handler
/// is a `VerifiedDecoder`, which keeps `register` open while still making "a new
/// Anchor decoder skipped its proof" a build failure.
///
/// Not meant to be implemented by hand; the derive is the only intended path.
pub trait VerifiedDecoder: ProtocolStateHandler {
    /// Path of the golden fixture (relative to the consuming crate's
    /// `fixtures/` dir) that pins this decoder's layout against the chain.
    const FIXTURE: &'static str;
}

/// Cache-side half: apply a parsed state observation to `cache` and report
/// any dependencies discovered in the process.
///
/// Generic over the cache type `C` so implementors only bind `C:
/// CacheInsert<K, V>` (and/or `CacheGet<K, V>`) for whichever fields they
/// touch — they never import a concrete cache.
pub trait StorageHandler<C>: ProtocolStateHandler {
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError>;
}
