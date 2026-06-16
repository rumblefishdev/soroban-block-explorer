---
id: '0297'
title: 'FEATURE: contract-name enrichment (off-ledger name() → side-table) + ScVal::Bytes name-decode fix'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0283', '0231']
tags: [clickhouse, enrichment, soroban, layer-data, priority-low, effort-medium]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 future work (G5 name-clobber structural close). Bundles
      the contract-name enrichment job with a minor ScVal::Bytes name-decode
      bug found in the same audit. The contract-name piece overlaps 0231 and may
      fold there.
---

# FEATURE: contract-name enrichment + bytes-decode fix

## Summary

Contract names are **off-ledger**: SEP-41 / OpenZeppelin Soroban tokens expose
`name` via a `name()` WASM function (read off-ledger via `simulateTransaction`),
NOT a persisted `Symbol("name")` ledger entry — so `soroban_contracts.name` is
empirically empty and the on-ledger name-write path is dead. Populating contract
names is therefore an **enrichment job** (RPC `name()` → side-table), the
structural close of the G5 name-clobber guardrail shipped in 0283. Bundles a
minor `ScVal::Bytes` name-decode mismatch found in the same audit.

## Context

Spawned from **0283**. The G5 guardrail (shipped in 0283) disabled the dead
on-ledger name-write loop and added a tripwire; the real fix for populating
`name` is enrichment, in the ADR 0048 side-table family. **Overlaps 0231**
(SEP-1/NFT enrichment side-tables, same AWS SQS+Lambda path) — consider folding
the name-enrichment piece into 0231 rather than a standalone runner.

## Implementation Plan

### Step 1 — contract-name enrichment

Fetch contract `name` (and `symbol`) via RPC `simulateTransaction` of `name()`
into a side-table (ADR 0048 Option C: enrichment owns off-chain values, API
read-composes; on-chain soroban names stay in `soroban_contracts`). Coordinate
with / fold into 0231's enrichment write path. Subject to the live-RPC liveness
ceiling (archived/evicted contracts un-enrichable — see 0231 note).

### Step 2 — ScVal::Bytes name-decode fix (minor)

Producer base64-encodes `ScVal::Bytes` (`scval.rs:45`) but `decode_scval_string`
hex-decodes (`state.rs:243`) — a bytes-typed name would fail to decode. Align
producer/consumer (pick base64 or hex consistently) + add a test. Low impact
today (names off-ledger) but a latent correctness bug.

## Acceptance Criteria

- [ ] Contract `name`/`symbol` populated via enrichment side-table (or folded into 0231)
- [ ] `ScVal::Bytes` decode consistent producer↔consumer + unit test
- [ ] API read-composes enriched names per ADR 0048 Option C
