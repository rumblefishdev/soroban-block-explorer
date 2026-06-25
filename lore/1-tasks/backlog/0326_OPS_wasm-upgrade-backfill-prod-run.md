---
id: '0326'
title: 'OPS: run wasm-upgrade-backfill on prod CH + validate (1,351 stale contract hashes)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0320', '0316']
tags: [clickhouse, ops, soroban, wasm-upgrade, backfill, validation]
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
