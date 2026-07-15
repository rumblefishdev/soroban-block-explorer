---
prefix: R
title: 'Soroban SAC token-event shapes (empirical, prod) + code anchor map'
status: mature
spawned_from: '0383'
date: 2026-07-13
who: karolkow
---

# R — Soroban token-event shapes (empirical) + where the code lives

Ground truth pulled from prod ClickHouse (`chq`) on 2026-07-13, plus a code
sweep of the parser + persist + read paths. This is the factual base the 0383
plan rests on.

## Event shapes (CAP-67 "unified" SAC events, verified in prod)

Every SAC (classic-asset-wrapped) event carries the asset identity **inside the
event** as a trailing SEP-11 string topic. `data_xdr` holds the amount as i128.
The `_xdr` column names are misleading — they hold ScVal-decoded **JSON**.

| signature | topics                                             | data        |
| --------- | -------------------------------------------------- | ----------- |
| transfer  | `[sym, address(from), address(to), string(asset)]` | i128 amount |
| mint      | `[sym, address(to), string(asset)]`                | i128 amount |
| burn      | `[sym, address(from), string(asset)]`              | i128 amount |
| clawback  | `[sym, address(from), string(asset)]`              | i128 amount |

Asset string forms: `"native"` (XLM) or `"CODE:ISSUER"` (classic credit).

ScVal JSON encoding (`xdr-parser/src/scval.rs`): address → `{type:"address",
value:"G.../C..."}`, i128 → `{type:"i128", value:"<decimal string>"}`, symbol →
`{type:"sym", value:...}`, string → `{type:"string", value:...}`.

## Shape distribution + volume (last ~2M ledgers)

```
mint:     n_topics=3 (SAC, asset explicit)  182.41M   | n=2 (bespoke) 32.5k | n=1 166
burn:     n_topics=3                          17.97M   | n=2           35.1k | n=1 1610
clawback: n_topics=3                           4.30M   | n=2               2 | n=1 0
```

- **n_topics=3 = 99.98%**: SAC events, asset in the trailing string.
- **n_topics=2 = ~0.02%**: bespoke (non-SAC) tokens. No asset string → asset
  identity = the emitting contract (type-3 token surrogate). `data` is sometimes
  a `map{amount:i128}` not a bare i128 (so amount decode may be None there).
- **n_topics=1**: degenerate, negligible.

Full `soroban_events` = 9.68B rows → backfill scan is billions; must batch by
partition `intDiv(ledger_sequence,500000)` to respect read quota.

## Key de-risk: no "third native key" trap

Because the asset is explicit in the event (`"native"` / `"CODE:ISSUER"`), we
parse asset identity straight from the row — **no `asset_sac` contract→asset map
lookup needed**, no SAC-contract-surrogate keying. `"native"` maps directly to
`ids::NATIVE_ASSET_ID = hash64("native")`; `"CODE:ISSUER"` to
`ids::asset_id(1, code, account_id(issuer), 0)`. This kills the third-native-key
concern the 0359 devils-advocate raised.

## Code anchor map

| Concern                                                        | Location                                                                                                                  |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| transfer decoder (mirror this)                                 | `crates/xdr-parser/src/event_filters.rs` — `parse_transfer` / `transfer_participants` / `numeric_scval` / `address_topic` |
| SAC signature list + topic-symbol reader                       | `crates/xdr-parser/src/sac.rs` (`SAC_CONTROL_EVENT_SIGNATURES`, `topic_symbol_value`)                                     |
| ingest event→participant loop (transfer-only today)            | `crates/db-clickhouse/src/persist/stage.rs:503-515`                                                                       |
| op→asset presence rows                                         | `crates/db-clickhouse/src/persist/stage.rs:960-982`                                                                       |
| surrogate IDs                                                  | `crates/db-clickhouse/src/persist/ids.rs` (`asset_id`, `account_id`, `NATIVE_ASSET_ID`)                                   |
| `soroban_events` DDL                                           | `crates/db-clickhouse/schema/init.sql:629-641`                                                                            |
| `transaction_participants` DDL                                 | `init.sql:594-601` (grain: account×tx presence, no role/amount)                                                           |
| `operation_asset_appearances` DDL                              | `init.sql:617-624` (grain: asset×tx presence, no amount)                                                                  |
| account activity read (source = participants, no union)        | `crates/api/src/accounts/queries.rs:471-612`                                                                              |
| asset activity read (arm A = operation_asset_appearances)      | `crates/api/src/assets/queries.rs:619-730` (seek arms ~line 664)                                                          |
| CH-transform backfill precedent                                | `crates/backfill-runner/src/wasm_upgrade_backfill.rs`                                                                     |
| `soroban_invocations_appearances.amount` = fold-count (K4-3/4) | `init.sql:643-660`                                                                                                        |

## K1-7 resolved (no key fix)

`soroban_events` RMT `ORDER BY (contract_id, ledger_sequence, transaction_id,
event_index)`. `event_index` is monotonic-unique per tx (`xdr-parser/src/
event.rs`, `types.rs:153`), so distinct events never collapse even though the
payload columns sit outside the sort key. Confirmed — no key change needed.
