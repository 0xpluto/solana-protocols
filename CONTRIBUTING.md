# Contributing

The most useful contribution is **adding a protocol**. That path is documented
below so you don't have to reverse-engineer 43k lines to find the six files.

First PR triggers the CLA bot — one comment, see [CLA.md](CLA.md). It exists so
the licence can be relaxed later without needing every contributor's permission.

## The one rule that shapes everything

**A layout is verified against the chain, never against an IDL.** IDLs go stale
and hand-transcribed discriminators are this codebase's most expensive bug class.
Every decoder is pinned by a golden fixture captured from a real mainnet account,
and every builder by replaying a real landed instruction.

A PR that adds a decoder without a fixture will be asked for one. It isn't
gatekeeping — an unpinned decoder is indistinguishable from a broken one.

## Adding a protocol

**1. Declare it.** `src/protocols/mod.rs` — add the `Protocol` variant, its
`program_id()`, its `from_program_id()` arm, and the `Protocol::ALL` entry. The
enum is deliberately closed and there are no `_` wildcards, so the compiler will
now list every site that needs you.

**2. Add the module.** `src/protocols/<name>/` with at minimum `state.rs`.
Derive `OnchainAccount` on each account struct — that generates the parser, the
`VerifiedDecoder` impl, and a golden-fixture test. Don't hand-write a parser.

**3. Capture a fixture.**

```bash
export SOLANA_RPC_URL=https://...
./scripts/capture_account.py \
    --address <ACCOUNT> --program <PROGRAM_ID> --type <StructName> \
    --out fixtures/<protocol>/<name>.json --note "what this pins"
```

It writes `expected: {}`. **Fill it in** with the fields the fixture must pin —
an empty `expected` asserts nothing and is worse than no fixture, because it
looks like coverage.

If the account has size variants, capture one fixture per size. Never guess what
a second observed length means; capture it and diff the field offsets.

**4. Swap math** (optional but where the value is). Implement `SwapMath` in
`math.rs`. Fees that key on chain state must read chain state — do not hardcode a
ladder. If exact-out isn't implemented, say so in a doc comment rather than
silently answering exact-in.

**5. Register in the quote table.** `src/quote.rs` — add a row to
`quote_protocols!`, or list the variant under `not_ported` if you're only adding
decode. That list is explicit so a new variant breaks the table at compile time
rather than defaulting quietly.

**6. Instruction builders** (optional). Annotate account derivation with
`#[derive(BuildAccounts)]` and verify by replay:

```bash
./scripts/capture_instruction.py \
    --signature <TX_SIG> --program <PROGRAM_ID> --instruction <name> \
    --out fixtures/<protocol>/ix_<name>.json
```

Account order is pinned **per instruction**, never inferred from a sibling — in
pumpfun, `buy` and `sell` order `creator_vault` and `token_program` differently.

Note on flags: a landed success proves a declared privilege set was *sufficient*,
never that it was *necessary*, so the fixture check is `>=`, not equality. The
capture script records `top_level` because jsonParsed signer/writable flags are
message-level and authoritative only there.

## Before you open the PR

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features
cargo fmt --all
```

Clippy's levels live in the workspace `[lints]` table, not the CI command line.
`deny` means currently clean — a hit is a regression. Don't add `#[allow]` to
silence one; if a lint is wrong for a real reason, say so in the PR.

## What gets rejected

- A decoder or builder with no fixture.
- A hand-transcribed discriminator, offset, or program id. Derive it.
- A `_` wildcard on `Protocol` or another producer-side enum.
- A check the caller has to remember to call. Validate inside the operation.
- A value invented to stand in for missing data — a fabricated default, a zero
  timestamp, a silent fallback. Return a typed error instead.

## Reporting a bug

A transaction signature or an account address is worth more than a description.
Both are public, and they let the exact bytes be replayed.
