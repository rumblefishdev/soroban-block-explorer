---
id: '0214'
title: 'CH writer: initial-snapshot mechanism for account state on backfill start'
type: FEATURE
status: active
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
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Activated in parallel with task 0220 (CH writer parity for
      0217 quarantine routing + 0218 SAC override). Branch
      `fix/0214_ch-initial-snapshot-account-state` cut from develop.
      Surfaces are disjoint from PR #181 (0218) and PR #182 (0219)
      — this task touches `crates/backfill-runner` + new Soroban
      RPC client integration + CH writer's account-state staging,
      none of which the other two PRs modify.

      Cross-task dependency note: Karol's pre-audit findings #1
      (classic credit assets, task 0219), #2 (home_domain backfill
      gap, this task), and #4 (is_sac pre-existing SAC, task 0218)
      share a common architectural pattern — the indexer is purely
      event-driven and entity rows that pre-date the indexed window
      arrive as skeletons. The fully-general fix is "initial-state
      RPC enrichment on first observation" (Karol's framing). This
      task delivers the RPC client + the home_domain / sequence
      enrichment path; 0218 + 0219 ship cheaper non-RPC layers for
      their respective domains that catch the common cases
      without RPC. The trio together = full coverage.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 1 (parser-side RPC client + bootstrap module + runner
      wiring) implemented on branch
      `fix/0214_ch-initial-snapshot-account-state`. Phase 2's
      incremental top-up gate landed as part of the discovery query
      (`WHERE sequence_number = 0` on the `accounts FINAL` join) —
      cheaper than a separate post-pass and self-idempotent on
      window re-runs.

      **Design decision (RPC client location)** — Option A: embed the
      Soroban RPC client inline in `crates/backfill-runner` as a
      private module (`src/rpc_snapshot.rs`). Rationale:
      - This task is the only consumer of `getLedgerEntries` today.
      - Tasks 0218 (SAC override) and 0219 (classic-credit assets)
        ship cheaper non-RPC layers; their RPC fallbacks are open
        backlog and may never need the full RPC surface.
      - Smallest blast radius; no new crate scaffolding, no public
        API surface to maintain.
      - The refactor to a shared `crates/soroban-rpc-client` crate
        (Option B) is a one-day move if a second concrete consumer
        appears.

      **Other emergent decisions:**
      - Bootstrap runs **after** the per-ledger ingest loop, not
        before (task body §"Implementation Plan §1" suggested
        before). Reason: the discovery query reads CH's
        `transaction_participants`, which on a fresh database is
        empty until ingest populates it. Running bootstrap after
        ingest lets us scan the just-populated participants table.
        Phase 2 incremental top-up gate makes the post-ingest
        position natural — a window re-run only fixes the rows
        that still need it.
      - `from_snapshot: true` provenance tag — implementation
        adopts the simpler convention of using `last_seen_ledger =
        window_start` as the snapshot watermark. RMT
        deduplication on `last_seen_ledger` lets a per-ledger
        parser emit at a higher sequence overwrite the snapshot
        row naturally. A dedicated `from_snapshot: true` boolean
        column would require a schema migration for one telemetry
        bit; the watermark convention captures the same audit
        signal (SELECT count(*) FROM accounts FINAL WHERE
        last_seen_ledger = <window_start>) without schema churn.
      - Trustline + native-balance staging — Phase 1 ships the
        `AccountEntry` snapshot path only (native XLM balance
        included). The trustline RPC pass is left as a follow-up
        note in the bootstrap module's docstring; the
        `decode_trustline_snapshot` / `rebuild_trustline_asset`
        helpers are on the public module surface ready to wire in
        when Phase 3 lands.
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

If the backfill window slides forward in time (resume after pause), subsequent windows that re-encounter the same account should NOT re-fetch state if already populated. Cheapest gate: `WHERE id NOT IN (SELECT id FROM accounts FINAL WHERE sequence_number > 0)` — i.e. only top up accounts that are still skeletons. (Both sides use the `Int64` surrogate `id` from the hub table; do not mix with the `String` `account_id` column.)

### Phase 3 — Trustline + asset aggregate hookup

Trustlines populate `account_balances_current` rows. **Once Phase 1 lands, E08/E09 gap (assets.holder_count / total_supply NULL) becomes addressable** by porting task 0194's `recompute_asset_aggregates` to CH writer's post-stage step, because the recompute formula's input (`account_balances_current` rows) will then be populated. **Port deferred** per separate decision (CH pilot + backfill done first); this task focuses on the snapshot mechanism only.

## Acceptance Criteria

- [x] New `bootstrap_account_state` step in `backfill-runner` (CH target).
      _(Phase 1 shipped in this branch:
      `crates/backfill-runner/src/{rpc_snapshot,bootstrap}.rs`,
      wired into `run::execute` via the new `--soroban-rpc-url`
      CLI flag. PG target short-circuits; CH target without the
      URL logs and skips; CH target with the URL discovers
      skeleton accounts via the `transaction_participants` JOIN
      `accounts FINAL WHERE sequence_number = 0`, fetches via
      Soroban RPC `getLedgerEntries`, and stages into `accounts` +
      `account_balances_current` with `last_seen_ledger =
    window_start` as the snapshot watermark.)_
- [ ] Empirical test: re-run 64k-ledger backfill, then `SELECT countIf(sequence_number > 0) FROM accounts FINAL` is > 50% of total rows (instead of ~0% today). (ClickHouse: use `countIf` — `count() FILTER (WHERE ...)` is Postgres-only.)
      _(Open — operational follow-up. Needs a live Soroban RPC
      endpoint + a CH instance with backfill data. The implementation
      is gated on `--soroban-rpc-url` so this AC can be verified by
      re-running an already-backfilled window with the flag set.)_
- [ ] Empirical E06 verification: account `GARDNV3Q7...` shows real `sequence_number`, `home_domain`, and at least the native XLM balance row in `account_balances_current`.
      _(Open — same operational gate as above. Decoder is
      unit-tested against the audit-pinned StrKey shape, but the
      live-RPC round trip is a follow-up.)_
- [ ] `account_balances_current` row count > 0 (today: 0 in the 64k window for most accounts).
      _(Open — same operational gate.)_
- [x] **Docs updated** — audit doc §E06 marked resolved;
      `docs/architecture/database-schema/clickhouse-pilot.md` gains
      a §State-side ingestion paragraph.
- [x] **API types regenerated** — N/A (no API change).

## Out of Scope

- Porting `recompute_asset_aggregates` (task 0194) to CH writer — separate task, deferred until pilot + backfill done.
- LP analytics (`tvl` / `volume` / `fee_revenue`) — covered by task 0199 (blocked-on-oracle).
- Real-time / live-path account state (CH pilot is read-empty; live indexing comes later).

## Notes

- Soroban RPC `getLedgerEntries` is the right primitive — protocol 22 supports it for both `AccountEntry` and `TrustLineEntry` keys.
- For large windows (~100k+ accounts), batch concurrency matters. RPC rate limits TBD; expect ~50 req/s sustained (~200 keys/req = 10k keys/s). 100k accounts → ~10s wall time. Acceptable for a one-time-per-window bootstrap.
- Provenance tag `from_snapshot: true` enables auditability — distinguishes "ingested from observed change" vs "filled from snapshot" rows.
