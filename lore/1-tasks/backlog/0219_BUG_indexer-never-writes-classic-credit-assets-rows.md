---
id: '0219'
title: 'BUG: indexer never writes classic-credit assets entity rows'
type: BUG
status: backlog
related_adr: ['0027', '0030', '0031', '0043']
related_tasks: ['0118', '0119', '0188', '0191', '0194', '0195', '0214', '0218']
tags:
  [
    layer-indexer,
    layer-parser,
    postgres,
    clickhouse,
    pre-audit-2026-05-13,
    priority-high,
    effort-medium,
  ]
milestone: 2
links:
  - crates/xdr-parser/src/state.rs
  - crates/indexer/src/handler/persist/write.rs
history:
  - date: '2026-05-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from Karol's 2026-05-13 pre-audit Bug #1 (finding doc
      `docs/audits/2026-05-13-pre-audit-finding-classic-credit-asset-row-missing.md`
      on the 0197 branch, will land in the 0197 PR merge).

      Karol's empirical pin from a local pubnet backfill of ledgers
      `51000000..51000300` (301 ledgers): the indexer writes 17 186
      `account_balances_current` rows for classic credits across 3 904
      distinct `(asset_code, issuer)` pairs (AQUA, VELO, USDC, XRP,
      NUNA, TIDE, BTC, SSLX, ETH, RIO, …), but the `assets` table
      contains exactly **1 row** — the native XLM placeholder. Every
      classic credit's entity row is missing.

      Root cause: `crates/xdr-parser/src/state.rs::detect_assets` only
      emits `ExtractedAsset` for `Sac` deployments and
      `Fungible`-classified WASM deployments. `TokenAssetType::ClassicCredit`
      is not produced anywhere in the production code path; the
      `crates/indexer/src/handler/persist/write.rs::upsert_assets_classic_like`
      branch fires only in tests where fixtures hand-inject
      `ClassicCredit` rows. Native XLM has the same shape problem at
      the singleton level.

      Downstream impact (Karol's table, verified):

      - Task 0188 (SEP-1 detail) — `description` / `home_page`
        runtime fetch works (no DB dependency on assets row).
      - Task 0191 (icon enrichment) — UPDATE on `(code, issuer_id)`
        no-ops because no row exists.
      - Task 0194 (`holder_count` + `total_supply` recompute) —
        same; UPDATE matches zero rows.
      - Task 0195 §2a (sep1_assets name) — same; no row to update.
      - API `GET /v1/assets` returns only native + SAC + Soroban-
        fungible; classic credits invisible.

      Karol's audit 0197 is **paused** until this is fixed — running
      the field-allocation coverage matrix now would surface every
      classic credit as FAIL with the same root cause, producing
      noise rather than signal.

      Critical coupling with task 0218 (SAC forward-derive): 0218's
      helper consumes `ExtractedAsset` to derive SAC contract_ids;
      with this bug live, 0218 has zero inputs in production
      (integration test passes only because the fixture manually
      injects a `ClassicCredit` asset). 0219 unblocks 0218's
      production effect.
---

# BUG: indexer never writes classic-credit assets entity rows

## Summary

`detect_assets` (`crates/xdr-parser/src/state.rs`) emits
`ExtractedAsset` from two paths only:

1. **SAC deployments** → `TokenAssetType::Sac`.
2. **WASM deployments classifying as `Fungible`** →
   `TokenAssetType::Soroban`.

Neither path produces `TokenAssetType::ClassicCredit` or
`TokenAssetType::Native`. The indexer correctly stages classic-credit
balance / trustline data into `account_balances_current` via task
0119's path, but the entity-row producer is missing. Every
classic-credit row in `assets` would have to come from a manual
seed or a test fixture; in production the table holds at most the
native XLM placeholder.

## Empirical confirmation (Karol's pin, 2026-05-13)

Local pubnet backfill of ledgers `51000000..51000300`:

| Metric                                                              | Value          |
| ------------------------------------------------------------------- | -------------- |
| Ledgers indexed                                                     | 301            |
| `operations_appearances` type=6 (`ChangeTrust`)                     | 5 499          |
| `account_balances_current` rows (non-native)                        | 17 186         |
| `account_balances_current` distinct `(asset_code, issuer_id)` pairs | 3 904          |
| `assets` rows                                                       | **1** (native) |

Expected `assets` count: ≥ 3 905 (native + 3 904 distinct credits).
Observed: 1. Gap = 3 904.

```sql
-- Reproducible queries from Karol's finding doc
SELECT asset_type, COUNT(*) FROM assets GROUP BY asset_type;
SELECT COUNT(*) FROM account_balances_current WHERE asset_code IS NOT NULL;
SELECT COUNT(DISTINCT (asset_code, issuer_id))
  FROM account_balances_current WHERE asset_code IS NOT NULL;
```

## Fix strategy

Extend `detect_assets` (or add a sibling producer) to emit
`ExtractedAsset { asset_type: ClassicCredit, asset_code,
issuer_address, … }` for every distinct `(code, issuer)` pair
observed in trustline `LedgerEntryChange`s within the ledger. Plus
a native singleton bootstrap (idempotent UPSERT of the asset_type=0
row on every ledger or on first ingest).

Sources of `(code, issuer)` pairs available to the parser today
without RPC calls:

- `trustline` entries in `ExtractedLedgerEntryChange` (current
  carrier of classic-credit identity in our pipeline, already used
  by 0119's balance staging).
- `ChangeTrust` op surface (type=6) — explicit asset reference.
- `payment` / `path_payment_strict_send` / `path_payment_strict_receive`
  / `create_account` ops referencing classic-credit assets.
- `manage_buy_offer` / `manage_sell_offer` / `create_passive_sell_offer`
  bidirectional offer assets.

The cheapest path piggybacks on the existing trustline-change pass
(`extract_account_states::Pass 2`) — every classic-credit asset that
matters for explorer purposes will eventually appear on at least one
trustline change in some ledger; we don't need to enumerate it from
every op variant. Native XLM is a singleton bootstrap.

## Implementation plan

### Phase 1 — parser helper

New public function in `crates/xdr-parser/src/state.rs` (or sibling
module if cleaner):

```rust
pub fn detect_classic_credit_assets(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedAsset>;
```

Scans `entry_type == "trustline"` changes, extracts the `asset` field
(code + issuer), deduplicates within the call by `(code, issuer)`,
returns one `ExtractedAsset` per distinct pair with
`asset_type: ClassicCredit`. Pool-share trustlines skipped (those
are LP positions per `extract_lp_positions`).

### Phase 2 — native XLM bootstrap

Decide between:

- **Option A** — emit one `ExtractedAsset { asset_type: Native }` per
  ledger; persist UPSERT idempotently against `uidx_assets_native`.
  Trivial code, slight per-ledger write churn (one row).
- **Option B** — emit only on first observation (e.g. only if
  `account_balances_current` for native is non-empty in this ledger).
  More conservative.

A is simpler and the `uidx_assets_native` partial UNIQUE makes the
UPSERT a no-op after the first write. Recommended unless measurement
shows a cost.

### Phase 3 — wire into `persist_ledger`

Concatenate the new producer's output with `detect_assets`' output
before passing to `Staged::prepare`. Per existing dedup logic in
`staging.rs::asset_rows` (line 970+), same `(code, issuer)` from
multiple sources collapses to one row.

### Phase 4 — integration test

`crates/indexer/tests/persist_integration.rs`:

- Fixture: an `ExtractedAccountState` with a non-pool-share trustline
  to a `(code, issuer)` pair, no SAC / Soroban deployments.
- Assert: post-persist, `SELECT COUNT(*) FROM assets WHERE
asset_type = 1 AND asset_code = $1 AND issuer_id = (SELECT id
FROM accounts WHERE account_id = $2)` returns 1.
- Negative case: a pool-share trustline produces no classic-credit
  asset row.

### Phase 5 — empirical replay

Re-run Karol's pubnet 51000000..51000300 backfill (or equivalent
local sweep) and verify:

```sql
SELECT asset_type, COUNT(*) FROM assets GROUP BY asset_type;
-- asset_type=1 (ClassicCredit): ~3904 rows expected
```

### Phase 6 — docs

- `docs/architecture/database-schema/database-schema-overview.md`
  §4.10 (`assets`) — update producer-path note: indexer emits
  ClassicCredit + Native from observed trustlines / bootstrap, not
  only Sac + Soroban.
- `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — note
  the trustline-driven classic-credit producer as a new
  responsibility for `state.rs`.
- ADR 0043 §Decision — already says "List endpoint + on-chain →
  indexer" requires the rows; no amendment needed, the implementation
  finally matches the rule.

## Acceptance Criteria

- [ ] `detect_classic_credit_assets` (or equivalent) public + unit-tested.
- [ ] Native XLM singleton bootstrap path emits `ExtractedAsset { asset_type: Native }` idempotently.
- [ ] `persist_ledger` wires the new producer's output into the same staging path as `detect_assets`.
- [ ] Integration test: trustline observation → classic-credit row in `assets`; pool-share trustline → no asset row.
- [ ] **Empirical replay**: re-run a backfill window that previously held 0 classic credits and verify `SELECT COUNT(*) FROM assets WHERE asset_type = 1` matches the distinct `(asset_code, issuer_id)` count in `account_balances_current` for the same range.
- [ ] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md` §4.10 + `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated; no ADR amendment needed.
- [ ] **API types regenerated** — N/A (no API contract change; `GET /v1/assets` shape unchanged, just returns more rows).

## Out of Scope

- RPC fallback for assets that never surface via trustlines /
  balance changes — bundled with the future "initial-state RPC
  enrichment" task that also covers Bug #2 (home_domain, task 0214)
  and Bug #4 stragglers (task 0218 RPC complement).
- CH writer parity — same follow-up bucket as 0217 `_pending` CH
  parity and 0218 SAC override CH parity. Document scope clearly
  in the PR.
- Backfilling existing rows on already-indexed environments —
  operational runbook analogous to 0217 Part 1; spawn alongside
  or after Phase 1–5 ship.
- Bugs #5 / #6 (enricher worker `token_uri` signature + transient
  classifier) — already addressed inline by Karol in the 0197
  audit branch (`fix(lore-0197): SEP-39 token_uri fallback +
permanent error patterns`).

## Notes

- This is the **production-effect blocker for task 0218** — the
  SAC forward-derive helper consumes `ExtractedAsset` entries; with
  this bug live there is nothing to forward-derive from in
  production. Sequence: ship 0218 (inert) → ship 0219 → end-to-end
  classic-credit-asset + SAC-classification chain becomes alive.
- Related historical context: task 0119 added the classic-credit
  balance staging path; this bug exists because that task added
  the balance path without the matching entity-row producer.
- Karol's finding doc (link in history) carries the full
  reproducible query set + downstream impact table; treat that as
  authoritative source for the empirical numbers above.
