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
  - date: 2026-06-16
    status: active
    who: fmazur
    note: >
      Re-diagnosed on prod (EXPLAIN + per-table Processed). Original
      read-in-order hypothesis REFUTED — transactions scan reads 0.2M
      (InReverseOrder). The 35M is the JOINs: accounts 23M (ORDER BY
      account_id, so the id-surrogate join cannot seek) + ledgers 13M
      (hash-join over full table). Fix = accounts.id index (skip-index/dict) +
      ledgers/accounts join→seek rewrite. Stopgap 50B/errors-0 now actually
      live (CH restarted during 0293 dev_read deploy).
---

# Polled /transactions (Statement A) full-partition scan blows api_reader read_rows quota

## Summary

The polled `GET /transactions` first page (Statement A in
`crates/api/src/transactions/queries_ch.rs`) reads **~35M rows per call** instead
of the intended **~2e5**. Homepage auto-refresh runs it ~430×/90min → ~15B
`read_rows`/hour, which exhausts the `api_reader` `read_rows` quota
(`api_throttle`, 10B/hr) and returns **CH Code 201 QUOTA_EXCEEDED**, 500-ing
_every_ CH endpoint (the quota is per-user, so all read paths fail once it
trips). **The 35M is NOT the partition scan** (that read-in-order path reads
~0.2M) — it is the two JOINs: `accounts` ~23M (the `id` surrogate is not the sort
key, so it cannot seek) + `ledgers` ~13M (hash-join over the full table). Fix =
an `accounts.id` index + rewrite both joins to key-seeks; then lower the quota
back toward ~1–2B.

## Status: Active

**Current state:** Root cause CONFIRMED on prod CH (2026-06-16) — see Root cause
below. The original read-in-order hypothesis is **REFUTED**: the `transactions`
scan reads ~0.2M (`InReverseOrder`, fine). The 35M is the `accounts` (23M) +
`ledgers` (13M) JOINs in the full query. Stopgap quota bump (50B / errors 0) is
now actually live (CH restarted 2026-06-16 during the 0293 dev_read deploy). Fix
not yet written — needs an `accounts.id` index (schema) + join→seek rewrite.

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

### Root cause — CONFIRMED 2026-06-16 (EXPLAIN + per-table Processed on prod CH)

The original hypothesis (tautologies / `intDiv` partition filter defeat
read-in-order) is **REFUTED**. The `transactions` scan is fine; the 35M is the
two JOINs in the full Statement A query.

Measured on prod (head partition, first page, `LIMIT 11`):

| part of the query                                        | Processed rows                                                                                                                        |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| inner `transactions` subquery                            | **0.20M** — `ReadType: InReverseOrder`; tautologies fold to constants; partition prunes to 13 parts / 1523 granules. Already correct. |
| `LEFT JOIN accounts src ON src.id = t.source_id`         | **23.1M** — full table                                                                                                                |
| `INNER JOIN ledgers l ON l.sequence = t.ledger_sequence` | **12.8M** — full table                                                                                                                |
| full Statement A                                         | **35.7M**                                                                                                                             |

Why the JOINs scan full tables (`SHOW CREATE` on prod, matches `init.sql`):

- **`accounts`**: `ENGINE = ReplacingMergeTree(last_seen_ledger) ORDER BY
account_id`. The join key is the surrogate `id`, but the sort key is
  `account_id` — so `ON src.id = …` / `WHERE id IN (…)` **cannot use the primary
  index** and scans all ~23M rows. Verified: hash-join, `IN (subquery)`, and
  literal `IN (…)` all read 23M; `... FINAL` reads even more (full merge). No
  accounts dictionary exists (`dict.xml` only has `transaction_hash_dict`).
- **`ledgers`**: `ENGINE = MergeTree PARTITION BY intDiv(sequence,500000) ORDER BY
sequence`. Sort key IS the join key, so a key-seek (`sequence IN (literals)`)
  prunes to ~11 granules. Plain MergeTree → no FINAL/dedup. The only problem is
  the hash-join building over the whole table; a seek fixes it in the query alone.

This is **systemic**: every tx-list path that projects `source_account` /
`account_id` from `accounts` by `id` has the same ~23M scan (the FINAL'd B/C
statements are worse). So the `accounts.id` index benefits all of them, not just
Statement A.

