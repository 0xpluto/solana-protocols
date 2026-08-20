# solana-protocols

Decode, quote, and build Solana AMM swaps.

Layouts and discriminators are **derived at compile time** and pinned against
golden fixtures captured from mainnet — not transcribed from an IDL. IDLs go
stale, and a hand-copied discriminator is the most expensive bug class in this
domain: a decoder that silently matches nothing looks exactly like a healthy one.

**Two tiers, and the difference is visible in the source.** Pumpfun and PumpSwap
are the reference implementations: everything above is true of them end to end.
The other seven protocols were written earlier, decode correctly, and have not
been migrated to that shape yet — several still carry hand-written account
layouts and transcribed constants. Copy from `protocols/pumpfun/`, not from
whichever protocol you happen to open first. Each unmigrated module says so in
its own docs.

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

| Protocol | Shape | Decode | Swap math | Cache-composed quote | Instruction build |
|---|---|:---:|:---:|:---:|:---:|
| Pumpfun | **reference** | ✓ | ✓ | ✓ | ✓ |
| PumpSwap | **reference** | ✓ | ✓ | ✓ | ✓ |
| Raydium CPMM | partial² | ✓ | ✓ | — | — |
| Raydium Launchpad | partial² | ✓ | ✓ | — | — |
| Meteora DBC | partial² | ✓ | ✓ | — | — |
| Raydium CLMM | partial² | ✓ | — | — | — |
| Raydium V4 | partial² | ✓ | ✓ | — | — |
| Meteora DLMM | legacy³ | ✓ | bin-walk¹ | — | — |
| Meteora DAMM v2 | legacy³ | ✓ | — | — | — |

¹ Standalone `quote_exact_in` / `quote_exact_out`; not yet behind the `SwapMath` trait.

² **partial** — generated instruction dispatch and a derived, identity-checked
account layout, but events and extraction are still hand-written and no IDL
verification runs.

³ **legacy** — predates the current shape entirely: hand-rolled dispatch and
hand-written account structs. Correct as far as it goes and used in production,
but not the pattern to copy.

Concretely, per protocol:

| | generated dispatch | derived account layout | IDL-verified events | trait-based extraction |
|---|:---:|:---:|:---:|:---:|
| Pumpfun / PumpSwap | ✓ | ✓ | ✓ | ✓ |
| Raydium CPMM / CLMM / Launchpad, Meteora DBC | ✓ | ✓ | — | — |
| Raydium V4 | ✓ | —⁴ | — | — |
| Meteora DLMM, Meteora DAMM v2 | — | — | — | — |

⁴ `LiquidityPool::from_account_data` is explicitly unimplemented and returns an
error rather than a guess. Instruction handling is the standard shape.

**Every protocol above handles instructions the same way**: one `…Instruction`
enum carrying `#[derive(ProtocolInstruction)]`, one file per discriminator, a
params type per discriminator deriving `InstructionData`, and account structs
deriving `AccountMetas`. Meteora DLMM and DAMM v2 are the two exceptions and are
marked as such in their own modules — they predate the derive and have not been
migrated. Everything else differs from the reference only in how far coverage
extends, never in how it is built.

### Measured parse completeness

The table above says which protocols we decode *at all*. This one says how much
of a program we decode, measured against the program's own IDL — the only
denominator that cannot be chosen to flatter the answer.

<!-- BEGIN:COVERAGE -->
| protocol | instructions | accounts | events | overall |
|---|---:|---:|---:|---|
| pumpfun | 14/40 | 3/6 | 3/23 | `███░░░░░░░` 29.0% |
| pumpswap | 7/27 | 1/7 | 3/22 | `██░░░░░░░░` 19.6% |
| meteora_dbc | 2/28 | 0/8 | 0/23 | `░░░░░░░░░░` 3.4% |
| raydium_clmm | 2/25 | 0/9 | 0/11 | `░░░░░░░░░░` 4.4% |
| **total** | | | | **35/229 = 15.3%** |
<!-- END:COVERAGE -->

These numbers are **generated** by `tests/parse_coverage.rs` and a test fails
if this section drifts from what the code actually parses, so they cannot rot
into decoration. That test is also a ratchet: coverage may only go up.

Protocols with no IDL vendored (Raydium V4, Raydium CPMM, Raydium Launchpad,
Meteora DLMM, Meteora DAMM v2) are absent rather than shown at zero — an absent row means "not
measured", never "not covered", and conflating the two would be its own lie.

One caveat the numbers do not carry on their own. Instruction coverage is
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
- Roughly 70 `collect_creator_fee` instructions per 150s of mainnet emit no
  `CollectCreatorFeeEvent`, so the payout is visible but its amount is not.
  Counted as `event_missing` rather than dropped; unexplained so far, and a
  zero-balance collect is the obvious hypothesis.
- Extraction failures are counted by (protocol, kind) via
  `chain::extract_failure_tally`. Nothing exits quietly, but a non-zero count
  means events are missing from your stream — read it rather than assuming.

## Repository

<https://github.com/0xpluto/solana-protocols> — contributing guide, the
"adding a protocol" recipe, and the fixture-capture scripts.

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
