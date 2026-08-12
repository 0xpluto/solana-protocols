# solana-account-cache

`LocalCache` — a slot-versioned in-memory mirror of on-chain account state.

This is what makes quoting from live state possible: decoding an account gives
you a struct, but pricing a swap needs the *bundle* of accounts a pool depends
on, all present, all at a known slot.

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

## What's here

**Slot versioning.** Every entry carries the slot it was written at, so a reader
can tell fresh state from stale rather than assuming. Arrival order can't corrupt
the cache — a later slot wins.

**Bundle assembly.** A quote's input is a set of accounts, not one. `assemble`
returns `Result<_, NotQuotable>`, which distinguishes *no bundle exists for this
protocol* from *one does, but an account it reads isn't cached at this slot* —
two very different answers that an `Option` would collapse into `None`. There is
deliberately no `from_parts` constructor that would let a caller hand-build a
partial bundle and skip the check.

**Persistence.** gzip + bincode snapshot and restore, so a restart doesn't begin
blind.

## Repository

<https://github.com/0xpluto/solana-protocols>

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
