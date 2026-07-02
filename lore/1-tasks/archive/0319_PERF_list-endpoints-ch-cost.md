---
id: '0319'
title: 'PERF: list-endpoint CH cost — PK-aligned ordering (projections) + drop FINAL/reverse-lookup; measure prod first'
type: FEATURE
status: completed
related_adr: ['0047']
related_tasks: ['0243', '0281', '0290', '0317']
tags:
  [
    'performance',
    'api',
    'clickhouse',
    'accounts',
    'liquidity-pools',
    'contracts',
    'assets',
    'priority-medium',
    'layer-api',
    'layer-backend',
  ]
links:
  - crates/api/src/accounts/queries_ch.rs
  - crates/api/src/liquidity_pools/queries_ch.rs
  - crates/api/src/contracts/queries_ch.rs
  - crates/api/src/assets/queries_ch.rs
history:
  - date: 2026-06-23
    status: backlog
    who: fmazur
    note: >
      Spawned from the list-endpoint latency investigation (~1s/request in
      prod). CORS preflight max-age was split off and fixed under 0317; this
      task covers the CH query cost + the prod measurement to confirm
      attribution.
  - date: 2026-06-23
    status: active
    who: fmazur
    note: 'Promoted to active; starting Step A (measure prod TTFB + read_rows).'
  - date: 2026-06-23
    status: completed
    who: fmazur
    note: >
      Step A measured (prod TTFB: accounts 2.23s, assets 1.35s, contracts
      0.85s, LP 0.70s) + Step B Option A shipped (query-side, no schema): drop
      the full-table reverse-id / native-balance joins → page + bloom-pruned
      key-seeks. Local read_rows: accounts 2.09M→0.52M (~4×), contracts
      375k→14.8k list-scan (~25×), assets 399k→~149k (~2.7×); output verified
      (A/B + e2e issuer/deployer resolve). 204 tests + clippy green. Detail
      paths untouched. Projection for the non-PK sort deliberately deferred
      (needs the 0281 window). Not yet committed/deployed (target: Compute).
---

# PERF: list-endpoint CH cost

## Summary

The list endpoints (`/accounts`, `/liquidity-pools`, `/contracts`, `/assets`)
take ~1s/request in prod. Structural analysis (EXPLAIN on a local CH; numbers
scale to prod) points to one recurring cause: **the list sort column is not
aligned with the CH table primary key, so each page does a full-table scan +
sort** (often plus `FINAL` and an `accounts` reverse-id lookup). This task
(a) **measures prod first** to confirm the attribution, then (b) makes the
queries index-aligned.

The CORS-preflight half of the latency (an extra `OPTIONS` round-trip per
request) was the cheap win and is already handled under [[0317]] (API Gateway
`maxAge`). This task is the backend/CH half.

## Step A — RESULTS (measured 2026-06-23)

**Prod TTFB** (warm, 3 runs each, through the edge with a free-tier JWT; TTFB ≈
total since bodies are KB, TLS ~0.08s → TTFB is essentially server time):

| Endpoint                        | prod TTFB (warm) | rank               |
| ------------------------------- | ---------------- | ------------------ |
| `/accounts?limit=20&order=desc` | **~2.23 s**      | 🔴 #1              |
| `/assets?limit=20`              | **~1.35 s**      | #2                 |
| `/contracts?limit=20`           | ~0.85 s          | #3                 |
| `/liquidity-pools?limit=20`     | ~0.70 s          | #4                 |
| `/network/stats` (ref)          | ~0.17 s          | (0291 cache works) |

**Local `read_rows` (mechanism, 25k-ledger CH; scales ~×N on prod):**

- `accounts`: **2.09M rows / 80.7 MiB / 123 ms** — `accounts FINAL` + non-PK
  `ORDER BY last_seen_ledger` full scan+sort + `account_balances_current FINAL`
  join. The dominant cost.
- `contracts`: 375k rows — the `LEFT JOIN accounts deployer` reverse-id scan
  (soroban_contracts itself is tiny).
- `liquidity-pools`: cheap on local (~0.1s) — matches the low prod TTFB.
- `assets`: 500 on local (missing enrichment tables locally); prod TTFB 1.35s
  stands — cost is the `accounts`/`soroban_contracts` reverse-id joins (its
  `ORDER BY` is already PK-aligned).

**Attribution confirmed:** the ~1s+ is server-side CH query cost (not edge /
network). Measured ranking differs from the structural guess — **accounts is the
clear priority** (2.2s), then assets; LP/contracts are sub-second. Fix order
below re-prioritised accordingly.

