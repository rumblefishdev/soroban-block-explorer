---
id: '0217'
title: 'PG+CH: nfts_pending quarantine table for unclassified NFT candidates (defer-then-promote)'
type: FEATURE
status: backlog
related_adr: ['0027', '0044']
related_tasks: ['0118']
tags:
  [
    layer-db,
    layer-indexer,
    postgres,
    clickhouse,
    audit-2026-05-12,
    priority-high,
    effort-medium,
  ]
milestone: 2
links:
  - docs/audits/2026-05-12-ch-pilot-endpoint-audit.md
history:
  - date: '2026-05-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0118 discussion (daily 2026-05-13). Current 0118 Phase 2
      filter permissive-inserts `Other` / NULL-classified contracts directly
      into `nfts` — designed for Phase 3 post-backfill SQL cleanup. CH pilot
      audit (2026-05-12) confirmed empirical impact: 99.4% of `nfts` rows =
      misclassified fungibles (663k / 667k; XLM SAC alone = 421k). Karol's
      parallel 0118 Patch C (parser-side whitelist, reject i128/u128 per
      SEP-50/SEP-41/OZ trait sources) shrinks the bulk at source but does
      not eliminate the `Other` / NULL bucket (pre-window WASM-less
      contracts that the classifier cannot yet decide on). This task: route
      `Other` / NULL inserts to a dedicated `nfts_pending` /
      `nft_ownership_pending` quarantine so the hot, API-facing tables stay
      clean by design. Phase 3 of 0118 redefined as drain-quarantine, not
      delete-from-hot.
---

# PG+CH: nfts_pending quarantine table for unclassified NFT candidates

## Summary

Today's persist filter (task 0118 Phase 2) writes 3-state outcomes:

| Classification       | Target                                                                |
| -------------------- | --------------------------------------------------------------------- |
| `Fungible` / `Token` | drop                                                                  |
| `Nft`                | `nfts` + `nft_ownership`                                              |
| `Other` / NULL       | `nfts` + `nft_ownership` (permissive, by design — cleaned by Phase 3) |

Permissive insert pollutes hot, API-facing tables until Phase 3 SQL runs
post-backfill. Audit-confirmed garbage ratio in production-like load:
**99.4%** (663k / 667k rows in a 15.7k-ledger sample).

**Proposed:** add `nfts_pending` + `nft_ownership_pending` quarantine
tables. Route `Other` / NULL there. Hot tables `nfts` / `nft_ownership`
only get definitive `Nft`-classified rows.

## Context

Reference paths:

- Filter logic: `crates/indexer/src/handler/persist/write.rs::resolve_nft_filter` (~1436-1500)
- Retroactive reclassify: `crates/indexer/src/handler/persist/write.rs::reclassify_contracts_from_wasm`
- PG schema: `crates/db/migrations/0005_tokens_nfts.sql`
- CH schema: `crates/db-clickhouse/schema/init.sql` (nfts + nft_ownership tables)
- Audit evidence: `docs/audits/2026-05-12-ch-pilot-endpoint-audit.md` §E15–E17

Karol's 0118 Patch C (parallel work) reduces parser emit volume by
rejecting `i128` / `u128` at `looks_like_token_id`. After Patch C,
remaining `Other` / NULL rows come from genuinely unclassifiable contracts
(no observed `wasm_upload` in window yet) — not from fungible
misclassification at the parser. Quarantine isolates that residual until
WASM observation reclassifies them.

## Design

### Schema

PG (`crates/db/migrations/NNNN_nfts_quarantine.sql`):

```sql
CREATE TABLE nfts_pending (
  -- 1:1 shape with nfts, no FK to soroban_contracts
  -- (contract may not yet be classified; FK adds churn for nothing)
  -- no extra indexes — write-heavy, read only at promotion time
  ...
);
CREATE TABLE nft_ownership_pending (
  ...
);
```

CH equivalent in `crates/db-clickhouse/schema/init.sql`:

- Same row shape as `nfts` / `nft_ownership`.
- Same partitioning key (so `OPTIMIZE TABLE ... FINAL` works post-drain).
- No materialized views — read only by promotion job.

Open: should `_pending` schemas drop indexes entirely? Suggestion: keep
only `(contract_id)` for promotion lookup; skip the rest. Refine in
implementation PR after measuring promotion-query plan.

### Persist routing (revised filter)

```text
Fungible / Token   → drop
Nft                → nfts + nft_ownership   (hot)
Other / NULL       → nfts_pending + nft_ownership_pending   (quarantine)
```

`resolve_nft_filter` returns two index vectors instead of one keep-set:
`nft_rows_hot`, `nft_rows_pending` (same for ownership). Downstream UNNEST
binds into two distinct INSERTs.

### Promotion (retroactive)

Hook into existing `reclassify_contracts_from_wasm` UPDATE path. After
UPDATE flips a `soroban_contracts.contract_type` row from `Other` to a
definitive verdict:

