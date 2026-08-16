# solana-protocols

Decode, quote, and build Solana AMM swaps.

Layouts and discriminators are **derived at compile time** and pinned against
golden fixtures captured from mainnet — not transcribed from an IDL. IDLs go
stale, and a hand-copied discriminator is the most expensive bug class in this
domain: a decoder that silently matches nothing looks exactly like a healthy one.

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

## Where this sits

```
solana-protocols          decode · swap math · instruction building · log parsing
solana-protocols-macros   the derives behind it
solana-account-traits     cache + handler-registry contracts (zero deps)
solana-account-cache      LocalCache — slot-versioned account mirror
solana-account-ingest     account-update normalization + dependency resolution
```

Together they answer one question end to end: given a stream of account updates,
what does a swap cost right now, and what instruction executes it.

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

### Measured parse completeness

The table above says which protocols we decode *at all*. This one says how much
of a program we decode, measured against the program's own IDL — the only
denominator that cannot be chosen to flatter the answer.

<!-- BEGIN:COVERAGE -->
| protocol | instructions | accounts | events | overall |
|---|---:|---:|---:|---|
| pumpfun | 7/40 | 3/6 | 1/23 | `██░░░░░░░░` 15.9% |
| pumpswap | 6/25 | 1/7 | 2/22 | `██░░░░░░░░` 16.7% |
| meteora_dbc | 2/28 | 0/8 | 0/23 | `░░░░░░░░░░` 3.4% |
| raydium_clmm | 2/25 | 0/9 | 0/11 | `░░░░░░░░░░` 4.4% |
| **total** | | | | **24/227 = 10.6%** |
<!-- END:COVERAGE -->

These numbers are **generated** by `tests/parse_coverage.rs` and a test fails
if this section drifts from what the code actually parses, so they cannot rot
into decoration. That test is also a ratchet: coverage may only go up.

Protocols with no IDL vendored (Raydium V4, Raydium CPMM, Raydium Launchpad,
Meteora DLMM, Meteora DAMM v2) are absent rather than shown at zero — an absent row means "not
measured", never "not covered", and conflating the two would be its own lie.

Two caveats the numbers do not carry on their own.

The first is that a **stricter** parser can score *lower* here. Coverage probes
each instruction with a synthetic body, so a parser that refuses malformed
input is penalised against one that accepts anything. `create_v2` is exactly
this case: it parses real instructions and rejects zero-padding, and counts as
uncovered. Measuring against captured bodies rather than synthetic ones is the
fix.

The second. Instruction coverage is
measured by *discriminator dispatch* — our parser accepting the 8 bytes — not
by decoding a real instruction body. It is therefore an **upper bound**: at
least one instruction counted as covered here fails on live data. Treat it as
"how much of the program do we recognise", not "how much do we read correctly".


`Protocol` is a closed enum with no `_` wildcards anywhere, so adding a variant
breaks every site that needs updating — a compile-time checklist rather than a
runtime surprise. Protocols outside the cache-composed quote path are listed
explicitly under `not_ported` in the `quote_protocols!` table for the same reason.

## Known limitations

Stated rather than left to be discovered:

- `raydium_v4` and `raydium_cpmm` `calculate_swap` do not read `params.exact_out`
  and answer exact-in regardless. Don't use them for exact-out sizing.
- The PumpSwap fee-**tier** selection key is unsettled — the implemented ladder
  keys on quote-reserve SOL while on-chain tiers key on a market-cap threshold.
  See `Fees::TIER_KEY_UNSETTLED`.
- Instruction building is fixture-replay-verified for Pumpfun and PumpSwap only.

## Repository

<https://github.com/0xpluto/solana-protocols> — contributing guide, the
"adding a protocol" recipe, and the fixture-capture scripts.

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
