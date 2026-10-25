---
id: '0652'
title: 'BUG: lookups in the write path that miss silently and persist a default'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0571', '0316', '0421', '0640', '0641', '0309', '0392']
tags:
  [
    'indexer',
    'clickhouse',
    'data-integrity',
    'priority-medium',
    'effort-medium',
  ]
links: []
history:
  - date: '2026-10-19'
    status: backlog
    who: karolkow
    note: >
      Filed from the 0571 follow-up: an audit of the Rust write path for the
      defect class of 0571 (a lookup misses, a default is persisted, nothing
      logs). Read from code on develop; nothing measured on production yet.
---

# BUG: silent lookup fallbacks in the write path

## Summary

In 0571 a SAC missing from the SAC → asset map was keyed as a Soroban token
without a log line; 300 mainnet and 22,166 testnet balances sat wrong for a
month. This task lists every other place in the write path (`xdr-parser`,
`db-clickhouse` persist, `indexer`, `backfill-runner`, `enrichment-*`; tests and
the API excluded) where a lookup miss or a parse failure turns into a persisted
default or a dropped row with no error. Each item is fixed by making the miss
loud (an error or an `alarm` log field with its CloudWatch filter, as 0571 does
for `sac_map_miss`) or by removing the lookup; a fix that already has a task
stays there.

Line numbers are from `develop` at `9adeea012`. ✔ = checked in code by hand,
the rest are from the audit read.

## Wrong value persisted, silent

| #    | where                                                         | on a miss                                                                                                                                                                                                                                                                                         | task                                                     |
| ---- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| F1 ✔ | `persist/stage.rs:888-904`, accounts                          | a participant whose account entry did not change gets a newer row with `sequence_number = 0`, `home_domain = NULL`, `first_seen = last_seen = this ledger`, which wins the RMT merge                                                                                                              | 0316 / 0421                                              |
| F2   | `backfill-runner/src/sink.rs:265` → `stage.rs:453`            | no prior contract row → upgrade not applied, no log. Live too when the contract is absent from `soroban_contracts`                                                                                                                                                                                | 0640 / 0641                                              |
| F3   | `sink.rs:259-260` → `stage.rs:938-945`, `stage/nfts.rs:60-72` | empty verdict maps → `contract_type = Other`, NFT events to `nfts_pending`                                                                                                                                                                                                                        | 0641                                                     |
| F4 ✔ | `stage.rs:1069`, `:1978`                                      | pair legs and pool movements read `sac_classic`, the pool registry reads the same-ledger-seeded `sac_map` → two ids for one leg when a SAC deploys in the pool's ledger                                                                                                                           | here (PR #680, closed unmerged 2026-10-19, held the fix) |
| F0 ✔ | `stage/soroban_pools.rs:102`                                  | the original 0571 case: a balance stored in a SAC the map lacks keys on the SAC, no log. The parser knows the shape (`decode_sac_balance_value`) but drops it. Decided fix: pass `sac_balance`, `error!(alarm = "sac_map_miss")` + CloudWatch alarm, row still written (PR #680, closed unmerged) | here                                                     |
| F5 ✔ | `stage.rs:1720`                                               | SAC deployment with no identity → no `asset_sac` row; silent here, `warn!` in `xdr-parser/src/state.rs`                                                                                                                                                                                           | 0571                                                     |
| F11  | `xdr-parser/src/contract.rs:149`                              | `parse_spec_entries`: `Err(_) => break` truncates the function list that classification reads; `contract.rs:262` persists `upgradeable = false` on a parse failure                                                                                                                                | 0309                                                     |

## Rows dropped, silent or `debug!` only

| #   | where                                                 | on a miss                                                                                                         |
| --- | ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| F6  | `stage.rs:526-527`                                    | CAP-85 ref row dropped on a non-32-byte hash                                                                      |
| F7  | `stage.rs:872-883`                                    | `M…` (muxed) keys dropped from accounts staging, `debug!` count only                                              |
| F8  | `persist/classic_pools.rs:20-27`, `:90-97`            | pool dropped when an asset does not split; reserve written 0 when absent                                          |
| F9  | `stage.rs:1866-1882`, `:1916-1920`                    | trustline balance dropped on an unparseable asset; balance `"0"` when missing                                     |
| F10 | `indexer/src/contract_metadata.rs:~165-200`           | no metadata write when the WASM is not in `wasm_programs` yet (deliberate; `contract-metadata-backfill` fills it) |
| F12 | `xdr-parser/src/ledger_value.rs:140`, `stage.rs:2435` | balance delta dropped on overflow or SAC-map miss; no live caller found                                           |

## Softer: wrong flag, or `warn!` and a default

| #   | where                                                                                                   | on a miss                                                                                                            |
| --- | ------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| F13 | `stage.rs:1005`, `:1016`                                                                                | a `parse_error` transaction stored with `op_count = 0`, `has_soroban = false`                                        |
| F14 | `xdr-parser/src/state.rs:443-462`                                                                       | pool fee / reserves / shares `0` when the JSON lacks them                                                            |
| F15 | `persist/value_flow.rs:172-190`                                                                         | an unparseable muxed key kept raw                                                                                    |
| S1  | `persist.rs:260-275`, `:490-500`, `:380-410`                                                            | verdict prefetch errors → `warn!` and empty maps (live twin of F3); bad rows dropped by `continue` / `.ok()?`        |
| S2  | `stage/account_entry_state.rs:24-72`                                                                    | signers dropped with `warn!`; flags `unwrap_or(0)` silent                                                            |
| S4  | `enrichment-shared/src/enrich_and_persist/sep1_assets.rs:93`, `:104`; `nft_token_uri.rs:82`             | missing issuer row / home domain (a consequence of F1) → a permanent "failed" sentinel; `:104` logs at `debug!` only |
| S5  | `indexer/src/handler/process.rs:193`                                                                    | a `parse_error` transaction: `warn!` and its ops/events skipped                                                      |
| S6  | `backfill-runner/src/sink.rs:109`, `bootstrap.rs:172`, `rpc_snapshot.rs:271`, `balance_seed.rs:141/160` | out-of-range or undecodable snapshot entries skipped (`bootstrap.rs` silently)                                       |

## Also found

- `backfill-runner/src/rpc_snapshot.rs:305` `decode_balance_entry` is a second
  decoder of the token balance entry beside `xdr-parser/src/state.rs`
  (`extract_soroban_token_balances`).

## Acceptance Criteria

- [ ] Each row above: fixed (miss made loud or the lookup removed), moved to
      its task, or kept with the fact that makes the default correct.
- [ ] First, measure F1 on production: `accounts` rows whose newest version
      has `sequence_number = 0` and no `account_entry_state` row.
- [ ] **Docs updated** — `N/A` unless a fix changes the described pipeline.