- `Other` → `Nft`: `INSERT INTO nfts SELECT * FROM nfts_pending WHERE contract_id = $1 ON CONFLICT DO NOTHING; DELETE FROM nfts_pending WHERE contract_id = $1` (same for ownership).
- `Other` → `Fungible` / `Token`: `DELETE FROM nfts_pending WHERE contract_id = $1` (drop, do not promote).

Run promotion in same transaction as the UPDATE so reclassification +
hot-table state stay consistent.

### Phase 3 (0118 redefined)

Post-full-backfill drain:

```sql
-- Promote any remaining Nft-confirmed rows still in pending.
INSERT INTO nfts SELECT * FROM nfts_pending p
  WHERE EXISTS (SELECT 1 FROM soroban_contracts c
                WHERE c.contract_id = p.contract_id
                  AND c.contract_type = 'nft')
  ON CONFLICT DO NOTHING;

-- Drop everything still pending (genuinely unclassifiable after full
-- backfill = treated as not-NFT, log count for audit).
TRUNCATE nfts_pending;
TRUNCATE nft_ownership_pending;
```

Reviewable in `ops/sql/` (PG) and `ops/clickhouse/` (CH).

### API impact

- `/v1/nfts*` endpoints query only `nfts` / `nft_ownership`. **Never** read
  `_pending`. Production sees only definitive classifications by design.
- Frontend §6.11 (NFT list) and §6.12 (NFT detail / transfers) become
  reliable without waiting on Phase 3.

## Implementation phases

1. **Schema migrations** — new PG migration + CH DDL diff. No data move
   yet (existing rows stay in `nfts`).
2. **Persist routing** — split `resolve_nft_filter` keep-sets. Backwards
   compat: rolling deploy safe because `_pending` tables are additive.
3. **Promotion hook** — extend `reclassify_contracts_from_wasm` to move
   rows on classification change.
4. **One-shot migration of existing `Other`-classified rows** —
   `INSERT INTO nfts_pending SELECT * FROM nfts WHERE contract_id IN (SELECT contract_id FROM soroban_contracts WHERE contract_type = 'other'); DELETE FROM nfts WHERE contract_id IN (...)`. Or: wipe + replay if pilot/staging only.
5. **Phase 3 drain script** — ops SQL committed for post-backfill use.

## Acceptance Criteria

- [ ] PG migration adds `nfts_pending` + `nft_ownership_pending` with same row shape as hot tables, minimal indexes (only `contract_id`).
- [ ] CH equivalent in `init.sql`, same partitioning key.
- [ ] `resolve_nft_filter` routes `Other` / NULL to pending tables; `Nft` to hot tables; `Fungible` / `Token` dropped (unchanged).
- [ ] Promotion hook in `reclassify_contracts_from_wasm`: `Other → Nft` promotes pending rows; `Other → Fungible / Token` deletes them.
- [ ] Integration test: ingest unclassified contract → row lands in `nfts_pending`, NOT `nfts`.
- [ ] Integration test: late `wasm_upload` reclassifies → row moves to `nfts`.
- [ ] One-shot migration script for existing `Other`-classified rows committed to `ops/sql/`.
- [ ] Phase 3 drain script (post-backfill) committed for PG + CH.
- [ ] **Docs updated** — ADR 0027 schema diagram, ADR 0044 CH pilot schema diff, `docs/architecture/database-schema/*.md` gain `_pending` paragraph.
- [ ] **API types regenerated** — N/A (no API change; endpoints still read `nfts` / `nft_ownership`).

## Out of Scope

- Patch C (parser whitelist, reject i128/u128) — handled in 0118 by Karol in parallel.
- Phase 3 cleanup execution — task closes once schema + routing + promotion + scripts ship; actual drain runs operationally after full Soroban-era backfill.
- Frontend changes — `/v1/nfts*` endpoints are already wired to hot tables; quarantine is transparent.

## Notes / Open Questions

- **Retention:** keep `_pending` rows forever (audit trail) or TRUNCATE after Phase 3? Lean: TRUNCATE after Phase 3 drain to reclaim space; log row counts before truncate for audit.
- **CH `OPTIMIZE TABLE`:** post-drain `OPTIMIZE TABLE nfts FINAL` + `OPTIMIZE TABLE nfts_pending FINAL` in runbook.
- **Telemetry:** add a metric `nfts_pending_row_count{contract_classification}` for ops visibility — surfaces how much quarantine accumulates.
- **Conflict with 0118 Patch C:** none. Patch C reduces emit; quarantine handles the residual. Both ship independently.
- **Why not FK on `_pending.contract_id`?** Rows arrive before classification — FK to `soroban_contracts.contract_id` could be valid, but the lookup churn on every insert is wasted work when the row is by-design transient. Document explicitly.