## Stopgap (already applied — NOT the fix)

- `crates/db-clickhouse/users.d/quotas.xml` edited in repo: `api_throttle`
  `read_rows` 10B→50B, `errors` 1000→0. Rationale: `errors`-as-throttle on a
  single trusted read-only service is a footgun (a 201 increments `errors`,
  self-reinforcing the lockout); `read_bytes` (1 TiB) stays the real IO guard.
- First sed-ed by hand on the box; the **single-file bind-mount inode trap** meant
  the container kept the old file, so it never loaded (the site recovered only
  because the quota window reset). **Now actually live as of 2026-06-16**: the
  repo `quotas.xml` (50B / errors 0) was deployed via ansible `--tags app` and CH
  restarted during the 0293 dev_read grant — `SELECT … FROM system.quotas`
  confirms the new caps. So `api_throttle.read_rows` is currently **50B** on prod.
- **This only hides a 35M-row scan per refresh.** Must be reverted toward
  ~1–2B once the JOINs are fixed.

## Implementation Plan

### Step 1: Confirm — DONE 2026-06-16 (hypothesis refuted)

- `EXPLAIN indexes=1, actions=1` → `ReadType: InReverseOrder` on `transactions`;
  the proposed `ledger_sequence`-range rewrite gives an identical plan and
  identical 0.20M. So read-in-order already works — **not** the bug.
- Per-table `FORMAT Null` runs pinned the 35M on the **accounts (23M) + ledgers
  (13M) JOINs**. `SHOW CREATE` confirmed `accounts ORDER BY account_id` (id not
  indexed) and `ledgers ORDER BY sequence`. See Root cause above.

### Step 2: Fix the JOINs (the actual cost)

- **ledgers — query-only (cheap):** drop the hash-join; resolve `closed_at` via a
  key-seek on the 11 page keys — `ledgers WHERE sequence IN (<literal seqs>)` (PK
  seek, no FINAL). Inline literals (keys are `i64`, no injection surface — same as
  `common::ch::fetch_tx_list_aggregates`).
- **accounts — needs a schema-level `id` index** (query rewrite alone cannot help;
  `id` is not the sort key). Pick one + verify the 23M drops:
  - (a) **bloom_filter skip-index on `id`** — `ALTER TABLE accounts ADD INDEX
idx_acc_id id TYPE bloom_filter GRANULARITY 1` + `MATERIALIZE`. `id IN (…)`
    then prunes to ~11 granules (~90K). Cheapest, no RAM. **Recommended first.**
  - (b) projection `ORDER BY id` — doubles `(id, account_id)` storage.
  - (c) dictionary `id → account_id` (like `transaction_hash_dict`) — in-memory
    O(1), zero accounts read; ~1.5 GB RAM + periodic reload. Best latency.
    Then rewrite Statement A to resolve `account_id` via the seek/`dictGet` over the
    11 keys instead of `LEFT JOIN accounts`.
- **Scope:** apply the same join→seek fix to every tx-list path projecting
  `source_account`/`account_id` from `accounts` by `id` (not just Statement A) —
  spawn a follow-up if it grows beyond Statement A.
- Re-read `queries_ch.rs` first (touched by commit `b5fa9c89`, 0284 — sealed-ledger
  cap); Statement A is at `queries_ch.rs:456-505`, joins at lines 500-501.

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

- [x] `EXPLAIN` ran — **refuted** read-in-order theory; pinned the 35M on the
      accounts (23M) + ledgers (13M) JOINs (2026-06-16)
- [x] `accounts.id` index added (`bloom_filter(0.001)` skip-index, live on prod
      via ALTER + MATERIALIZE 2026-06-16) — `id IN (…)` seeks ~1M instead of
      scanning ~23M, verified on prod CH
- [x] Statement A full query reads ≪ 35M — two-step seek rewrite deployed
      2026-06-16; prod `query_log` shows ~1.0M/poll (0.2M scan + 0.82M accounts + ~0 ledgers) vs old 35.7M, the 35M pattern gone from the window
- [ ] Regression test bounding Statement A `read_rows`
- [ ] `api_throttle.read_rows` lowered back toward ~1–2B after the fix (currently
      50B live on prod)
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
