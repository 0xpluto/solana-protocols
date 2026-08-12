# solana-account-ingest

Normalizes RPC/gRPC account updates and drives them through the handler registry.

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

**The hot path does no I/O, and the type system says so.** `Ingest::apply` is
not `async`. The constructor hands back a separate dependency resolver for the
caller to spawn; `apply` itself does one lock-free `try_send`. Removing `async`
from a signature is the strongest available guarantee that nothing blocks there.

**Dependency resolution.** A handler can declare that an account it decoded
depends on others — a PumpSwap pool needs its two vault token accounts before it
can be quoted — and the resolver subscribes to them dynamically. Those vaults are
owned by the token programs, not the AMM, so a pure program subscription never
delivers them.

**Failure is not permanent.** A failed fetch un-sees the pubkey, so a transient
RPC error can't blacklist a config account for the process lifetime. Deduplication
lives in the resolver rather than the hot path: duplicates travel the channel and
die on a set lookup, because the hot path is the part that must never contend.

## Features

`rpc` — the concrete `solana-client`-backed `AccountFetcher`. Off by default so
consumers with their own transport don't pull it in.

## Repository

<https://github.com/0xpluto/solana-protocols>

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
