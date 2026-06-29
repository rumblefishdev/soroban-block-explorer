---
id: '0334'
title: 'GET /v1/assets/:id reads ~1.58 GB/request — full-dimension hash joins + unfiltered asset_enrichment GROUP BY; rewrite to two-step key-seek'
type: BUG
status: active
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

## Status: Active

**Current state:** Diagnosed + reproduced. Fix not started.

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

- [ ] /v1/assets/:id CH `read_rows`/`read_bytes` cut by orders of magnitude
      (target: no full `accounts` / `asset_enrichment` read) — measured on prod
- [ ] p50 latency of the endpoint materially down from ~997 ms (DB portion)
- [ ] Parity: response fields unchanged (name/symbol/decimals/icon/supply/holders/
      issuer/home_domain/description/home_page) — validated against current output
- [ ] List endpoint checked (shared SELECT) — fixed or noted N/A
- [ ] **Docs updated** — `endpoint-queries-clickhouse/09_get_assets_by_id.sql`
      (+ list 08 if touched) per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md)
- [ ] **API types regenerated** — N/A unless DTO shape changes (query rewrite only)

## Notes

- SEP-1 fetch is a SEPARATE, smaller lever (per-Lambda moka cache scatters across
  the fleet, same 0330 failure mode; connect 1 s / total 2 s budget). If it shows
  up after the CH query is fixed, consider a shared/edge cache or prefetching
  `description`/`home_page` into `asset_enrichment`. Spawn its own task if needed.
