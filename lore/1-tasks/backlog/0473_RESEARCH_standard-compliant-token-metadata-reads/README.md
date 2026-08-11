---
id: '0473'
title: 'RESEARCH: standard-compliant token metadata reads (SEP-41 interface vs storage-peeking, RPC drain design)'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0472', '0340', '0297']
tags:
  [
    research,
    parser,
    enrichment,
    assets,
    metadata,
    priority-medium,
    effort-medium,
  ]
links:
  - 'https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md'
history:
  - date: '2026-08-11'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0472 finding 11b after the standard-compliance question
      unravelled it. The parser fix for the third storage layout was
      implemented and chain-verified on the 0472 branch, then pulled off it
      and parked here as patches/ — nothing ships ahead of the policy
      decision. The drain, the negative marker and the compliance policy all
      live here.
---

# RESEARCH: how should the explorer read token metadata, by the standard?

## The question

SEP-41 defines token metadata as an INTERFACE — `name()`, `symbol()`,
`decimals()` — and says nothing about storage. Our parser reads metadata by
peeking at instance-storage layouts instead, which is a per-library
convention, not the standard. Three layouts are now known, found by three
separate investigations (0297, 0340, 0472). The list is unbounded by
construction. What is the policy, and what closes the gap for rows already
missed?

## Established facts (all measured 2026-08-11, prod + mainnet RPC)

- **SEP-41 specifies functions only.** No storage shape. The only
  standard-guaranteed read is executing the interface — which is what RPC
  `simulateTransaction` does.
- **The three known layouts are library conventions:**
  `Symbol("METADATA")` struct = `soroban-token-sdk` (`METADATA_KEY` in its
  `metadata.rs`); `Vec([Symbol("Metadata")])` = OpenZeppelin NFT
  `NFTStorageKey`; `Vec([Symbol("Name"|"Symbol"|"Decimals")])` sibling
  entries = hand-rolled `DataKey` enums (mainnet `CDQLKMI4…GPXT` /
  `CBNMAFRH…A4MY`).
- **527 type-3 assets have no metadata row.** RPC probe of 14 random ones:
  8 full SEP-41 tokens (we drop their names), 2 tokens without the metadata
  functions (nothing to read — honest "?"), 4 with partial/no token
  interface (likely not tokens at all; classifier question). Extrapolated
  ~300 recoverable — the drain itself is the real measurement.
- **Volume is a trickle:** 39 new fungible contracts in the last ~121k
  ledgers (≈7 days). Parser layouts cover ~87% at zero cost; the RPC path
  would see a few calls per week plus the one-shot 527 backlog.
- **Local simulation is possible but starts from zero here:** no WASM blobs
  stored (even the 0465 decompiler fetches WASM from public RPC per
  request), no raw instance storage persisted, and `symbol()` may read
  arbitrary state or cross-call (proxy tokens) — only a full-ledger-state
  executor is universally correct. That means an RPC node or own
  captive-core infra.
- **RPC-in-enrichment precedent exists:** 0340 `nft-collection-name` drain
  in `backfill-enrichment-runner`; `WasmCodeFetcher` in the API.

## Prepared, not shipped — `patches/`

The parser change for the third layout was implemented, tested and then
PULLED OFF the 0472 branch (decision: nothing ships ahead of the policy
call in this task). It sits here as git patches, ready for `git am` once
decision 1 lands on "freeze-plus-drain":

- `patches/0001-…third-on-chain-token-metadata.patch` —
  `extract_token_metadata` folds sibling `Name`/`Symbol`/`Decimals`
  entries, packed struct stays authoritative; regression test decodes the
  actual `CDQLKMI4…GPXT` instance XDR; 348 xdr-parser tests green.
- `patches/0002-…document-the-third-metadata-layout.patch` — module-header
  docs.

Even applied, the parser is forward-path only: a dormant contract is
recovered on its next instance write, which may be never. That remainder —
plus the already-missed 527 — is what the drain design covers.

## To decide

1. **Policy:** freeze the parser at the three layouts (fast path for known
   libraries) + RPC drain as the standard-compliant authority for the rest?
   Or retire storage-peeking entirely in favour of RPC? The freeze rule is
   the current lean — it answers "we don't adapt to custom implementations"
   without paying ~300 assets of regression while the drain is built.
2. **Drain home:** `backfill-enrichment-runner` subcommand (0340 pattern —
   one-shot backlog + occasional re-run) vs a standing enrichment worker.
   Lean: subcommand; the trickle does not justify a worker.
3. **Negative marker:** ~3/5 of candidates have no metadata functions; a
   "still missing" predicate would re-hammer RPC every run.
   `soroban_contract_metadata` has no column for "probed, absent" — sentinel
   row (empty fields, cheap, no migration) vs schema migration
   (`probed_at`-style, cleaner semantics). Undecided.
4. **Non-token rows:** the probe found contracts in `assets` with no
   `balance`/`transfer` at all — a classifier (0309-family) question, not a
   metadata one. Scope or spawn.

## Acceptance criteria

- [ ] Policy decision recorded (freeze-plus-drain vs RPC-only), with the
      parser rule written into `token_metadata.rs` docs
- [ ] Drain designed + implemented per decisions 2–3; run against the 527
- [ ] Real split (recoverable vs nameless vs non-token) measured by the
      drain, replacing the 14-sample extrapolation
- [ ] Negative marker chosen and applied; re-run is a no-op for absent ones
- [ ] Non-token-rows question dispatched (scoped here or spawned)
- [ ] Docs: database-schema-overview (`soroban_contract_metadata` readers /
      writers) + backfills doc if a new subcommand lands