## Step A — original plan (measure prod)

Confirm where the ~1s actually goes before changing schema:

- Per-endpoint **TTFB** via `curl -w` (server time vs total) through the edge
  with a token; and/or
- prod `system.query_log` `read_rows` / `query_duration_ms` for the four list
  statements (rank by real cost).

Acceptance: a table of prod TTFB + read_rows per endpoint, so the fixes below
are prioritised by measured cost (not just structure).

## Step B — query-side fixes (Option A; no schema change)

Decision (2026-06-23): do the **query-side** wins only — replace the full-table
hash joins with page-then-key-seeks (same pattern as the 0317 events fix /
transactions Statement A). Sort-alignment via CH projections is **out of scope
for this task** (it needs a prod-CH schema change + the 0281 window).

1. **`/accounts`** (priority — 2.2s). Drop the `account_balances_current FINAL`
   LEFT JOIN — it builds the join side from every native balance (~1.5M of the
   2.09M local rows). Page accounts first, then resolve the native
   (`asset_type=0`) XLM balance for the page's account ids by a PK-prefix
   key-seek (`account_balances_current FINAL WHERE account_id IN (page) AND
asset_type=0`). The `accounts FINAL` + non-PK `last_seen_ledger` scan+sort
   remains (projection territory — out of scope).
2. **`/assets`** (1.35s) — resolve the `accounts` issuer + `soroban_contracts`
   reverse-id lookups by per-page key-seek instead of the joins.
3. **`/contracts`** (0.85s) — drop the `accounts deployer` reverse-id join from
   the list; resolve the deployer per-page by key-seek.

## Constraints

- **No full-table hash joins** (0317 events: a naive join builds the hash side
  from the whole table → CH Code 241). Page-then-key-seek only.
- Preserve the API sort contract and response shape **exactly**.
- **No CH schema changes** in this task (projections out of scope).

## Acceptance Criteria

- [x] Step A: prod TTFB + `read_rows` table per list endpoint (attribution
      confirmed) — see Step A RESULTS above.
- [x] `/accounts`: native-balance FINAL join replaced by a per-page key-seek —
      local read_rows **2.09M → 0.52M (~4×)**, 74→25 MiB; output identical
      (A/B: matching accounts carry the balance, others null).
- [x] `/assets` + `/contracts`: reverse-id joins replaced by per-page key-seeks
      (`accounts WHERE id IN (…)`, bloom-pruned via `idx_acc_id`). contracts
      list-scan **375k → 14.8k (~25×)** + seek; assets **399k → ~149k (~2.7×)**.
      e2e: issuer/deployer resolve correctly.
- [x] API sort contract + response shape preserved exactly (same ORDER BY; same
      DTO fields; assets detail paths untouched — only the list got a join-free
      SELECT + resolve).
- [x] No full-table hash joins introduced; no new Code 241 risk (page +
      bloom-pruned key-seeks bounded to the page).
- [x] `cargo test -p api` green (204) + clippy clean; verified against a local CH.
- [x] **Docs / API types**: `N/A` (query internals only; no DTO/route change).

## Implementation Notes

Query-side only (Option A), per-endpoint:

- **accounts** (`accounts/queries_ch.rs`): dropped the
  `account_balances_current FINAL` LEFT JOIN; page accounts, then resolve the
  native (`asset_type=0`) XLM balance by `account_balances_current FINAL WHERE
account_id IN (page) AND asset_type=0` (PK-prefix seek). New
  `AccountListBalanceRow`.
- **contracts** (`contracts/queries_ch.rs`): dropped the `accounts deployer`
  LEFT JOIN; select `sc.deployer_id` (`Nullable(Int64)`), resolve per-page via
  `accounts WHERE id IN (…)` (bloom `idx_acc_id`). New `ContractDeployerRow`.
- **assets** (`assets/queries_ch.rs`): added a list-only `ASSET_LIST_CH_SELECT`
  (= `ASSET_CH_SELECT` minus the `accounts iss` join + its 2 columns); resolve
  issuer StrKey + home_domain per-page via the bloom key-seek. New
  `AssetListChRow` / `AssetIssuerRow`. **Detail paths untouched** (they filter
  on `iss.account_id`, so they keep the joined `ASSET_CH_SELECT`).

The remaining accounts `FINAL` + non-PK `last_seen_ledger` scan+sort is **not**
addressed here (it needs a CH projection — out of scope, deliberately deferred).
Verified on a local CH (25k-ledger backfill; created an empty
`soroban_contract_metadata` locally so the assets query runs). Not yet
committed/deployed — single deploy target: **Compute**.
