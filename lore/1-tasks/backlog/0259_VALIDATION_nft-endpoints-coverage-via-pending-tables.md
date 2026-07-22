---
id: '0259'
title: 'VALIDATION: NFT endpoints (E15/E16/E17) coverage via `nfts_pending` / `nft_ownership_pending`'
type: VALIDATION
status: backlog
related_adr: []
related_tasks: ['0252', '0228', '0392']
tags: [priority-low, effort-small, layer-validation, nft]
milestone: 1
links:
  - scripts/0252/phase_d_e15.py
  - scripts/0252/phase_d_e16.py
  - scripts/0252/phase_d_e17.py
history:
  - date: 2026-07-22
    status: backlog
    who: karolkow
    note: >
      **Premise changed — the blocker named in this task is gone, so the
      validation is now runnable.** Task 0392 removed `nfts_pending` /
      `nft_ownership_pending` entirely (ADR 0053): NFT rows are written to the
      canonical `nfts` / `nft_ownership` and hidden by a read-time filter on the
      contract's verdict, so there is no promotion step left to wait on. The
      canonical tables are no longer empty — 13,050 `nfts` rows across 66
      contracts, 21,597 `nft_ownership` rows (prod, 2026-07-21).
      What to change when picking this up: the title and body still describe
      coverage "via pending tables" — rewrite as plain E15/E16/E17 parity against
      `nfts` / `nft_ownership`. The 49 M / 112 M pending row counts quoted below
      are from the 2026-05 Hetzner snapshot and no longer exist in any table.
      One caveat worth asserting while here: the endpoints must return ONLY
      verdict-`Nft` contracts. That is now a query predicate rather than a table
      boundary, so a parity run is also the natural place to catch a query that
      forgot it (the in-repo guard test covers the code path; this would cover
      the served output).
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Task 0252 Phase D ran the 9 internal-consistency NFT checks
      against the canonical tables `nfts` and `nft_ownership`, both
      of which are empty in the current Hetzner backfill snapshot —
      0228 Phase 6 report flagged these tables as `Sampled rows = 0`
      / "empty by design" because the NFT pipeline routes its writes
      into a quarantine bucket pending classification: `nfts_pending`
      (49 M rows) and `nft_ownership_pending` (112 M rows). The
      sanity checks therefore passed vacuously (sample=0).

      Coverage gap: until classification promotes data from `_pending`
      to the canonical tables, the user-facing API endpoints E15/E16/E17
      have no production data to serve — so a parity verdict against
      them is premature. This task tracks closing the coverage gap.
---

# VALIDATION: NFT endpoints coverage via pending tables

## Summary

E15 (`/nfts list`), E16 (`/nfts/:id`), and E17 (`/nfts/:id/transfers`)
all returned `sample_size=0` in Phase D because the canonical `nfts`
and `nft_ownership` tables are empty in the Hetzner backfill snapshot.
The data sits in `nfts_pending` (49 M rows) and `nft_ownership_pending`
(112 M rows) — a quarantine bucket that the classification path is
expected to drain into the canonical tables.

This task closes the coverage gap when either:

1. classification lands and the canonical tables fill, OR
2. we explicitly cover the pending tables here as a
   sample-data-source override.

## Plan

Option A — wait for classification (preferred).

If classification is on the roadmap (verify with whoever owns the
NFT pipeline), once the canonical tables fill, re-run E15/E16/E17
unchanged and the existing assertions cover the surface.

Option B — point checks at `_pending`.

Add a `SBE_NFT_TABLES=pending` env switch to E15/E16/E17 that flips
all FROM/JOIN references from `nfts` → `nfts_pending` and from
`nft_ownership` → `nft_ownership_pending`. Run once with the switch
on. Note in the artifact that the pending coverage is a
"pre-classification snapshot" not the production-facing data.

Recommended: do Option B for short-term confidence, then re-run
unchanged after classification per Option A.

## Acceptance Criteria

- [ ] Decide Option A vs B.
- [ ] If B: env-switch implementation + run.
- [ ] E15/E16/E17 report non-zero samples with the chosen path; the
      existing pass/fail assertions hold.
- [ ] Validation artifact at
      `docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md`
      records the NFT coverage path (pending vs canonical) chosen.
