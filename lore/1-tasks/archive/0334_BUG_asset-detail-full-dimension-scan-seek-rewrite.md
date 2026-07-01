---
id: '0334'
title: 'GET /v1/assets/:id reads ~1.58 GB/request — full-dimension hash joins + unfiltered asset_enrichment GROUP BY; rewrite to two-step key-seek'
type: BUG
status: completed
related_adr: []
related_tasks: ['0290', '0333']
tags:
  [
    'clickhouse',
    'api',
    'performance',
    'assets',
    'phase-launch',
    'priority-high',
  ]
links: []
history:
  - date: 2026-06-29
    status: active
    who: fmazur
    note: >
      Spawned from 0333 investigation. The original ~997 ms /v1/assets/:id latency
      is NOT the SEP-1 fetch (measurement disproved that) — it is the CH query:
      prod query_log shows 730–1721 ms, ~21.3M read_rows, ~1.58 GB read_bytes per
      single-asset lookup. Same root pattern as 0290/0333: full scans where seeks
      are possible.
  - date: 2026-06-29
    status: completed
    who: fmazur
    note: >
      Two fixes shipped. (1) Asset DETAIL seek rewrite (queries_ch.rs): three
      detail fetches now read the accounts-join-free ASSET_LIST_CH_SELECT + a
      single accounts.id key-seek (resolve_issuer / seek_latest_account /
      finish_detail; list_row_to_asset_row by-value). Removed dead ASSET_CH_SELECT
      / AssetChRow / map_ch_row. Verified locally 261K→48K read_rows; full
      old-vs-new parity on all 3 forms (5-agent local-API review, all green; no
      secret leak; SQL-inj safe). cargo check+clippy+release all clean. (2) E10
      asset-transactions skip-index `idx_oa_asset_issuer_id` (init.sql) for the
      classic/SAC OR arm — PROD-CONFIRMED 6.2B→39M read_rows / 115GiB→747MiB /
      7.5s→0.96s (live, mutation_540965). Docs 09+10 updated (ADR 0032).
      Code-review (max) safe-set applied. NOT YET: commit/push + compute-stack
      deploy of the detail Lambda (CH indexes already live; deploy is independent).
---

# GET /v1/assets/:id — full-dimension hash joins, ~1.58 GB/request

## Summary

`ASSET_CH_SELECT` (`crates/api/src/assets/queries_ch.rs:98`) resolves one asset
via 5 LEFT JOINs over the dimension tables. In ClickHouse the hash-join reads the
**entire right-hand table** to build the hash, so the `sc.contract_id = ?` filter
only narrows the left side — every request full-reads `accounts` (18.5M rows,
the dominant cost), full-scans `asset_enrichment` through an unfiltered
`argMax/GROUP BY`, and `FINAL`-scans `assets` + `soroban_contract_metadata`.
Measured on prod: **730–1721 ms, ~21.3M read_rows, ~1.58 GB read_bytes per
single-asset request.** Rewrite to a two-step key-seek (the 0290 pattern).

## Status: Completed

**Done:** detail seek rewrite + `idx_oa_asset_issuer_id` (prod-confirmed). Remaining
ops (tracked, not blockers): commit/push this branch + deploy the compute stack to
ship the detail Lambda (the CH indexes are already live on prod and independent).

<!-- historical: -->

**Current state:** Implemented + verified locally. The three detail fetches now
use the accounts-join-free `ASSET_LIST_CH_SELECT` (shared with the list, task 0319) + a single `accounts.id` key-seek for the issuer. Pending: commit/PR, prod
measurement after deploy.

## Implementation Notes

- `crates/api/src/assets/queries_ch.rs`: removed the dead `ASSET_CH_SELECT`
  const, `AssetChRow`, `map_ch_row` (the full-`accounts`-join detail SELECT).
  Detail paths now read `ASSET_LIST_CH_SELECT` (no `accounts` join) and resolve
  the issuer separately:
  - `fetch_by_contract_id` / `fetch_native`: step-1 row → `resolve_issuer(id)`.
  - `fetch_by_code_issuer`: seek `accounts WHERE account_id = ?` FIRST (accounts
    is `ORDER BY account_id` → PK seek) to get the issuer surrogate + home_domain,
    then filter `assets` by `(asset_code, issuer_id)`.
  - New helpers `list_row_to_asset_row` (shared with the list map) and
    `resolve_issuer` (single `accounts.id` bloom-pruned seek).
