# solana-protocols-macros

The derives behind [`solana-protocols`](https://crates.io/crates/solana-protocols).

The operating rule: **swap math is the only thing written by hand per protocol.**
Discriminators, layouts, parsers, account-meta derivation, quote bundles and
their tests all go behind a macro. Every one landed only after the hand-written
version existed first, so the derive is proven to replace it unchanged.

A hand-transcribed discriminator, offset or program id is this domain's most
expensive bug class. These compute them at expansion time instead.

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

**Discriminators**, sha256 at expansion time — never typed by a human:
`anchor_account_discriminator!`, `anchor_instruction_discriminator!`,
`anchor_event_discriminator!`

**Derives**

| Derive | Generates |
|---|---|
| `OnchainAccount` | parser + `VerifiedDecoder` + a golden-fixture test |
| `OnchainState` | the layout, from the field list — the fields *are* the layout |
| `OnchainInstruction` | instruction decode |
| `BuildAccounts` | account derivation declared per field: `input` / `key` / `pda` / `ata` |
| `QuoteState` | a quote bundle's `assemble` and `dependent_accounts` from one list |
| `AccountMetas` | `to_account_metas()` |
| `InstructionData` | `to_data()` |
| `LogParser` | program event log parsing |
| `ProtocolInstruction` | instruction enum dispatch |

`BuildAccounts` annotations are the provenance documentation for where each
account comes from, and they cannot drift from the code because they *are* the code.

## Repository

<https://github.com/0xpluto/solana-protocols>

## License

AGPL-3.0-only. Commercial licensing available — open an issue.
