---
id: '0368'
title: 'Bump stellar-xdr 26→27: decode protocol-27 ledgers (indexer XDR parse failure → DLQ)'
type: BUG
status: active
related_adr: []
related_tasks: ['0367']
tags: [indexer, xdr, protocol-27, production-incident, prices]
links: []
history:
  - date: 2026-07-10
    status: active
    who: stkrolikiewicz
    note: 'Task created — prod indexer frozen on first proto27 ledger'
---

# Bump stellar-xdr 26→27: decode protocol-27 ledgers

## Summary

Production indexer froze on 2026-07-09 when pubnet activated protocol 27. The
indexer binary links `stellar-xdr 26.0.0`, which cannot decode proto27 ledger
XDR. The first proto27 ledger (63401875, closed 2026-07-09 17:18:57Z) fails to
parse; the reconcile aborts before commit and the SQS doorbell is redelivered
until it dead-letters. The whole proto27 tail is blocked behind it. Fix: bump
the workspace `stellar-xdr` pin to 27, migrate the `curr`/`next` module split
that 27 removed, rebuild, deploy, and redrive.

## Status: Active

**Current state:** Code fix complete, verified locally. Workspace `stellar-xdr`
26->27 (invalid `curr` feature dropped); `stellar_xdr::curr::` -> crate-root
import migration across 36 files / 7 crates; `SorobanCredentials` gained proto27
variants `AddressV2` + `AddressWithDelegates`, handled in
`op_source.rs::credentials_signer` (deployer attribution) with 2 new tests.
`cargo check --workspace --all-targets`, `cargo test -p xdr-parser` (287 + 2),
and `cargo fmt --check` all green. Pending: api-types regen, commit/PR, deploy,
ESM re-enable + DLQ redrive.

## Context

- Pubnet protocol 27 activated 2026-07-09 (follow-on to the galexie stall,
  task 0367). Ledger 63401875 = first proto27 ledger.
- Indexer error: `HandlerError::Parse` → `"XDR parse failed"`
  (`crates/indexer/src/handler/mod.rs:442`) on every ledger >= 63401875.
- `production-ledger-processor-dlq` ~= 7.5k dead-lettered doorbells; the ingest
  ESM was manually disabled during the fix (UUID `27553d98-...`).
- `prices-production-ledger-processor` is built from a SEPARATE repo (not this
  workspace — no `prices` crate here). It lags behind the wall (~63358072) and
  will hit the identical proto27 failure when it catches up; it needs the same
  stellar-xdr 26->27 bump applied in ITS repo (separate follow-up).
- stellar-xdr 27 removed the `curr`/`next` feature + module split; XDR types
  moved to the crate root. 65 `stellar_xdr::curr::` references across 7 crates.

## Implementation Plan

### Step 1: Pin + feature fix

Bump workspace `stellar-xdr` to `27`, drop the now-invalid `features = ["curr"]`
(done). `default = ["std"]` in 27.

### Step 2: curr -> root import migration

Replace `stellar_xdr::curr::X` -> `stellar_xdr::X` and `use stellar_xdr::curr::*`
-> `use stellar_xdr::*` across all 7 crates (xdr-parser, api, db-clickhouse,
indexer, enrichment-shared, backfill-runner, audit-harness). Resolve the
`OperationResultTr` import (downstream of the curr glob).

### Step 3: Full build

`cargo build --workspace` + tests to catch any field-level struct changes
between 26 and 27 beyond the mechanical rename.

### Step 4: API types + deploy

Regenerate api-types (`Cargo.{toml,lock}` changed). Deploy compute stack +
prices stack. Re-enable the indexer ESM. Redrive / purge the DLQ (doorbells are
content-free; one reconcile catches up the gap).

## Acceptance Criteria

- [ ] `cargo build --workspace` green against stellar-xdr 27
- [ ] Indexer decodes proto27 ledgers (63401875+) without parse error
- [ ] Follow-up: `prices-production-ledger-processor` (separate repo) bumped to
      stellar-xdr 27 before it reaches ledger 63401875
- [ ] Indexer ESM re-enabled; DLQ drained/redriven; ingestion caught up to head
- [ ] **Docs updated** — N/A unless XDR parsing responsibilities under
      `docs/architecture/**` change; verify at PR time
- [ ] **API types regenerated** — `Cargo.{toml,lock}` changed → run
      `npx nx run @rumblefish/api-types:generate` and commit the diff

## Notes

- Evidence: Horizon ledger 63401875 `protocol_version: 27`; crates.io
  stellar-xdr 27.0.0 has no `curr`/`next` feature (`default: ["std"]`);
  docs.rs 27 has no `curr` module.
- Sibling incident: task 0367 (galexie proto-upgrade hardening).
- prod: AWS acct 750702271865, eu-central-1, profile `sorobanscan`.