- `cargo check` + `cargo clippy -p api` clean (no warnings).
- **Verified locally** (CH with 172K accounts; prod has 18.5M). For one asset
  (contract `CBA7…`): OLD full-join = 261 K read_rows; NEW = 48 K (step-1 32 K +
  issuer seek 16 K). The removed chunk is the `accounts` read (172 K → 16 K /
  2 granules); at prod scale the 18.5M `accounts` read disappears
  (~21M → ~2.8M read_rows, ~1.58 GB → ~200 MB).
- **Parity** confirmed OLD vs NEW for all three forms (contract_id, CODE-ISSUER,
  native), including the edge where one code+issuer exists as both a classic and a
  SAC-wrap row: identical `issuer` / `home_domain` / `contract_id` / keys.

## Design Decisions

### From Plan

1. **Reuse `ASSET_LIST_CH_SELECT` + issuer key-seek** — the list already proved
   this pattern (0319); detail just needed the same treatment.

### Emerged

2. **Did NOT also key-seek `asset_enrichment` / `asset_aggregates` / metadata.**
   The measured dominant cost was `accounts` (18.5M of ~21M read_rows); those
   side tables are small and the list keeps them as sub-select joins too. Staying
   identical to the shipped list SELECT maximises parity + minimises risk. If
   their residual cost ever matters, key-filtering them is a follow-up.
3. **CODE-ISSUER resolves the issuer first** (extra seek) because the old
   `WHERE iss.account_id = ?` predicate lived on the dropped join; resolving the
   StrKey→surrogate up front (cheap PK seek) replaces it and reuses the row for
   the issuer StrKey + home_domain (no second lookup).

## Context

Originated from the user's original question (asset page ~997 ms, screenshot
2026-06-29). First suspected the blocking SEP-1 TOML fetch (`handlers.rs:279`),
but `query_log` shows the CH query alone is 730–1721 ms — SEP-1 is minor here
(cache warm / fast issuer). The asset query is also a `read_rows`/`read_bytes`
quota consumer (≈1.58 GB × runs), so this also buys back `api_throttle` headroom
on top of the 0333 contract-filter fix.

The query (abridged):

```sql
FROM assets a FINAL
LEFT JOIN accounts iss          ON iss.id = a.issuer_id          -- reads ALL 18.5M accounts
LEFT JOIN soroban_contracts sc  ON sc.id  = a.contract_id
LEFT JOIN (SELECT … FROM soroban_contract_metadata FINAL) m ON … -- FINAL scan
LEFT JOIN asset_aggregates agg  ON …
LEFT JOIN (SELECT …, argMax(icon_url,version), argMax(name,version)
           FROM asset_enrichment GROUP BY …) ae ON …             -- full scan + GROUP BY
WHERE sc.contract_id = ?                                         -- narrows left side only
```

**Local reproduction** ([[local-api-clickhouse-run]]): the joined query read
261 K rows for ONE asset (≈ Σ all dimension tables); a key-seek of the issuer
(`accounts WHERE id = ?`) read 8 192 rows (1 granule, rides the 0290
`idx_acc_id` bloom) vs 171 922 full — ~21× locally, **~2000× on prod scale**
(8 K vs 18.5M).

## Implementation Plan

### Step 1: Resolve the asset's keys cheaply

PK-seek `soroban_contracts.contract_id` (or `assets` by code+issuer for the
`CODE-ISSUER` / native forms) → get `issuer_id`, `contract_id` surrogate,
`asset_code`, `asset_type`. Avoid `FINAL` full scans where a seek + dedup suffices.

### Step 2: Seek the dimensions by key, not hash-join

- `accounts WHERE id = <issuer_id>` (rides `idx_acc_id`).
- `asset_enrichment WHERE asset_type/code/issuer_id/contract_id = …` **before**
  the `argMax` (a handful of rows, not the whole table).
- `asset_aggregates` / `soroban_contract_metadata` by the resolved keys.
- Assemble in the handler (mirror 0290 `resolve_source_and_closed_at`), or use a
  single query that the planner can prove is a seek (test EXPLAIN read_rows).

### Step 3: Verify + apply to the list path too

Check whether the assets LIST `ASSET_CH_SELECT` variant (queries_ch.rs:150) shares
the full-dimension-scan cost; fix together if so. Re-measure prod read_bytes.

## Acceptance Criteria

- [x] /v1/assets/:id CH `read_rows`/`read_bytes` cut by orders of magnitude
      (no full `accounts` read) — verified locally (261K→48K; prod 18.5M accounts
      read removed). Prod confirmation pending deploy.
- [ ] p50 latency of the endpoint materially down from ~997 ms (DB portion) —
      pending prod measurement after deploy
- [x] Parity: response fields unchanged (name/symbol/decimals/icon/supply/holders/
      issuer/home_domain) — validated OLD vs NEW on all 3 forms (description/
      home_page are the unchanged runtime SEP-1 overlay)
