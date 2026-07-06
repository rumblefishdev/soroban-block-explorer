---
id: '0344'
title: 'PERF: txdetail — eliminate ~100M-row read (whole-`accounts` FINAL joins, not an operations_appearances scan)'
type: PERF
status: completed
related_adr: []
related_tasks: ['0338', '0329', '0345']
tags:
  [priority-high, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — top bottleneck (39% of all CH rows, all timeouts).'
  - date: 2026-07-03
    status: active
    who: fmazur
    note: 'Activated — starting the txdetail full-scan fix.'
  - date: 2026-07-03
    status: completed
    who: fmazur
    note: >
      Implemented + verified on the live local API (fresh 25k-ledger DB). Premise
      corrected: the ~100M read is NOT an operations_appearances scan (that query
      already exact-seeks (ledger_sequence, transaction_id)) — it is 4× per
      statement `JOIN accounts/soroban_contracts FINAL` reading the WHOLE ~25M
      dimension to build the hash side. Rewrote the 5 detail fns to fetch surrogate
      ids + resolve via `WHERE id IN (…) LIMIT 1 BY id`; added a bloom `idx_sc_id`
      on soroban_contracts.id. Output byte-identical (SQL diff 0/0 over 5.6M rows +
      5 live-API tx). read_rows 976k→71k/request locally (~14×); scales to ~1000×
      on prod (baseline grows with the accounts table size). Resolvers later
      promoted to common/ch.rs (shared with 0345). PENDING: commit + prod
      `ALTER TABLE soroban_contracts ADD INDEX idx_sc_id + MATERIALIZE`.
---

# PERF: txdetail — eliminate ~100M-row read

## Summary

`txdetail` (GET /transactions/:hash) was the #1 bottleneck in the 0338 load test:
**~102M rows / ~7.6 GB per request, ~25 s of CH time → 7/10 requests 504** at the
API Gateway 29 s cap (39% of all CH read_rows from 7% of requests). The fix drops
the per-request read from the whole `accounts` table to a granule seek, with
byte-identical output.

## Context

Evidence: `crates/load-tests/out/2026-07-01T13-43-39Z/results.csv` (10-VU smoke).
The CH detail path is 6 statements (`transactions/queries_ch.rs`): hash→ledger
lookup, header, operations, participants, events, invocation appearances.

## Corrected diagnosis (the key finding)

The original premise ("full-scan of `operations_appearances`, add a PK/partition
filter") was **wrong**. The operations query already exact-seeks
`oa.transaction_id = ? AND oa.ledger_sequence = ?` — a precise
`(ledger_sequence, transaction_id)` prefix seek. The ~100M read is the
**dimension joins**: `fetch_operations` alone does `LEFT JOIN accounts FINAL` ×3
(src/dst/issuer) + `LEFT JOIN soroban_contracts FINAL`, and the header/participants/
events/invocations statements add more. ClickHouse builds each join's hash side
from the ENTIRE dimension table (~25M `accounts` on prod) regardless of how few
ids are needed — the same trap sibling task **0345** found across 4 more endpoints.

## Implementation

- Rewrote the 5 detail fns in `crates/api/src/transactions/queries_ch.rs`
  (`fetch_detail`, `fetch_operations`, `fetch_participants`,
  `fetch_event_appearances`, `fetch_invocation_appearances`): drop the
  `JOIN accounts/soroban_contracts (FINAL)`, select the surrogate ids, resolve
  StrKeys in Rust via `resolve_accounts` / `resolve_contracts`
  (`SELECT id, account_id FROM accounts WHERE id IN (…) LIMIT 1 BY id`). Kept the
  cheap `INNER JOIN ledgers` (PK seek) and `operations_appearances FINAL` (now
  cheap — matching granules only).
- Schema: `crates/db-clickhouse/schema/init.sql` — bloom `idx_sc_id` on
  `soroban_contracts.id` (mirror `accounts.idx_acc_id`) so contract id-IN is a
  granule seek, not a full scan.
- Resolvers later promoted to `common/ch.rs` (0345 Step 0); this module imports them.

## Acceptance Criteria

- [x] `txdetail` read_rows/request no longer scales with the accounts table
      (local 976k→71k/req ~14×; prod projection ~1000× as baseline = accounts size)
- [~] No 504s at load — expected once deployed; not re-run at 1000 VU (that is 0338/0339 scope)
- [x] Output byte-identical — SQL diff 0/0 over 5.6M op rows + 5 live-API tx (soroban / multi-op / issuer / pool / payment)
- [x] Coordinated with 0329 — untouched: the folded light list comes from `fetch_operations`; `heavy.operations` is Archive-sourced

## Implementation Notes

- Verified end-to-end on the local API (`LOCAL_API` patch, since reverted — never
  commit it): before/after JSON `jq -S` diff empty for all 5 sample tx.
- Per-statement read_rows (local): `C_operations` 724k→25k, `B_header` 251k→17k;
  the resolver seeks are ~8–24k (bloom-pruned).

## Issues Encountered

- **`transactions.source_id` is `Int64` (NOT `Nullable`)** — the first cut typed
  the raw row's `source_id` as `Option<i64>` and every request 500'd
  (`schema mismatch: Int64 as Option<T>`). Fixed to `i64`. Exactly why the live
  end-to-end run mattered. Other appearance-table surrogates ARE `Nullable(Int64)`.

## Design Decisions

### From Plan

1. **B2 / id-IN resolver over the FINAL join** — resolve surrogate→StrKey by a
   bloom-pruned `WHERE id IN (…) LIMIT 1 BY id`, mirroring the already-shipped
   list path (`resolve_source_and_closed_at`).

### Emerged

2. **Premise correction** — the bottleneck was the dimension FINAL joins, not an
   `operations_appearances` scan (which already exact-seeks). No PK/partition
   filter was needed on the fact table; the whole fix is the dimension resolution.
3. **Resolve only the immutable StrKey** — `LIMIT 1 BY id` takes an arbitrary
   ReplacingMergeTree version, which is exact ONLY because `account_id`/`contract_id`
   never change across versions (proven: `id↔strkey` 1:1, 0 violations). A mutable
   column would need `FINAL`/`argMax`.
4. **Added a schema index (`idx_sc_id`)** — `soroban_contracts` was keyed by
   `contract_id`, so `WHERE id IN (…)` full-scanned it; the bloom on `id` makes it
   a granule seek (validated locally: 14,805→6,613).
5. **Kept `operations_appearances FINAL`** — with the exact (ledger, tx) seek it
   only merges matching granules (cheap); no need to drop it + Rust-dedup here.

## Future Work

- Deploy: commit + prod `ALTER TABLE soroban_contracts ADD INDEX idx_sc_id;
ALTER TABLE … MATERIALIZE INDEX idx_sc_id` (local already done).
- The full 1000-VU re-run to confirm the 504s are gone is 0338/0339 scope.
