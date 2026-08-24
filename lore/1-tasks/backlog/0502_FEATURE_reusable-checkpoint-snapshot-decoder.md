---
id: '0502'
title: 'FEATURE: reusable checkpoint-snapshot decoder — read pubnet state, not just our stream'
type: FEATURE
status: backlog
related_adr: ['0055']
related_tasks: ['0463', '0503', '0321', '0501', '0492']
tags:
  [
    backend,
    backfill,
    xdr-parser,
    data-integrity,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0463's seed work. The seed builds a one-off decode of the
      history-archive bucket list; this task promotes it to a reusable
      capability once that decode has actually run and its costs are known.
      Deliberately NOT started before the seed — the shape should come from
      working code, not from speculation.
---

# FEATURE: reusable checkpoint-snapshot decoder

## Why this exists

Our index is a **stream of changes** since ledger floor 50,457,424. The
official SDF history archive publishes a **full state snapshot** of pubnet as
raw XDR in the checkpoint bucket list — measured at **4.54 GB gzipped across
21 files** (2026-08-17):

```
https://history.stellar.org/prd/core-live/core_live_001/.well-known/stellar-history.json
```

The difference matters more than it sounds. A stream can only tell you about
things that moved; **78.85 % of chain history predates our floor**, so an
entry that never changed since then has no row in our database at all — it
cannot be sampled, counted, or even detected by any query we can write. The
snapshot is the only source that answers **"what do we NOT have?"**.

Verified: this project has never used it. Every lore mention of "history
archive" is Galexie/captive-core catch-up configuration; no code touches
`currentBuckets` or the bucket files.

## Scope

Extract the one-off decode built for the 0463 seed into a component that can
be pointed at the archive and asked for a typed stream of ledger entries:

- fetch + verify the bucket list from the archive manifest (the archive is a
  hash-chained transport anchored to SCP — a **verified** source under the
  standing rule, unlike an unverifiable API response);
- stream-decode buckets without materialising 4.54 GB in memory;
- yield typed entries — `AccountEntry`, `TrustLineEntry`, `ContractData`,
  `LiquidityPoolEntry` — with each entry's own `lastModifiedLedgerSeq`;
- expose the checkpoint ledger, so callers can reason about staleness rather
  than guess.

**Staleness is part of the contract, not a caveat:** the snapshot is correct
at its checkpoint ledger and stale the moment it lands. Anything written from
it versions on the entry's own ledger so live parser writes win regardless of
load order — never on a window boundary (the task 0492 defect).

## What it unlocks

- **Task 0503** — exhaustive completeness audit against network state.
- **Task 0501** — trustline authorization flags ride the same entries.
- Contract balances, pool state, and any future entity that is a ledger entry.
- A repeatable drift check: "our state versus the network's", runnable on a
  schedule instead of discovered by accident.

## Built in 0463 already (extract, do not rebuild)

One module tree under `crates/backfill-runner/src/snapshot/`, split along the
seam this task needs (2026-08-24):

| module          | lines | concern                                             |
| --------------- | ----- | --------------------------------------------------- |
| `archive`       | 415   | manifest, buckets, framed XDR, per-bucket SHA-256   |
| `network_state` | 553   | classify + first-wins dedup into `NetworkState`     |
| `verdict`       | 472   | the eleven-way comparison rule, plus what it writes |
| `report`        | 433   | counters, samples, `summary.txt`                    |
| `seed`          | 534   | reads our `balances`, builds and writes corrections |

**`archive` + `network_state` are what moves to the new crate** — the first
knows nothing about our tables, the second knows nothing about comparison.
`verdict` / `report` / `seed` stay behind as
backfill-runner consumers. Measured: framed-XDR streaming at 13.5 MB peak on
4.44 GB, first-wins dedup, four-way compare, seed.

## The window discriminator — carry it into the tool's contract

Standing rule (2026-08-18), proven on the first full pass: for any
discrepancy, the entry's own `lastModifiedLedgerSeq` against our ledger floor
is the verdict —

- **before the floor** = never indexed (coverage gap, expected);
- **inside our window** = the change passed through our parser and the stored
  result is wrong: **we index incorrectly**. A defect WITH its reproduction
  ledger attached.
- **post-seed**, the first category vanishes for seeded entities, so the
  comparison becomes a pure indexing-correctness monitor.

The reusable tool must expose this split (histogram + per-bucket counts), not
leave it to each caller to reinvent. The 0463 run: 99.997% below floor, 648
in-window all explained by export-vs-snapshot skew — the parser's first
full-population pass, and it passed.

## Also fold in

- `--at-ledger`: the archive keeps every 64-ledger checkpoint's manifest, so
  a historical snapshot is choosable; today only the newest is fetched.
- **Retiring the per-window RPC bootstrap** (`bootstrap.rs`, task 0214/0492):
  it fills skeleton accounts each backfill window via per-key RPC. Once the
  0463 seed lands and verifies, the snapshot covers its purpose strictly
  better (complete, verified transport, real-ledger versions, no synthetic
  watermarks). Retire ONLY after the seed verifies on prod, as its own
  change — it is live backfill-flow behaviour, not a spent one-shot.
- ~~Deleting `snapshot-verify`~~ — **DONE 2026-08-21**, ahead of the seed
  rather than after it. The snapshot outranks RPC as a source (content-hash
  verified + enumerable, vs per-key JSON taken on trust), so a permanent RPC
  comparator earns nothing; the compare module lost `verify_command` and
  `trustline_ledger_key` (186 lines). `rpc_snapshot.rs` STAYS — `bootstrap`
  and `balance-seed` are its real consumers. Standing decoder confidence is
  the ignored network test plus 0503's audit. Note this drops the pre-seed
  independent oracle: 0463's "cross-check RPC regardless of result" AC is
  superseded and its post-seed verification now rests on coverage
  measurement + the 200-account probe.

## Acceptance criteria

- [ ] A caller can request a typed entry stream for a given entry type without
      knowing anything about bucket layout
- [ ] Memory stays bounded — measured, not assumed, on the full 4.54 GB
- [ ] The checkpoint ledger and per-entry `lastModifiedLedgerSeq` are exposed
- [ ] The floor split is exposed BY THE TOOL — histogram and per-bucket counts
      together — not left for each caller to reinvent
- [ ] `--at-ledger <n>` decodes the checkpoint asked for, not the newest:
      verified by decoding a past checkpoint and matching its manifest hashes
- [ ] Wall-clock and peak memory of a full pass are recorded in the task
- [ ] The 0463 seed is refactored onto it rather than keeping a second copy
- [ ] **Docs updated** — `docs/backfills.md` gains the snapshot as a source
- [ ] **API types** — N/A
