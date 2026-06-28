---
id: '0326'
title: 'OPS: run wasm-upgrade-backfill + upgradeable-backfill on prod CH + validate'
type: OPS
status: active
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
      separate (mirrors 0295 → 0321). Read-only prod preview 2026-06-24:
      upgraded_contracts=1362, corrected=1351, unparseable=0.
  - date: 2026-06-27
    status: active
    who: karolkow
    note: >
      Promoted to active for the prod run. SCOPE WIDENED (user decision): this task
      now also umbrellas the 0327 `upgradeable-backfill` prod run — 0327 shipped the
      code but has no dedicated OPS task, so both one-shot backfills execute under
      0326. Read-only prod re-verification 2026-06-27 (chq): script 1 invariant =
      1351 stale (unchanged baseline), script 2 scanned = 2667 (in-use=2673,
      already-keyed=0). Engines confirmed RMT for both soroban_contracts +
      wasm_interface_metadata. Indexer stopped (last ledger 2026-06-15). Mainnet-RPC
      truth spot-check 20/20: events-hash == on-chain, db stale. Two defensive code
      patches requested for upgradeable-backfill (hard-fail -> log+nonzero-exit;
      JSON-fallback -> skip-not-overwrite). Local only - no commits/push.
---

# OPS: run wasm-upgrade-backfill on prod CH + validate

## Summary

Execute the one-shot `backfill-runner wasm-upgrade-backfill` (built in **0320**)
against prod ClickHouse to correct the stale `soroban_contracts.wasm_hash` for
contracts that upgraded their WASM, then validate. Code + local validation are
done in 0320; this is the prod run only.

## Pre-run facts (read-only prod preview, 2026-06-24)

- `upgraded_contracts = 1362`, `corrected = 1351`, `unparseable = 0` (computed
  read-only via `chq`; mirrors the binary's `--dry-run` stats exactly).
- Invariant baseline: **1351 violations** (see 0320 `notes/G-invariant-wasm-hash-current.sql`).

## Steps

1. **Stop the indexer** — the backfill does a whole-table `EXCHANGE` on
   `soroban_contracts`; a concurrent live write between staging-build and swap
   would be lost. (Same constraint as `contract-type-rebuild` / `repair_tier1`.)
2. **Dry-run** — `backfill-runner --target clickhouse … wasm-upgrade-backfill --dry-run`.
   Expect `upgraded_contracts=1362 corrected≈1351 unparseable=0`; live table untouched.
3. **For-real** — same without `--dry-run`. Staging + `EXCHANGE`.
4. **Restart the indexer.** The live path (0320) keeps new upgrades correct going forward.

## Validation (acceptance)

- [ ] Invariant query (`G-invariant-wasm-hash-current.sql`) returns **0** post-run.
- [ ] ≥20 corrected contracts spot-checked: `soroban_contracts.wasm_hash` ==
      live on-chain wasm (`stellar contract fetch … | sha256`). (0320 validated 28/28 pre-run.)
- [ ] API contract page for a known upgraded contract (e.g. `CDL74RF5…` → current
      `db2c14…`) shows the corrected hash + interface + `upgradeable: true`.
- [ ] No identity regression: deployer / deployed_at / name / contract_type / is_sac
      unchanged for a sample of corrected rows (carry-forward held).

## Notes

- Needs prod CH **write** creds (the `chq` mTLS certs are read-only — cannot run
  the binary). Run from a host with the write endpoint + certs.
- Coordinate with **0316** (RMT clobber discipline): after the backfill, the
  invariant doubles as the clobber-back tripwire — if it goes non-zero later, a
  co-writer regressed an upgraded row.

## Findings + decisions (2026-06-27 prep)

Read-only prod re-verification (chq) + mainnet-RPC truth check. Runbook +
verification script live in the session scratchpad (`runbook_0326.sh`,
`verify_wasm_upgrade.sh`).

- **Engines (prod):** both `soroban_contracts` (`RMT(wasm_uploaded_at_ledger)`) and
  `wasm_interface_metadata` (`RMT`, no version col) confirmed ReplacingMergeTree —
  the "some tables shipped as plain MergeTree" worry does NOT apply here.
- **No data loss, either script:** no DELETE/DROP on data (only self-made temp
  tables). Both writes pass the FULL row — script 1's `INSERT…SELECT` projects all
  9 columns (only `wasm_hash` + `wasm_uploaded_at_ledger` overridden, 7 carried
  forward); script 2's row IS the whole table (`wasm_hash` + merged `metadata`).
- **Mainnet truth:** sampled 20/20 stale contracts — `events_chain == on-chain
(stellar contract fetch | sha256)`, `db_stored` never. The hash script 1 writes
  is the true current mainnet hash.
- **Swap vs insert (script 1):** keep EXCHANGE (atomic, collapses dupes, engine-
  agnostic). Insert would work on RMT but leaves old+new until merge.
- **Two defensive patches applied to `upgradeable-backfill`** (user-requested):
  hard-fail `Err`/panic → `warn!` + non-zero exit (resolved rows already written);
  JSON `merge_upgradeable` → skip non-object instead of overwriting `{}`. 59 tests
  green.
- **`wasm_interface_metadata` is RMT with NO version column** + the API reads it
  WITHOUT `FINAL` (3 sites). After script 2's re-insert, old+new rows coexist until
  merge → transient Unknown-chip window (NOT data loss). **Decision:** the runbook's
  `OPTIMIZE TABLE wasm_interface_metadata FINAL` step is _sufficient for steady
  state_ (the live path writes byte-identical metadata per hash going forward, so no
  future divergence). The fundamental hardening (make the API reads `FINAL` + fix the
  wrong "plain MergeTree" comment) is tracked as **0332** — kept out of this OPS run
  because it's a hot-path API change that previously broke (`d258c93b`) and needs
  cross-env engine verification + testing.
