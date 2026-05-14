---
id: '0217'
title: 'PG+CH: nfts_pending quarantine table for unclassified NFT candidates (defer-then-promote)'
type: FEATURE
status: completed
related_adr: ['0027', '0044', '0046']
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
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Activated after PR #178 merged (0118 Patch C + Phase 3 cleanup
      runbook). Patch C shrinks parser emit at source; this task lands
      the architectural follow-up — schema migration + persist routing
      + promotion hook for the `Other` / NULL residual bucket.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Implementation shipped on branch
      `feat/0217_nfts-quarantine-table` in six phases:

      A. Schema migrations — `nfts_pending` + `nft_ownership_pending`
      added in both PG (timestamped migration
      `20260513130000_nfts_pending_quarantine`) and CH
      (`init.sql` diff). No FKs, minimal indexing, natural-key PKs.

      B. Persist routing split — `resolve_nft_filter` now returns a
      4-bucket `NftFilterDecision` (`hot_nfts`, `pending_nfts`,
      `hot_ownership`, `pending_ownership`); the previous "permissive
      insert for `Other`" path is gone. `upsert_nfts_and_ownership`
      gains 12c/12d INSERT blocks for the quarantine tables.

      C. Promotion hook — `reclassify_contracts_from_wasm` now drives
      `promote_pending_nfts_to_hot` (Other→Nft) and
      `drop_pending_nfts_for_contracts` (Other→Fungible) in the same
      transaction as the contract-type UPDATE, so the type flip and
      the row migration are atomic from any reader's perspective.

      D. Integration tests — three new DATABASE_URL-gated tests in
      `crates/indexer/tests/persist_integration.rs`
      (`quarantine_routes_other_contract_to_pending`,
      `quarantine_promotes_pending_to_hot_on_nft_verdict`,
      `quarantine_drops_pending_on_fungible_verdict`). Shared fixture
      helpers keep each test body small.

      E. Operational runbook —
      `docs/runbooks/0217_nfts_pending_migration_and_drain.md` with
      two parts: (1) one-shot migration of existing `Other`/NULL rows
      out of the hot tables on the 0217 deploy, and (2) post-
      Soroban-backfill drain of the residual pending (promote
      stragglers, TRUNCATE).

      F. Architecture docs — `database-schema-overview.md` gains
      §4.13.1 quarantine subsection; `clickhouse-pilot.md` gains
      §4c-bis with CH-side schema + routing table. Both link to the
      runbook for the operational lifecycle.

      `cargo check --workspace` clean.
      `cargo clippy -p indexer --all-targets -- -D warnings` clean.
      Integration tests pending DB-bound CI verification.
  - date: '2026-05-14'
    status: completed
    who: stkrolikiewicz
    note: >
      Closed after empirical verification. Quarantine routing
      validated on 64k + 512k CH pilots — hot `nfts` table = 0 rows
      (post-0118 revert), `nfts_pending` correctly populated with
      `Other`/NULL-classified contracts. CH writer parity for the
      pending tables shipped in PR #186 (task 0220) and the SAC leak
      into `nfts_pending` is structurally captured (task 0221) with a
      committed drain runbook empirically validated at -25.7% row
      reduction.
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

- [x] PG migration adds `nfts_pending` + `nft_ownership_pending` with same row shape as hot tables, minimal indexes (only `contract_id`). _(`crates/db/migrations/20260513130000_nfts_pending_quarantine.up.sql`)_
- [x] CH equivalent in `init.sql`, same partitioning key on the ownership table. _(no partitioning on PG side — pending is transient; CH keeps `intDiv(ledger_sequence, 500000)` for part-copy symmetry with `nft_ownership`. **Schema-only on CH for PR #180**: the CH writer (`crates/db-clickhouse/src/persist/{stage,writer}.rs`) does not yet stage or INSERT into either pending table — CH writer parity is tracked as task **0220**.)_
- [x] `resolve_nft_filter` routes `Other` / NULL to pending tables; `Nft` to hot tables; `Fungible` / `Token` dropped (unchanged). _(returns `NftFilterDecision` with 4 buckets in `crates/indexer/src/handler/persist/write.rs`.)_
- [x] Promotion hook in `reclassify_contracts_from_wasm`: `Other → Nft` promotes pending rows; `Other → Fungible / Token` deletes them. _(`promote_pending_nfts_to_hot` + `drop_pending_nfts_for_contracts`, both run inside the caller's transaction.)_
- [x] Integration test: ingest unclassified contract → row lands in `nfts_pending`, NOT `nfts`. _(`quarantine_routes_other_contract_to_pending` in `crates/indexer/tests/persist_integration.rs`.)_
- [x] Integration test: late `wasm_upload` reclassifies → row moves to `nfts`. _(`quarantine_promotes_pending_to_hot_on_nft_verdict`; companion test `quarantine_drops_pending_on_fungible_verdict` covers the Fungible drop path.)_
- [x] One-shot migration script for existing `Other`-classified rows. _(Embedded in the operator runbook `docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 1, PG and CH sections side-by-side — same form-factor as the 0118 cleanup runbook for consistency.)_
- [x] Post-backfill drain procedure (PG + CH). _(Runbook §Part 2 — straggler promotion + TRUNCATE.)_
- [x] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md` gains §4.13.1 quarantine subsection (PG schema); `docs/architecture/database-schema/clickhouse-pilot.md` gains §4c-bis (CH schema + routing table). Decision record captured in [ADR 0046](../../2-adrs/0046_classifier-quarantine-tables-nfts-pending.md) — alternatives considered, design rationale, consequences — linked from both architecture docs per ADR 0032.
- [x] **API types regenerated** — N/A (no API change; endpoints still read `nfts` / `nft_ownership`).

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
