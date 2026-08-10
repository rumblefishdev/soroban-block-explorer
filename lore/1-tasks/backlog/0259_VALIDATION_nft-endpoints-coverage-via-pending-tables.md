---
id: '0259'
title: 'VALIDATION: NFT endpoints (E15/E16/E17) coverage via `nfts_pending` / `nft_ownership_pending`'
type: VALIDATION
status: backlog
related_adr: []
related_tasks: ['0252', '0228']
tags: [priority-low, effort-small, layer-validation, nft]
milestone: 1
links:
  - scripts/0252/phase_d_e15.py
  - scripts/0252/phase_d_e16.py
  - scripts/0252/phase_d_e17.py
history:
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
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Premise inverted — re-measure before doing anything, 2026-07-22.**
      The task exists because E15/E16/E17 returned `sample_size=0` while the data
      sat in quarantine: it cites `nfts_pending` at 49M rows and
      `nft_ownership_pending` at 112M against empty canonical tables.
      Today the numbers are the other way round: **`nfts_pending` 274 rows,
      `nft_ownership_pending` 492; canonical `nfts` 13,053 and `nft_ownership`
      21,600.** 0306's drain plus the write-time verdict fix (PR #341) emptied the
      quarantine. So the coverage question is no longer "how do we read around the
      quarantine" but simply "do E15/E16/E17 return rows now" — which is a
      five-minute check, not the task as written. Re-scope or close after that
      check; do not implement the pending-table workaround it describes.
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
