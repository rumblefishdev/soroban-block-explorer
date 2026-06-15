---
id: '0290'
title: 'Polled /transactions (Statement A) full-partition scan blows api_reader read_rows quota (CH Code 201)'
type: BUG
status: active
related_adr: ['0044']
related_tasks: ['0243', '0240']
tags:
  ['clickhouse', 'api', 'performance', 'quota', 'phase-launch', 'priority-high']
links: []
history:
  - date: 2026-06-15
    status: active
    who: fmazur
    note: 'Created from live prod incident — api_reader read_rows quota exhausted (CH Code 201), 500-ing every CH endpoint. Root cause traced to Statement A reading ~35M rows/poll.'
---

# Polled /transactions (Statement A) full-partition scan blows api_reader read_rows quota

## Summary

The polled `GET /transactions` first page (Statement A in
`crates/api/src/transactions/queries_ch.rs`) reads **~35M rows per call** (the
whole mainnet head partition) instead of the intended **~2e5**. Homepage
auto-refresh runs it ~430×/90min → ~15B `read_rows`/hour, which exhausts the
`api_reader` `read_rows` quota (`api_throttle`, 10B/hr) and returns **CH Code
201 QUOTA_EXCEEDED**, 500-ing _every_ CH endpoint (the quota is per-user, so all
read paths fail once it trips). Fix the query so CH reads in primary-key order
and stops at the limit, then lower the quota back toward ~1–2B.

## Status: Active

**Current state:** Diagnosed from `system.query_log` on prod CH (2026-06-15).
Site mitigated by a temporary quota bump (see Stopgap). Real fix not yet
written — pending `EXPLAIN` confirmation of the read-in-order failure.

## Context

Incident 2026-06-15 ~09:16Z: front (`sorobanscan.rumblefish.dev`) showed
"Something went wrong" on all widgets; API lambda logged
`DB error in list_*: ch: bad response: Code: 201` for ledgers / transactions /
network. Direct curl returns 401 (Cloudflare edge auth, task 0277), so the 500s
only reproduce via the browser origin.

`system.quotas_usage` showed `api_throttle` (user `api_reader`) over on
`read_rows` (10.02B / 10B) and `errors` (1051 / 1000). `system.query_log`
(`GROUP BY normalized_query_hash`, last 90 min) pinned **one** pattern as the
cause:

| query                                                  | runs     | total read_rows | avg/run   | read      |
| ------------------------------------------------------ | -------- | --------------- | --------- | --------- |
| Statement A `/transactions` (hash 8919907202405859429) | 429      | **15.19B**      | **35.4M** | 124.8 GiB |
| everything else                                        | 1–2 each | < 162M          | —         | —         |

`avg ≈ max` (35.41M ≈ 35.50M) per run = a constant structural scan, not a
user filter. The live query is the no-filter, no-cursor, `LIMIT 11` Statement A
— the homepage "Latest transactions" poll.

The code (`queries_ch.rs:454-491`) _intends_ the FINAL-drop read-in-order fast
path and its comment claims "~2e5 rows/page". Prod contradicts it: the head
partition (~35M tx) is fully scanned + sorted to return 11 rows.

### Suspected root cause (to confirm with EXPLAIN)

First-page WHERE emitted by Rust:

```sql
WHERE intDiv(ledger_sequence, 500000) = ifNull(intDiv(NULL,500000),
        (SELECT intDiv(max(sequence),500000) FROM ledgers))
  AND (NULL IS NULL OR (ledger_sequence, toInt64(application_order)) < (NULL, NULL))  -- (1)
  AND (NULL IS NULL OR source_id = NULL)                                              -- (2)
ORDER BY ledger_sequence DESC, application_order DESC
LIMIT 11
```

1. **Always-true tautologies wrap sort-key cols in functions.** Rust injects
   `NULL IS NULL OR …` on the first page instead of omitting the predicate. The
   keyset arm wraps `application_order` in `toInt64(...)` inside a comparison on
   the sort key — likely defeats the `optimize_read_in_order` / PK-range
   optimization even though it is logically constant-true.
