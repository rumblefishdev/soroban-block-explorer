---
id: '0221'
title: 'BUG: CH stage routes NFT candidates of pre-classified SAC contracts to nfts_pending (no DB lookup)'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0118', '0217', '0220']
tags:
  [
    'clickhouse',
    'sac',
    'quarantine',
    'runbook',
    'phase-future',
    'effort-small',
    'priority-medium',
  ]
links:
  - 'docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md'
history:
  - date: 2026-05-14
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from empirical post-merge verification of PR #186 (task 0220).
      CH backfill of ledgers 62080000-62143999 produced 1.29M `nfts_pending`
      rows; 25.7% (331,273) of them belong to contracts ALREADY classified
      as SAC in `soroban_contracts` (`is_sac=true, contract_type=0=Token`).
      Hot `nfts` table = 0 rows (API correctness preserved); bug only
      inflates quarantine volume. Drain runbook committed +
      empirically executed on the 64k pilot (1,288,888 → 957,615 rows).
      512k pilot confirmed leak is structural (26.75% at scale).
---

# BUG: CH stage routes NFT candidates of pre-classified SAC contracts to nfts_pending (no DB lookup)

## Summary

`crates/db-clickhouse/src/persist/stage.rs` routing `route_for(strkey)` falls
back to `NftRoute::Pending` for any contract missing from in-ledger
`verdict_by_contract` map. The map is populated **only** from contracts
emitted into `out.contract_rows` during THIS stage call (deploys in window

- same-ledger SAC overrides). CH stage has no DB access, so SAC contracts
  classified in earlier ledgers are invisible — NFT-candidate transfers for
  those contracts in a later ledger that lacks a parallel asset transfer
  route to `nfts_pending` instead of `Drop`.

Empirical impact:

| Pilot                    | nfts_pending total | SAC leak  | Leak % |
| ------------------------ | ------------------ | --------- | ------ |
| 64k (62080000-62143999)  | 1,288,888          | 331,273   | 25.7%  |
| 512k (62016000-62554128) | 9,169,616          | 2,452,683 | 26.75% |

API impact: zero — pilot endpoints read from `nfts` hot table only,
which contains 0 rows. Bug surfaces as inflated `nfts_pending` storage
and skewed audit numbers. **Leak rate stable across scale = structural,
not window-artifact.**

## Context

The design is **acknowledged** in `stage.rs` lines 889-893:

```rust
// Contracts with NO entry → treat as `Other`/uncached → route to
// pending. CH has no DB access in the stage, so prior-ledger
// classifications are not visible here; this is the same semantic
// PG would produce for a worker with an empty `ClassificationCache`
// — pending now, drained / promoted later via the runbook.
```

PG-side (task 0118 Phase 2) closes the gap with a per-worker
`ClassificationCache` populated by a single SELECT against
`soroban_contracts` before per-event routing. CH side has no equivalent
because the stage is a pure function over the parsed slices — no
sqlx/clickhouse handle in scope.

The follow-up drain runbook implied by the comment is now committed
at [`docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md`](../../../docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md).

## Root cause (precise)

`stage.rs:894-901`:

```rust
let mut verdict_by_contract: HashMap<&str, ContractType> = HashMap::new();
for row in &out.contract_rows {
    if let Some(ty_i16) = row.contract_type
        && let Ok(ty) = ContractType::try_from(ty_i16)
    {
        verdict_by_contract.insert(row.contract_id.as_str(), ty);
    }
}
```

`out.contract_rows` contains:

1. In-window `contract_deployments` (line 367) — only if deploy is in this stage call.
2. `contract_name_writes` (line 403) — `contract_type: None`, skipped by the `if let Some`.
3. SAC overrides (line 432) — only contracts in **this ledger's** `sac_overrides` slice.
4. Pass-2 stub-rows (line 1219) — `contract_type: None`, skipped.

So a SAC contract `C…` only appears in `verdict_by_contract` for a ledger
where (a) it was deployed (very rare for SAC), or (b) an asset
operation/transfer for the underlying classic asset fired
`derive_sac_overrides_from_assets` for it. For all other ledgers where the
contract emits NFT-candidate transfer events, the map lookup returns
`None` and `route_for` returns `Pending`.

