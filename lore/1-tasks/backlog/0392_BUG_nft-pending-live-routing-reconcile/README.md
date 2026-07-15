---
id: '0392'
title: 'NFT pending: stop live fungible-misroute (verdict fail-open) + continuous promote/reconcile'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0391', '0283', '0217', '0306', '0296']
tags: [priority-high, effort-medium, layer-indexer, layer-db, nft, clickhouse]
links: []
history:
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Spawned from 0391 §"Why *_pending grows unbounded" + R §4. Two sub-bugs,
      one shared root (write-time verdict resolution). Measured: hot frozen 33
      days, live path writes ~6,575 pending rows/day @ 91% fungible false-pos.
---

# NFT pending: stop live fungible-misroute + continuous promote/reconcile

## Summary

The `nfts_pending` / `nft_ownership_pending` quarantine (built by **0217**) was
designed as **defer-then-promote**, but only the _defer_ half runs live — the
_promote/drop_ half exists exclusively as the one-shot backfill
`backfill-runner nft-reclassify`. As a result pending grows without bound and
NFT collection/detail pages lag reality. This task fixes the live path so
pending stops filling with fungible false-positives and classified rows get
promoted continuously — **without** mirroring the backfill's brute
`ALTER … DELETE` sweep.

Measured on prod (2026-07-15, see [0391 R §4](../0391_RESEARCH_nft-token-flow-coverage-audit/notes/R-nft-coverage-measured-state.md)):
hot `nfts` frozen at ledger `62,989,407` (**2026-06-12**, last manual drain) for
33 days; live writes ~6,575 pending rows/day, **91% Fungible-verdict**; 401/401
fungible-verdict pending contracts confirmed real fungible assets; 21
`Nft`-verdict collections / 559 tokens stranded.

## Context

Root cause is a single mechanism: **write-time verdict resolution fails open to
Pending.** `route_for` (`stage.rs:1444`) routes `Nft→Hot`, `Fungible|Token→Drop`,
`Other|NULL|uncached→Pending`. The ingest verdict prefetch (0283 G1/G9,
`persist.rs:225,394`) is best-effort with a bounded cross-ledger window and, on
miss, the row falls through to `Pending`. But for the 91% fungible bulk the
contract's verdict **is** already `Fungible` in `soroban_contracts` — a
verdict-authoritative lookup would route them to `Drop` at write time. So the
leak is fixable at the source; a live copy of the backfill DELETE would only
treat the symptom (and would fight the ingest inserts).

`Fungible`/NFT `transfer` events are byte-identical in shape (`from,to,i128` vs
`from,to,token_id`) — the parser cannot distinguish them; only WASM
classification can. So a genuinely-never-seen contract MUST still be able to
quarantine. This task does not try to make pending zero — it makes pending hold
_only_ genuinely-unresolved contracts, and reconciles them once resolved.

## Implementation Plan

### Step 1: Stop the leak — verdict-authoritative write-time routing

- For a contract that already has a verdict row in `soroban_contracts`, the
  ingest path must resolve it (not fail open). Options to evaluate:
  - Widen / fix the G1/G9 prefetch so already-classified contracts are never
    missed (measure miss rate first — why is a classified fungible contract's
    verdict not in the prefetch window?).
  - Or a fallback point lookup on prefetch miss before defaulting to `Pending`.
- Target: `Fungible|Token`-verdict transfers route to `Drop`, never `Pending`.
- Guard cost: this is on the hot ingest path — the lookup must be cheap
  (cache-backed; no per-event CH round-trip). `ponytail:` measure before adding
  any new query per event.

### Step 2: Reconcile the residual — event-driven, per newly-classified contract

- When a contract's verdict first resolves to `Nft`/`Fungible`/`Token` (i.e. its
  WASM becomes observed / `contract-type-rebuild`-equivalent runs), promote
  (`Nft` pending→hot) or drop (`Fungible|Token`) **that one contract's** pending
  rows.
- Scope to the contract, not a full-table sweep. This is the live equivalent of
  the `nft-reclassify` promote/drop, triggered by classification, not by cron.
- Decide the trigger point: at deploy/upgrade when WASM is classified, vs a
  lightweight scheduled reconcile keyed on `soroban_contracts` verdict changes
  since last run.

### Step 3: One-shot cleanup of the accumulated backlog

- The ~280k existing fungible false-positives + stranded `Nft` rows still need a
  single drain to clear the 33-day backlog. That is **0306**'s prod
  reclassify run — coordinate, don't duplicate. This task ensures the backlog
  does not re-accumulate after 0306 drains it.

## Acceptance Criteria

- [ ] `Fungible|Token`-verdict transfers for already-classified contracts route
      to `Drop` at ingest — verified: post-fix, daily fungible-verdict pending
      intake drops to ~0 (re-run the R §4c split).
- [ ] Newly `Nft`-classified contracts' pending rows promote to hot without a
      manual `nft-reclassify` run.
- [ ] Ingest hot-path latency not regressed (verdict lookup is cache-backed, no
      per-event CH query).
- [ ] Genuinely-unresolved contracts (WASM never observed) still quarantine
      correctly — pending is not forced to zero.
- [ ] **Docs updated** — `docs/architecture/**` ingestion-pipeline + XDR-parsing
      sections describe the routing + reconcile (per ADR 0032). Update in PR.
- [ ] **API types regenerated** — N/A unless the fix touches `crates/api/**`
      (routing/ingest is `crates/db-clickhouse` + `crates/indexer`).

## Notes

- Depends conceptually on 0283 (verdict prefetch) — this sharpens its fail-open.
- 0217 (archived) built the quarantine; 0306 is the one-shot prod drain; this
  task is the _live_ continuous half neither covers.
- Do NOT implement as a live mirror of `nft_reclassify`'s `ALTER … DELETE` — that
  treats the symptom and races the ingest inserts.
