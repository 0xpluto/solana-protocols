# solana-protocols

Decode, quote, and build Solana AMM swaps — with the account cache that makes
quoting from live state possible.

Most Solana decoding libraries stop at "here is your typed struct." This one
covers the layer above: given a stream of account updates, keep a slot-versioned
mirror of pool state, assemble the full account set a quote needs, price a swap,
and build the instruction that executes it.

Layouts and discriminators are **derived at compile time** and pinned against
golden fixtures captured from mainnet — not transcribed from an IDL. That
distinction is the point: IDLs go stale, and a hand-copied discriminator is the
single most common source of a decoder that silently matches nothing.

## Quick start

```rust
use solana_protocols::{PoolState, SwapMath, SwapParams};
use solana_protocols::parsing::state::Legacy;
use solana_protocols::pumpfun::BondingCurve;
use solana_program::pubkey::Pubkey;

// Decoding is gated on the account's 8-byte discriminator, so arbitrary
// bytes are refused rather than misparsed into a plausible-looking pool.
assert!(PoolState::detect_and_parse(&[0u8; 128]).is_none());

// Any decoded pool prices a swap through one trait, whatever the protocol.
// These are pump.fun's launch-state reserves.
let curve = BondingCurve {
    virtual_token_reserves: 1_073_000_000_000_000,
    virtual_sol_reserves: 30_000_000_000,
    real_token_reserves: 793_100_000_000_000,
    real_sol_reserves: 0,
    token_total_supply: 1_000_000_000_000_000,
    complete: false,
    creator: Pubkey::default(),
    // `Absent` means the account predates the cashback upgrade — which is
    // not the same fact as `Present(false)`.
    is_mayhem_mode: Legacy::Absent,
    is_cashback_coin: Legacy::Absent,
};

// 1 SOL in, exact-in.
let out = curve.calculate_swap(&SwapParams::buy(1_000_000_000))?;
assert!(out.amount_out > 0);
assert!(out.fee > 0);
```

This runs as a compiled doctest in CI, so it fails the build rather than
rotting quietly — the crate's own rule applied to its own documentation.

## Crates

| Crate | What it does |
|---|---|
| `solana-protocols` | Per-protocol decode, swap math, instruction building, log/event parsing |
| `solana-protocols-macros` | Derives discriminators, account layouts, account-meta derivation, quote bundles |
| `solana-account-traits` | `CacheGet`/`CacheInsert` + the handler registry — contracts only, zero deps |
| `solana-account-cache` | `LocalCache`: slot-versioned account mirror with gzip-bincode persistence |
| `solana-account-ingest` | Account-update normalization and dependency-resolving dispatch |

## Protocol coverage

| Protocol | Decode | Swap math | Cache-composed quote | Instruction build |
|---|:---:|:---:|:---:|:---:|
| Pumpfun | ✓ | ✓ | ✓ | ✓ |
| PumpSwap | ✓ | ✓ | ✓ | ✓ |
| Raydium V4 | ✓ | ✓ | — | — |
| Raydium CPMM | ✓ | ✓ | — | — |
| Raydium Launchpad | ✓ | ✓ | — | — |
| Meteora DBC | ✓ | ✓ | — | — |
| Meteora DLMM | ✓ | bin-walk¹ | — | — |
| Raydium CLMM | ✓ | — | — | — |
| Meteora DAMM v2 | ✓ | — | — | — |

¹ Standalone `quote_exact_in` / `quote_exact_out`; not yet behind the `SwapMath` trait.

Protocols outside the cache-composed quote path are declared explicitly in the
`quote_protocols!` table's `not_ported` list, so adding a `Protocol` variant
breaks that table at compile time rather than silently defaulting.

## Design notes

**Accounts are identified by (owner program, discriminator, PDA).** The 8-byte
discriminator is an Anchor convention, not a Solana fact — it collides across
programs and within a single owner. Routing accounts for a known collision is
handled explicitly rather than first-match-wins.

**Delivery is an ownership axis, not a cadence one.** `DeliveryExpectation`
splits accounts into program-owned firehose state (the subscription *is*
delivery, never fetch), program-owned config written rarely (the only fetched
class), and accounts owned by no subscribed program — vaults — which are
subscribed by pubkey through dependency resolution. Getting this wrong produces
hundreds of serial RPC round-trips per second on the hot path.

**Registering a handler cannot widen your subscription.** A handler needed only
as a dependency opts out via `subscribe_program_accounts()`, so adding a
token-program handler can't silently subscribe you to every SPL token account
on Solana.

**`Ingest::apply` is not `async`.** The constructor hands back a separate
resolver task; the hot path does one lock-free `try_send`. Removing `async` from
a signature is the strongest available guarantee that no I/O happens there.

**Version-added fields are `Legacy<T>{Present, Absent}`, not `Option<T>`** —
deliberately, because `Option`'s combinators make the absent ≡ false collapse
the ergonomic path, and "this account predates the field" is not the same fact
as "this flag is off."

## Known limitations

Stated rather than discovered:

- `raydium_v4` and `raydium_cpmm` `calculate_swap` do not read `params.exact_out`
  and answer exact-in regardless. Do not use them for exact-out sizing.
- The PumpSwap fee-**tier** selection key is unsettled — the implemented ladder
  keys on quote-reserve SOL while on-chain tiers key on a market-cap threshold.
  See `Fees::TIER_KEY_UNSETTLED`.
- Instruction building is fixture-replay-verified for Pumpfun and PumpSwap only.
  Other protocols decode and quote but have no builder.

## Status

Extracted from a private trading monorepo, where it runs in production. External
support is best-effort. Bug reports with a transaction signature or an account
dump are the most useful kind.

## License

[AGPL-3.0-only](LICENSE).

You can use, modify, and self-host this freely. If you distribute a product
containing it, or run a modified version that users interact with over a
network, the AGPL requires you to offer those users your corresponding source.

If that does not fit your product, **commercial licensing is available** — open
an issue.

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for the
"adding a protocol" recipe and the fixture-capture scripts. First PR triggers the
[CLA](CLA.md) bot; it's one comment.