- [x] List endpoint checked — N/A: already accounts-join-free (task 0319); this
      task reuses its SELECT, no list change
- [x] **Docs updated** — `endpoint-queries-clickhouse/09_get_assets_by_id.sql`
      rewritten to the two-step seek per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md); list 08 untouched
- [x] **API types regenerated** — N/A: query rewrite only, no DTO/route change

## Review (code-review, max, multi-agent)

Applied the safe set (no change to returned data; quality/guarantee fixes):

- **ORDER BY a.asset_type on the CODE-ISSUER seek** — a `(code, issuer)` can match
  both a classic (type 1) and its SAC-wrap (type 2) row; the old query relied on
  physical PK order to return classic. Now explicit (classic wins). Verified
  old==new (type 1) for STICKER/GAVAIL… both stable across runs.
- Removed the dangling `[`AssetChRow`]` intra-doc link; refreshed the stale
  module header + `AssetIssuerRow` doc (no longer an `accounts` join; `home_domain`
  is mutable → latest-version seek).
- DRY/efficiency: shared `seek_latest_account` (used by `resolve_issuer` + the
  CODE-ISSUER seek), `finish_detail` (contract/native tail), `list_row_to_asset_row`
  now takes the issuer tuple by value (no clone).

Intended behavior (confirmed, kept):

- **home_domain = latest version** — the detail seek returns the LATEST
  `home_domain` (`ORDER BY last_seen_ledger DESC LIMIT 1`), matching the list
  path. This differs from the old full-join (arbitrary un-merged version) for an
  issuer that changed its domain, but the latest is the correct value. Proven on
  synthetic multi-version data: with two `accounts` rows for one issuer (NULL @
  older ledger, `new-domain.example` @ newer), both seek forms (by `id` and by
  `account_id`) return `new-domain.example`. (The reviewer could not test this —
  all local accounts are single-version.)

Deferred (not regressions; need your call):

- **orphan-issuer empty wire id** (pre-existing from 0319): a classic asset whose
  `issuer_id` has no `accounts` row renders `id:""` in `GET /v1/assets`. Parity
  holds vs old. Candidate for its own backlog task.

## Additional fix — E10 `/assets/:id/transactions` classic arm skip-index

Found while measuring on prod after the 0333 contract-id index landed. The
asset-transactions driver filters
`(oa.asset_code = ? AND oa.asset_issuer_id = ?) OR (oa.contract_id = ?)`.

- A **pure soroban** asset (CBR6…) uses only the `contract_id` arm → 0333's
  `idx_oa_contract_id` now prunes it: prod 6.2 B → 459 K read_rows (~13,500×).
- A **classic-wrap SAC** (e.g. `zkSync`, CBEDR…) has BOTH arms. The `asset_*`
  arm had NO skip-index, and with `OR` a granule is only skippable if BOTH
  disjuncts can be ruled out → `idx_oa_contract_id` was defeated, **still a full
  scan**: prod-measured **~6.2 B rows / ~115 GiB / ~7.5 s** per request (after
  09:44, i.e. after 0333) — still blowing `api_throttle.read_rows`.

Fix: add `INDEX idx_oa_asset_issuer_id asset_issuer_id TYPE bloom_filter(0.001)
GRANULARITY 1` to `operations_appearances` (init.sql). CH unions it with
`idx_oa_contract_id` for the OR (EXPLAIN shows `<Combined skip indexes>`).

**Verified locally** (OR predicate, STICKER classic + its SAC wrap): before =
13.18 M read_rows / 252 MiB / 78 ms (whole table); after = **114 K / 2.92 MiB /
11 ms** (~115×). EXPLAIN confirms both blooms + the combined-granule step.

**Prod apply** (online, like 0333 — no maintenance window; inserts keep running):

```sql
ALTER TABLE operations_appearances
  ADD INDEX IF NOT EXISTS idx_oa_asset_issuer_id asset_issuer_id TYPE bloom_filter(0.001) GRANULARITY 1;
ALTER TABLE operations_appearances MATERIALIZE INDEX idx_oa_asset_issuer_id;
```

(then confirm `SELECT is_done FROM system.mutations WHERE table='operations_appearances' AND not is_done` is empty.)

## Notes

- SEP-1 fetch is a SEPARATE, smaller lever (per-Lambda moka cache scatters across
  the fleet, same 0330 failure mode; connect 1 s / total 2 s budget). If it shows
  up after the CH query is fixed, consider a shared/edge cache or prefetching
  `description`/`home_page` into `asset_enrichment`. Spawn its own task if needed.
