---
id: '0256'
title: 'VALIDATION: Phase 3 — re-run compare_e11.py to confirm deployer mismatch < 0.1 % post Phase 1'
type: VALIDATION
status: backlog
related_adr: []
related_tasks: ['0255', '0252', '0241']
tags: [priority-medium, effort-small, layer-validation, data-correctness]
milestone: 2
links:
  - lore/1-tasks/archive/0255_BUG_parser-deployer-id-op-source-semantic.md
  - lore/1-tasks/active/0252_VALIDATION_clickhouse-endpoint-parity-against-stellar-apis.md
  - /tmp/0252/compare_e11.py
history:
  - date: '2026-05-22'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from task 0255 Phase 1 follow-up. The parser fix shipped
      via PR #213 covers live-mode ingestion going forward, but the
      original 0255 acceptance criterion ("E11 deployer field mismatch
      rate < 0.1 %") cannot be measured until live mode is actually
      deployed (post task 0241 cutover) and the new parser has chewed
      through some volume of fresh deploys. Spawned as a standalone
      validation task so 0255 can archive cleanly.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Blocked on a missing artifact — checked 2026-07-22.**
      The task says to *re-run* `compare_e11.py`. That script **is not in the
      repository and never has been** — `git log --diff-filter=A` across all refs
      returns nothing for it. It was presumably a local file on whoever ran Phase
      3 originally.
      So this cannot be executed as written. Either the script is recovered from
      its author, or the deployer-mismatch check is re-derived from scratch (in
      which case it is a new task, not a re-run). Needs an owner decision before
      it can be scheduled.
---

# VALIDATION: Phase 3 — re-run compare_e11.py to confirm deployer mismatch < 0.1 %

## Summary

Phase 3 of the 0255 fix arc: once live mode is rolled out (post task
0241 cutover) and the post-fix parser has ingested some volume of
fresh Soroban contract deploys, re-run task 0252's `compare_e11.py`
against the migrated + freshly-ingested CH state and confirm the
deployer field mismatch rate has dropped from the pre-fix ~93 %
(within sampled cohort) to under 0.1 %.

## Context

- Phase 1 (parser fix, PR #213) closes the accumulation surface but
  is dormant until the indexer Lambda runs the new image.
- Phase 2 (Hetzner CH backfill, 2026-05-22) corrected the 2,825
  misattributed rows in the existing snapshot. Spot-check on
  `CB5GADAT…JJGD` already passed against stellar.expert canonical.
- Phase 3 is the closing-loop verdict: does the deployer column hold
  up across a broad sample post-fix, post-cutover?

## Implementation

1. Wait for task 0241 cutover (live mode running on the new
   `xdr-parser` build).
2. Re-deploy / re-run task 0252's `/tmp/0252/compare_e11.py` on the
   Hetzner CH box (the script is already in place from earlier 0252
   work; see `[[hetzner-ch-artifacts]]` for paths).
3. Sample size ≥ the original E11 cohort. Compare with the pre-fix
   summary at `/tmp/sbe-artifacts/0252/phase_b_e11_summary.json` for
   apples-to-apples deployer-field accounting.
4. Record the new deployer mismatch rate. Update task 0252 Phase B
   summary if applicable.

## Acceptance Criteria

- [ ] `compare_e11.py` re-run on post-cutover CH state with sample
      size ≥ the original cohort.
- [ ] Deployer field mismatch rate < 0.1 % (allowing stellar.expert
      classification edge cases). If higher, root-cause and feed back
      into a follow-up parser fix.
- [ ] Result + sample size recorded in task body; original 0255
      acceptance criterion marked done at the cross-link.

## Notes

- The 0.1 % bound allows for stellar.expert classification edge
  cases (e.g. brand-new contracts whose deployer the API has not yet
  resolved). Be generous on judging "edge case" vs "real mismatch" —
  spot-check the largest outliers individually before declaring a
  regression.
- If the rate is still substantially > 0, the fix has a gap; do not
  silently relax the bound.
