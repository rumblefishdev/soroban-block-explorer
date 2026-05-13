---
id: '0214'
title: 'CH writer: initial-snapshot mechanism for account state on backfill start'
type: FEATURE
status: backlog
related_adr: ['0044']
related_tasks: ['0119', '0194', '0204', '0205', '0207']
tags:
  [
    layer-db,
    layer-indexer,
    clickhouse,
    audit-2026-05-12,
    priority-high,
    effort-medium,
  ]
milestone: 2
links:
  - docs/audits/2026-05-12-ch-pilot-endpoint-audit.md
history:
  - date: '2026-05-12'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from CH pilot endpoint audit
      (2026-05-12-ch-pilot-endpoint-audit.md §E06). Audit confirmed CH
      `accounts` rows are skeletons (sequence_number=0, home_domain=null,
      0 balances in account_balances_current) for accounts that appear
      only as transaction_participants in the backfill window — because
      parser `extract_account_states()` emits state rows conditionally
      on observed LedgerEntry changes, and most participants never have
      their AccountEntry / TrustLineEntry updated within a 64k-ledger
      window. Same root cause for E08/E09 state-NULL fields on assets
      (also state-side, also depends on LedgerEntry).
---

# CH writer: initial-snapshot mechanism for account state on backfill start

## Summary

CH writer's `accounts` table is populated correctly when the parser
observes `LedgerEntryAccount` / `LedgerEntryTrustLine` changes in the
ingested ledger range. But for accounts that appear only as
`transaction_participants` (driver creates a skeleton row), the actual
state fields (`sequence_number`, `home_domain`, `account_balances_current`
rows) remain empty unless that account's LedgerEntry happens to update
inside the same window. In short backfill windows this means most
accounts persist as skeletons.

Add an **initial-snapshot mechanism** that runs at backfill start:
for each window's start ledger, fetch live state via Soroban RPC
`getLedgerEntries` for all accounts referenced in the window's
`transaction_participants`, write the resulting state rows.

## Context

CH `accounts` schema:

```
id Int64, account_id String, first_seen_ledger Int64,
last_seen_ledger Int64, sequence_number Int64,
home_domain LowCardinality(Nullable(String))
```

Staging path (`crates/db-clickhouse/src/persist/stage.rs:213-`) populates
`sequence_number` / `home_domain` from `ExtractedAccountState` overlay
when present, else 0/None defaults.

Parser `extract_account_states()` (`crates/xdr-parser/src/state.rs`)
emits one `ExtractedAccountState` per account whose LedgerEntryAccount
changed in the ledger. PG indexer relies on the same parser path —
trustlines are populated by task 0119, supply/holder aggregates by
task 0194's `recompute_asset_aggregates`. CH writer **inherits the
parser-emit-on-change pattern but lacks any equivalent of an initial
snapshot for previously-existing state.**

Empirical example from audit:

- account `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55` — CH stores `seqnum=0, home_domain=null, 0 balances`. Horizon shows `seqnum=148e15, home_domain="ultracapital.xyz", 12 861 XLM`. Account is fully alive on mainnet; the CH skeleton is purely a window-boundary artefact.

## Implementation Plan

### Phase 1 — Initial snapshot at backfill start

1. **New runner step** `bootstrap_account_state` invoked once per backfill window before the per-ledger ingest loop.
2. **Account discovery**: scan the window's `transaction_participants` + `accounts` (any G-address that the parser is going to reference) for the distinct G-StrKey set. ~10-100k accounts per 64k-ledger window observed.
3. **RPC fetch**: batch `getLedgerEntries` calls to Soroban RPC (~200 ledger keys per call). Read `LedgerKey::Account(...)` for each G-StrKey at the window's start ledger.
4. **Decode**: extract `seqnum`, `home_domain`, native balance from each `AccountEntry`. For trustlines, fetch via a follow-up `getLedgerEntries` call with `LedgerKey::Trustline(...)` keys derived from the same accounts.
5. **Stage**: feed into the same `AccountRow` / `AccountBalanceRow` staging path the parser uses. Tag with a `from_snapshot: true` provenance for telemetry.

### Phase 2 — Incremental top-up on later windows

If the backfill window slides forward in time (resume after pause), subsequent windows that re-encounter the same account should NOT re-fetch state if already populated. Cheapest gate: `WHERE account_id NOT IN (SELECT id FROM accounts FINAL WHERE sequence_number > 0)` — i.e. only top up accounts that are still skeletons.

### Phase 3 — Trustline + asset aggregate hookup

Trustlines populate `account_balances_current` rows. **Once Phase 1 lands, E08/E09 gap (assets.holder_count / total_supply NULL) becomes addressable** by porting task 0194's `recompute_asset_aggregates` to CH writer's post-stage step, because the recompute formula's input (`account_balances_current` rows) will then be populated. **Port deferred** per separate decision (CH pilot + backfill done first); this task focuses on the snapshot mechanism only.

## Acceptance Criteria

- [ ] New `bootstrap_account_state` step in `backfill-runner` (CH target).
- [ ] Empirical test: re-run 64k-ledger backfill, then `SELECT count() FILTER (WHERE sequence_number > 0)` on `accounts` is > 50% of total rows (instead of ~0% today).
- [ ] Empirical E06 verification: account `GARDNV3Q7...` shows real `sequence_number`, `home_domain`, and at least the native XLM balance row in `account_balances_current`.
- [ ] `account_balances_current` row count > 0 (today: 0 in the 64k window for most accounts).
- [ ] **Docs updated** — audit doc §E06 marked resolved; `docs/architecture/database-schema/clickhouse-pilot.md` gains a §State-side ingestion paragraph.
- [ ] **API types regenerated** — N/A (no API change).

## Out of Scope

- Porting `recompute_asset_aggregates` (task 0194) to CH writer — separate task, deferred until pilot + backfill done.
- LP analytics (`tvl` / `volume` / `fee_revenue`) — covered by task 0199 (blocked-on-oracle).
- Real-time / live-path account state (CH pilot is read-empty; live indexing comes later).

## Notes

- Soroban RPC `getLedgerEntries` is the right primitive — protocol 22 supports it for both `AccountEntry` and `TrustLineEntry` keys.
- For large windows (~100k+ accounts), batch concurrency matters. RPC rate limits TBD; expect ~50 req/s sustained (~200 keys/req = 10k keys/s). 100k accounts → ~10s wall time. Acceptable for a one-time-per-window bootstrap.
- Provenance tag `from_snapshot: true` enables auditability — distinguishes "ingested from observed change" vs "filled from snapshot" rows.