2. **Partition filter is `intDiv(ledger_sequence,500000) = <scalar subquery>`**,
   not a `ledger_sequence` range. Gives partition prune but no PK-range
   condition, so the primary index does not cut granules.

Net: CH reads the whole pruned partition (~35M) instead of the tail.

## Stopgap (already applied — NOT the fix)

- `crates/db-clickhouse/users.d/quotas.xml` edited in repo: `api_throttle`
  `read_rows` 10B→50B, `errors` 1000→0. Rationale: `errors`-as-throttle on a
  single trusted read-only service is a footgun (a 201 increments `errors`,
  self-reinforcing the lockout); `read_bytes` (1 TiB) stays the real IO guard.
- Applied on the prod box by hand (`sed -i` on
  `/srv/app/crates/db-clickhouse/users.d/quotas.xml`). **Gotcha:** `quotas.xml`
  is a _single-file_ bind-mount; `sed -i` swaps the inode so the container kept
  reading the old file → `docker restart app-clickhouse-1` (or a directory mount)
  needed for it to take. Left as a follow-up decision at incident time.
- **This only hides a 35M-row scan per refresh.** Must be reverted toward
  ~1–2B once Statement A is fixed.

## Implementation Plan

### Step 1: Confirm the read-in-order failure

- `EXPLAIN indexes=1, actions=1` on the captured Statement A; check
  `ReadFromMergeTree (transactions)` → `ReadType` (`Default` vs
  `InReverseOrder`) and `Granules`/`Parts`.
- A/B: same query with tautologies dropped and partition filter rewritten as a
  `ledger_sequence` range; compare `read_rows`.

### Step 2: Fix Statement A in `queries_ch.rs`

- Build the WHERE conditionally — on the first page, omit the
  `NULL IS NULL OR …` keyset and `source_id` predicates entirely (only emit them
  when cursor / source filter is set).
- Replace `intDiv(ledger_sequence,500000) = …` with an explicit
  `ledger_sequence >= part_start AND ledger_sequence < part_end` range so the PK
  index engages alongside partition prune.
- Re-check the cursor (subsequent-page) path emits a clean PK-range keyset too.

### Step 3: Guard + propagate

- Integration test asserting Statement A `read_rows` is bounded (≪ partition
  size) for a first-page request — regression guard.
- Update canonical SQL `docs/architecture/database-schema/endpoint-queries-clickhouse/02_get_transactions_list.sql`
  to match.
- Lower `api_throttle` `read_rows` back toward ~1–2B in `quotas.xml`; keep
  `errors` at 0 (justified independently).

### Step 4: Harden the deploy path

- Mount `users.d/` as a directory (not per-file) so config edits/deploys don't
  hit the single-file inode-swap trap. (Spawn as backlog if out of scope here.)

## Acceptance Criteria

- [ ] `EXPLAIN` confirms (or refutes) read-in-order not applying on Statement A
- [ ] Statement A first page reads ≪ partition size (target ~2e5, not ~35M),
      verified on prod CH
- [ ] Regression test bounding Statement A `read_rows`
- [ ] `api_throttle.read_rows` lowered back toward ~1–2B after the fix
- [ ] **Docs updated** — `02_get_transactions_list.sql` (and `quotas.xml`
      comment) reflect the fixed Statement A and final caps. Per
      [ADR 0032](../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — likely `N/A` (query-internal change, no DTO /
      route / openapi schema change). Confirm at PR time; regen if any
      `crates/api/**` DTO/route changed. CI gate: `API types freshness`.

## Notes

- Per-user quota means one heavy consumer 500s all CH read paths — argues for
  per-module or per-path isolation longer term (see task 0243).
- Other query_log offenders (accounts list ~81M, LP list ~70M per run) were
  single user-initiated runs, not the incident cause, but are worth a separate
  read-cost pass if they recur.
- Skip-index follow-up on `operations_appearances(type, contract_id)` (filtered
  Statements B/C) is already noted in `02_*.sql`; out of scope here.
