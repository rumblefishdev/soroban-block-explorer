---
id: '0307'
title: 'RESEARCH: SAC-skeleton /v1/contracts de-pollution — read-filter vs side-table (0294 Step 3)'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0294', '0221', '0218', '0283']
tags:
  [
    clickhouse,
    sac,
    contract-classification,
    api,
    registry-pollution,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: 2026-06-18
    status: backlog
    who: karolkow
    note: >
      De-bundled from 0294 Step 3. The 0294 SAC-labeling fix (live forward-fix +
      batch orphan-relabel) is the 100%-certain core; this registry
      de-pollution is NOT 100%-safe (the C3 0221-re-leak constraint) and needs
      design — so it is spun out. A 2026-06-18 research spike (agent,
      repo-as-interpretation + prod `chq`) produced a decision-ready note.
      RECOMMENDATION: PHASED — ship the read-filter (Option A) now, side-table
      (Option B) later. B's feared consumer-breakage is DISPROVEN by prod
      (skeletons are FK-orphans: 0/3,765 in `assets` FK). Full analysis +
      0221 re-validation test design in
      notes/R-read-filter-vs-side-table.md.
---

# RESEARCH: SAC-skeleton /v1/contracts de-pollution

## Summary

`soroban_contracts` exposes **307,247** SAC-skeleton placeholder rows
(`is_sac=true`, `contract_type=0` Token, `coalesce(deployed_at_ledger,0)=0`, no
deployer) via `GET /v1/contracts`, vs only **3,906** real deployed SACs — an
~80× inflation of the public registry (total table = 424,220; all SAC =
311,153; prod-verified). This was 0294 Step 3; it is de-bundled here because the
fix is not 100%-safe (the 0221 routing leak) while the 0294 SAC-labeling core
is. Decide and implement the de-pollution.

## The two options (full analysis in [notes/R-read-filter-vs-side-table.md](notes/R-read-filter-vs-side-table.md))

- **(A) Read-filter** — append `AND NOT (sc.is_sac AND coalesce(sc.deployed_at_ledger,0)=0)`
  to `fetch_contract_list` (CH `queries_ch.rs:140` + PG `queries.rs:116`).
  Skeleton verdict rows STAY in `soroban_contracts` → the 0221 guard
  (`query_contract_verdicts`, `persist.rs:380-383`) is untouched. ~6 lines, no
  migration, **0 leak risk**. Verified safe: the list has no total-count
  (keyset-paginated on `sc.id`), and `filter[type]=token` provably keeps exactly
  the 3,906 real SACs.
- **(B) Side-table root-fix** — move the SAC routing-verdict rows OUT of
  `soroban_contracts` into a side-table; the public registry then holds only
  real deployed contracts everywhere (detail, search, counts). **Mandatory C3
  guardrail:** repoint/UNION `query_contract_verdicts` to the side-table or the
  0221 leak returns instantly. Prod-verified that NO asset/LP/search/NFT
  consumer breaks (skeletons are FK-orphans: 0/3,765 in `assets` FK, 0 in `nfts`
  FK) — the entire residual risk is the single `query_contract_verdicts` repoint.

## Recommendation

**PHASED: ship Option A now, do Option B as the root-fix later** (bundled with
0294 Step 2's orphan flip, in the 0281 maintenance window). A buys the
user-visible win immediately at near-zero risk (de-risks pre-launch); B is the
correct end-state but is migration-gated and shares a window. Once B lands, A's
predicate is redundant (or kept belt-and-suspenders).

## Acceptance Criteria

- [ ] Decision ratified (phased A→B, or A-only, or B-only)
- [ ] Option A: read-filter on `fetch_contract_list` (CH + PG), API-types/docs
      SQL fixtures updated; `/v1/contracts` excludes skeletons; `filter[type]=token`
      still returns the 3,906 real SACs
- [ ] Option B (if pursued): side-table + indexer stage write-path + the
      `query_contract_verdicts` repoint, gated by the runnable 0221 replay test
      (see note) — replay an un-deployed-SAC event, assert NOT in `nfts_pending`
- [ ] Numbers re-confirmed at run time (`coalesce(deployed_at_ledger,0)=0`
      predicate; counts drift as 0294 Step 2 flips orphans)
