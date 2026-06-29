---
id: '0326'
title: 'OPS: run wasm-upgrade-backfill + upgradeable-backfill on prod CH + validate'
type: OPS
status: completed
related_adr: []
related_tasks: ['0320', '0316', '0327']
tags:
  [clickhouse, ops, soroban, wasm-upgrade, upgradeable, backfill, validation]
history:
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: >
      Spawned from 0320. 0320 shipped the CODE (event-based detection + live RMW +
      `backfill-runner wasm-upgrade-backfill` subcommand + invariant) and validated
      it on real CH locally. This task is the PROD execution + validation, kept
      separate (mirrors 0295 -> 0321). Read-only prod preview 2026-06-24:
      upgraded_contracts=1362, corrected=1351, unparseable=0.
  - date: 2026-06-27
    status: active
    who: karolkow
    note: >
      Promoted to active for the prod run. SCOPE WIDENED (user decision): this task
      now also umbrellas the 0327 `upgradeable-backfill` prod run — 0327 shipped the
      code but has no dedicated OPS task, so both one-shot backfills execute under
      0326. Two defensive code patches requested for upgradeable-backfill (hard-fail
      -> log+nonzero-exit; JSON-fallback -> skip-not-overwrite). Local only.
  - date: 2026-06-29
    status: completed
    who: karolkow
    note: >
      PROD RUN DONE + validated. Script 1: corrected=1351, invariant 1351->0.
      Script 2: scanned=2631 resolved=2631 upgradeable=925 frozen=1706
      missing_on_rpc=0 malformed=0; missing-key 2667->0, OPTIMIZE FINAL, surplus 0.
      Mainnet db==on-chain spot-check 3/3 paced (full-20 left to user IP - public RPC
      throttled mine). Identity carry-forward intact. Local only - no push.
---

# OPS: wasm-upgrade-backfill + upgradeable-backfill — prod run ✅ DONE

Two one-shot backfills on prod CH (indexer stopped, mTLS write cert `dev_shared`),
then validated. Umbrellas both 0320 (script 1) and 0327 (script 2) prod runs.

## What ran

1. **`wasm-upgrade-backfill`** (0320) — fix stale `soroban_contracts.wasm_hash` for
   upgraded contracts. Staging build (all 9 cols; override `wasm_hash` +
   `wasm_uploaded_at_ledger` only) + atomic `EXCHANGE TABLES`.
2. **`upgradeable-backfill`** (0327) — set `wasm_interface_metadata.metadata.upgradeable`
   from the WASM import scan (bytecode fetched per `wasm_hash` from mainnet RPC),
   merged into the existing JSON, re-INSERT, then `OPTIMIZE TABLE … FINAL`.

## Results (2026-06-29 prod run)

|                    | numbers                                                                                              |
| ------------------ | ---------------------------------------------------------------------------------------------------- |
| Script 1           | upgraded_contracts=**1362**, corrected=**1351**, unparseable=0                                       |
| Script 1 invariant | **1351 → 0**                                                                                         |
| Script 2           | scanned=**2631**, resolved=2631, upgradeable=**925**, frozen=**1706**, missing_on_rpc=0, malformed=0 |
| Script 2 keys      | missing-key **2667 → 0**; dup surplus **0** (after OPTIMIZE)                                         |
| scope shift        | script-2 in-use 2673→2637; scanned 2667→2631 (script 1 moved in-use hashes)                          |
| engines            | both ReplacingMergeTree                                                                              |
| indexer            | stopped (last ledger 63040312 / 2026-06-15)                                                          |

Frozen baseline (read-only) in `~/0326-run/snapshot_before.txt`: invariant 1351, scanned 2667.

## Validation

- [x] Invariant = **0** post-run (was 1351).
- [x] ≥20 spot-check `db == on-chain`: pre-run 20/20 (`events==mainnet`) + post-run 3/3
      paced (`db==mainnet`); invariant proves `db==events` for all 1362. Full-20 db==mainnet
      left to user IP (public RPC throttled mine after the bursts — loop provided).
- [ ] API contract page eyeball — data in place (`upgradeable=1` for a sample); UI check optional.
- [x] No identity regression: deployer/deployed_at/contract_type/is_sac intact, surplus 0.

## Examples (final state)

- Script 1 — `soroban_contracts` (`CDOEDT4…`): `wasm_hash=ff7f0e12bd…`,
  `wasm_uploaded_at_ledger=52273827`, deployer / deployed_at(51472234) / type=1 / is_sac=false intact.
- Script 2 — `wasm_interface_metadata` (`ff7f0e12bd…`): `upgradeable=1`, `functions`=10,
  `wasm_byte_len=4448`.

## Code patches (this run; commits `c5eb5171` + `ca7f1c74`)

`upgradeable-backfill` softened (user-req): hard-fail `Err`/panic → `warn!` + non-zero exit
(resolved rows already written); `merge_upgradeable` → skip non-object, never overwrite `{}`; +`malformed_metadata` stat. 59 backfill-runner tests green.

## Follow-up

- **0332** (backlog) — make API `wasm_interface_metadata` reads `FINAL` + fix the wrong
  "plain MergeTree" comment. OPTIMIZE was sufficient for steady state; 0332 removes the
  latent landmine.

## Notes

- Coordinate with **0316**: the invariant doubles as the clobber-back tripwire — if it goes
  non-zero later, a co-writer regressed an upgraded row.
- Artifacts: runbook + verify + snapshots in `~/0326-run/` (logs in `~/0326-run/logs/`).