## Fix options

Three viable approaches, **not mutually exclusive**:

### Option A — DB lookup pass in CH stage (closes the gap at write time)

Lift `stage::prepare_with_sac_overrides` from a pure function to a method
on a `Stage` struct that holds a CH client + per-worker cache (mirror PG
`ClassificationCache`). Before NFT routing, batch-SELECT
`contract_id, contract_type, is_sac FROM soroban_contracts FINAL WHERE
contract_id IN (…)` for all distinct `nft.contract_id` not yet in
`verdict_by_contract`. Populate map, then route.

- **Pros:** no garbage written, mirrors PG semantic, audit numbers clean.
- **Cons:** breaks the "stage is pure / unit-testable" invariant the
  CH crate currently relies on; one extra round-trip per ledger;
  `FINAL` reads are expensive on CH RMT.

### Option B — Post-backfill drain runbook (matches existing comment)

**Status: SHIPPED.** [`docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md`](../../../docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md).

```sql
ALTER TABLE nfts_pending
DELETE WHERE contract_id IN (
    SELECT id FROM soroban_contracts FINAL
    WHERE is_sac = true OR contract_type IN (0, 3) -- Token, Fungible
);
OPTIMIZE TABLE nfts_pending FINAL;
```

- **Pros:** trivial, matches comment intent, zero stage-code change.
- **Cons:** garbage exists transiently; operators must remember to run;
  no protection against incremental backfills accumulating between drains.

### Option C — Promote/drain on every partition close

Hook a post-write drain into the worker's per-window completion (e.g., at
the end of each `BatchWindowReport`). Same SQL as Option B but automatic.

- **Pros:** automatic; cleaner than Option B.
- **Cons:** still requires the SELECT round-trip; can race with concurrent
  workers; adds operational complexity.

## Implementation Plan

**Recommended: Option B (shipped) for short-term + Option A for long-term.**

### Step 1 — Commit drain script (Option B) ✅ DONE

- New file `docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md`.
- Idempotent: includes sanity SELECT before DELETE.
- Linked from this task.

### Step 2 — Empirical verify ✅ DONE

Executed on the 64k pilot (CH on local machine):

```
Pre-drain:    nfts_pending = 1,288,888 (leaked = 331,273)
Post-drain:   nfts_pending =   957,615 (leaked =       0)
Δ:                          -331,273 (exact, -25.7%)
```

Idempotent — second pass deletes 0 rows.

### Step 3 — Decide on Option A vs C (defer to follow-up)

Spawn a research/feature task if Option B drain proves operationally
painful (forgotten, expensive, racing). Until then, Option B is sufficient
for the pilot.

## Acceptance Criteria

- [x] `docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md` committed.
- [x] Sanity SELECT documented in the script (counts before / after).
- [x] Empirical run reduces `nfts_pending` proportionally.
- [x] Re-run is idempotent (second pass deletes 0 rows).
- [ ] Runbook entry added to CH backfill operator checklist (post-merge).
- [ ] **Docs updated** — N/A unless docs/architecture/\*\* describes
      `nfts_pending` semantics. Existing comment at
      `stage.rs:889-893` already documents the design.
- [ ] **API types regenerated** — N/A (no `crates/api/**` change).

## Future Work

- **Option A (DB lookup pass in stage)** — capture as separate task if
  drain proves painful. Lifts stage from pure-function invariant.
- **Mirror drain for `nft_ownership_pending`** — same SQL pattern,
  different table.
- **Audit: are there other quarantine tables with the same leak shape?**
  E.g. `liquidity_pools_pending`, `assets_pending` (if they exist) —
  same per-ledger pure-function constraint applies.

## Risks / Notes

- **Quarantine drain is destructive**: `ALTER TABLE DELETE` on CH RMT
  is async and expensive. Run during low-traffic windows.
- **Cross-partition consistency**: deletes apply per-shard; on
  multi-shard clusters confirm propagation.
- **Operator gotcha**: pending counts after backfill will look "broken"
  until drain runs — document this prominently to avoid panic / false
  bug reports.
