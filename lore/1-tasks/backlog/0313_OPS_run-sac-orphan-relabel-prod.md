---
id: '0313'
title: 'OPS: run sac-orphan-relabel batch on prod CH (flip un-deployed-SAC orphans)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0294', '0303']
tags: [ops, clickhouse, sac, orphans, layer-data, priority-medium, effort-small]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from 0294. The CLI subcommand `sac-orphan-relabel` is built +
      dry-run-validated read-only (5,558 crypto-confirmed / 0 false positives).
      Real run needs a write-capable CH cert (read-only `chq`/`dev_read` cannot
      INSERT) + the 0294 PR merged. Operational, so kept out of the 0294 code PR.
---

# OPS: run sac-orphan-relabel batch on prod CH

## Summary

Run the task-0294 `sac-orphan-relabel` batch against production ClickHouse to flip
existing un-deployed-SAC "orphan" rows (`is_sac=false`, no deploy, NULL `wasm_hash`,
emitting SAC events) to `is_sac=true, contract_type=Token`. This lets the
`nft-reclassify` step (task 0303) DROP their false-positive `nfts_pending` rows
(i128 transfer amounts mis-read as NFT token_ids).

## Status: Backlog

Blocked on: (1) the 0294 PR merging the subcommand, (2) a write-capable prod CH cert.

## Context

The 0294 forward-fix stops NEW orphans at ingest; this batch repairs the EXISTING
history. Code: `crates/backfill-runner/src/sac_orphan_relabel.rs` + subcommand in
`crates/backfill-runner/src/main.rs` (branch `fix/0294_sac-labeling-and-orphan-composition`).

`fetch_orphan_events` is chunked (`FETCH_CHUNK=500`) — a single-pass join over
`soroban_events` (~344M rows) materialising `topics_xdr` OOMs the server.

## Prerequisites

1. **0294 PR merged** (subcommand present on the target branch).
2. **Write-capable mTLS cert** for `https://ch.sorobanscan.rumblefish.dev/`. The
   read-only `dev_read` cert behind the `chq` helper CANNOT INSERT — get a write
   cert (`infra-hetzner/ca/issue-client-cert.sh`) or run from a write-roled host.

## Implementation Plan

### Step 1 — dry-run (read-only, safe)

```
backfill-runner --target clickhouse \
  --clickhouse-url https://ch.sorobanscan.rumblefish.dev/ \
  --ch-cert <write.crt> --ch-key <write.key> --ch-ca <ca.crt> \
  sac-orphan-relabel --dry-run
```

Expected (read-only validation 2026-06-23): `crypto_confirmed ≈ 5,558` of 5,558
SAC-event-emitting orphans (of ~5,607 total by predicate). If wildly different,
STOP and investigate before the real run.

### Step 2 — real run

Drop `--dry-run`. Flips confirmed orphans via an RMT `version=0` override INSERT.
Idempotent (a flipped row is no longer an orphan; re-run is a no-op).

### Step 3 — verify

`SELECT count() FROM soroban_contracts FINAL WHERE is_sac=false AND
coalesce(deployed_at_ledger,0)=0 AND wasm_hash IS NULL` drops; flipped rows show
`is_sac=true`.

### Step 4 — hand off to 0303

Run `nft-reclassify` (task 0303) after, to drop the now-SAC orphans' false-positive
`nfts_pending` rows. Coordinate with the 0303 rollout.

## Acceptance Criteria

- [ ] Dry-run executed; `crypto_confirmed` recorded (≈ 5,558 expected)
- [ ] Real run executed; before/after orphan counts recorded
- [ ] `nft-reclassify` run; false `nfts_pending` rows dropped
- [ ] 49 non-SAC-event orphans left untouched (tiny residual)

## Docs updated

- N/A — operational run only; no architecture-shape change (the parser/schema shape
  changes ship with the 0294 code PR).
