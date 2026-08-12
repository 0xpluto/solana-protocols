# solana-account-traits

Contracts for caching Solana account state and dispatching updates to handlers.
Depends on `solana-pubkey` and `thiserror` and nothing else — it sits at the
bottom so every layer above can name these types.

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

**`CacheGet` / `CacheInsert`** — the read and write halves of an account cache,
separate so a consumer can require only what it uses.

**`HandlerRegistry`** — dispatch keyed on `(owner program, discriminator)`. The
8-byte discriminator is an Anchor convention, not a Solana fact: it collides
across programs and within a single owner, so a registry that routes on owner
alone will hand accounts to the wrong handler.

**`DeliveryExpectation`** — how an account *arrives*, which is an ownership
question rather than a cadence one:

| Variant | Meaning |
|---|---|
| `Frequent` | program-owned firehose state — the subscription **is** delivery, never fetch |
| `Infrequent` | program-owned config written rarely — the only class that is fetched |
| `Dynamic` | owned by no subscribed program (vaults) — subscribe by pubkey, never fetch |

Getting this wrong is expensive in a specific way: treating `Dynamic` as fetchable
puts hundreds of serial RPC round-trips per second on the account hot path.

**`subscribe_program_accounts()`** — a handler needed only as a dependency opts
out of the program subscription. Without it, registering a token-program handler
silently subscribes you to every SPL token account on Solana.

## Repository

<https://github.com/0xpluto/solana-protocols>

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
