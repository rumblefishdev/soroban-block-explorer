---
title: 'S — Event-based fix decision + data-model + open decisions'
type: synthesis
status: mature
spawned_from: notes/R-soroban-upgrade-research.md
spawns: []
tags: [decision, executable_update, clickhouse-rmt, data-model]
history:
  - date: 2026-06-24
    status: mature
    who: karolkow
    note: >
      Synthesis after R-research + two devil's-advocate passes. Decides the
      event-based approach, the wasm-row data model, and records D1-D5.
---

# S — Event-based fix: decision, data model, open decisions

> Synthesis note, 2026-06-24, karolkow + Claude. Status: mature.
> So-what of [[R-soroban-upgrade-research]].

## Decision: detect upgrades from the `executable_update` event

Supersedes the 0295 parser draft (scan `updated` ContractInstance). Reasons:

1. **Already ingested** — 4,691 events in `soroban_events`, carrying old+new hash.
2. **Restore-noise immune** — fires only on a real executable change, not on
   TTL-restore (which the `updated`-entry diff cannot distinguish).
3. **No dependency on the unconfirmed XDR shape** — research could not pin whether
   an upgrade is a single `updated` or a `state`+`updated` pair; the event sidesteps it.
4. **Backfillable in-CH, no S3 re-parse** — unlike 0321's tombstone backfill.

Fix = (a) live: on `executable_update`, RMW `soroban_contracts.wasm_hash` +
contract_type from `prior_wasm_verdicts[new]`; (b) one-shot backfill of the 1,362
from existing events; (c) audit-harness invariant `wasm_hash == latest
executable_update.new_hash`.

## Data model: what's replaced vs what persists (answers "new wasm row vs update old")

- **`wasm_interface_metadata` (keyed by wasm_hash) is append-only.** Old AND new
  wasm interfaces both persist forever — verified (deploy `6A4F056B` and current
  `db2c14` both present). We never delete or replace a wasm's interface.
- **`soroban_contracts` (keyed by contract_id, RMT) holds only the CURRENT pointer.**
  The RMW overwrites the contract's `wasm_hash` (RMT collapses to one row per
  contract*id). The \_previous pointer* is not kept in this table.
- **Pointer history lives in `soroban_events`** — the full old→new chain per
  contract is queryable there (this is what powers D2's "upgrade history").

So nothing is lost: interfaces are kept (by hash), pointer-history is kept (by
event). Only the contract's _current_ pointer is mutated in place — which matches
the chain itself (the ledger mutates the instance entry in place, same contract_id).

## Class-change → scope (the important one)

0 net class changes across 1,362 contracts (see [[R-soroban-upgrade-research]]).
The fix is "update `wasm_hash`" for 100% of current state. The rare class-flip
handling (reclassify + NFT quarantine promote/drop) and verifying the one observed
flip is real vs an interface-extraction artifact are **deferred to [[0325]]** —
out of 0320 scope.

## `executable_update` usage

Central to the design: (1) the live detection signal, (2) the backfill source,
(3) the audit invariant, (4) D2's upgrade-history / upgradeable surface. We do not
add a parser-side instance diff at all.

## Decisions (D1-D5)

- **D1 — Backend: ClickHouse only.** PG retired (0243). Confirmed by human.
- **D2 — Ship history + "upgradeable: yes".** Confirmed by human. Source =
  `soroban_events` chain (count + old→new list); "upgradeable" positive = "emitted
  ≥1 `executable_update`". Immutability (the hard negative) stays deferred.
- **D3 — Cache: self-healing.** `contracts/cache.rs` is moka with a fixed **45s TTL**
  (Lambda, per-instance). Backfill/live RMW propagate within 45s — no explicit
  invalidation needed.
- **D4 — Sequencing: option C, refined (locked by human).** 0320 ships only its OWN
  write correctly: a sibling prefetch (`stage.rs` has verdicts, not full rows → SELECT
  deployer/deployed_at/name/is_sac for upgraded contract_ids, same shape as
  `fetch_prior_contract_verdicts`) → carry-forward → write full row, + the audit
  invariant tripwire. The other-writer clobber audit, the engine change
  (`CoalescingMergeTree` / `SimpleAggregateFunction` to drop read-first everywhere),
  and **removing 0320's prefetch-read** all move to **0316** — gated by its Phase-0
  "is it even worth it" recon (only 1–2 cases → keep read-modify-write, no migration).
  Rejected: B (ship known clobber) and A (block on full 0316).
- **D5 — Priority: normal** (was low). Confirmed. Justified by user-visible stale
  code-hash + interface on the most-viewed contracts.
