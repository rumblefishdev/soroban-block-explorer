---
id: '0614'
title: 'REFACTOR: one ledger-ingest entry for live and backfill (parse → stage → write)'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0381', '0320', '0283']
tags: [priority-medium, effort-medium, layer-indexer, layer-db]
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'From the architecture review of 2026-09-25, re-checked on develop 2026-10-04; shape decided in the review session.'
---

# One ledger-ingest entry for live and backfill

## Summary

The live indexer and the backfill runner each wire parse → prior-state reads →
stage → write by hand, and the staging input is a field-for-field copy of the
parser output. Collapse that into one entry point, behaviour unchanged, so one
new parser output stops costing ~30 edits.

## Stan teraz

- Done: shape decided (below); nothing built.
- Next: one `[structure only]` PR.
- In force: behaviour stays exactly as today, including backfill's empty
  prior-state maps (see "Prior-state reads").

## Start after (checked 2026-10-05)

Other open work edits the same files; start only once these have merged into
`develop`, then re-measure the counts above:

- `refactor/0573_event-extraction` — `persist.rs`, `stage.rs`, `tests_cross.rs`;
- `feat/0468-lp-first-deposits` — `stage.rs`, `rows.rs`;
- `feat/0553-ledger-source` — `indexer/src/handler/mod.rs` (and the 0553
  ledger-source work in `backfill-runner`).

## Context

Measured on develop 2026-10-04:

- `StageInputs` (`crates/db-clickhouse/src/persist/stage.rs:296-359`) repeats
  `ParseOutput` (`crates/indexer/src/handler/process.rs:30`) field for field;
  the 25-field literal is hand-typed 26 times (`tests_cross.rs`, `sink.rs`,
  `persist.rs`, `redecode_diff.rs`, the `*_real_e2e` tests, one example).
- `persist_ledger_clickhouse` (`persist.rs:76`) takes 23 positional slices;
  `backfill-runner/src/sink.rs:170-258` is a hand mirror whose difference
  from the live path exists only in comments ("change both together").
- Prior-state gating already produced a defect once: `sac_classic_map_needed`
  (`persist.rs:288`) forgot the pool arm and the live writer orphaned pool
  legs.
- `parse_ledger` lives in the `indexer` crate but uses only `xdr_parser`,
  `stellar_xdr` and `tracing`; `backfill-runner` depends on `indexer` only to
  call it.

## Prior-state reads — what differs between live and backfill

Before staging a ledger, the live path reads facts that arrived in EARLIER
ledgers (`persist.rs:116-130`):

- G1 — contract type by wasm hash (upload and deploy are separate ledgers);
- G9 — contract type by contract id (NFT events of a contract deployed earlier);
- 0320 — the contract's previous row (to rewrite `wasm_hash` on an upgrade).

Backfill writes partitions in parallel and out of order, so the earlier ledger
may not be in the database yet; it passes empty maps. G1/G9 are reconstructed
afterwards by `contract-type-rebuild` / `nft-reclassify`. **0320 has no such
tool since `wasm-upgrade-backfill` was removed (task 0425): a re-parse does
not restore WASM upgrades.** Enabling the reads in backfill is NOT a fix (it
would read incomplete state); the gap belongs to the batch side. Recorded here
as a fact, out of scope for this refactor.

The SAC map read is not part of this difference: both paths do it, gated by
which tables are written.

## Implementation Plan

One PR, `[structure only]`:

1. Move `parse_ledger` + `ParseOutput` from `indexer` to `xdr-parser` (own
   commit, so `git diff --color-moved` shows it clean); drop the
   `backfill-runner → indexer` dependency where it existed only for parsing.
2. Delete `StageInputs`: staging takes `&ParseOutput` plus a small struct of
   prior-state maps.
3. Replace `persist_ledger_clickhouse` with
   `ingest(&parsed, Live | Backfill { tables })` — a plain two-variant enum;
   one `match` decides which prior-state reads run. `tables` is the existing
   `TargetedTables` (all or a subset).
4. Tests build `ParseOutput::default()` and set only the slice under test;
   assertions unchanged.

## Acceptance Criteria

- [ ] `StageInputs` and `persist_ledger_clickhouse` gone; live and backfill
      call one `ingest`.
- [ ] Tests of `xdr-parser`, `db-clickhouse`, `indexer`, `backfill-runner`
      green with unchanged assertions.
- [ ] `redecode_diff` identical, and the ~1,565-ledger pool corpus produces
      identical rows on old and new code.
- [ ] Docs updated — `docs/architecture/**` indexing-pipeline overview
      (entry point), or `N/A — reason`.
