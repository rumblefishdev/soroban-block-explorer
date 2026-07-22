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
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Partially executed. The original method is gone, the question was
      re-derived from the protocol spec, and one headline number turned out to
      mean something narrower than it reads.**
      METHOD: `compare_e11.py` lived in `/tmp/0252/` on the Hetzner box — a
      tmpdir, so it is gone and was never in the repo. Re-derived instead from
      Horizon (the chain's own record) rather than reconstructing the script.
      WHAT "93%" ACTUALLY MEANT — it is a within-cohort rate, not an error rate.
      0255 split the population: **3,020 contracts had an explicit per-op source
      override, and 2,825 of those (93.5%) carried a wrong deployer**; the other
      **20,710 inherited the tx source, so storing tx-source was accidentally
      correct**. Against the whole universe the defect was **2,825 / 23,730 =
      12%**. Anyone reading "93%" as "most deployers were wrong" is misreading it.
      An early challenge of mine — that per-op override is pointless because
      Soroban allows one operation per transaction — is WRONG. Measured: **1.4%
      of Soroban ops carry an override** (72,839 of 5.2M in a 50k-ledger window).
      One-op-per-tx does not forbid it; the payer and the executor are simply
      different parties (relayer / sponsored-transaction pattern).
      DEFINITION OF "DEPLOYER", derived from the XDR spec and the protocol docs
      because neither 0255 nor this task ever wrote one down — which is why
      "correctness" was unmeasurable:
      **Deployer = the account that authorized the contract's creation.**
      (1) Direct deployment (top-level `CREATE_CONTRACT` / `_V2`): the effective
      operation source — `operation.sourceAccount`, falling back to
      `transaction.sourceAccount` ("If the source account parameter is omitted,
      the source account of the transaction is considered as the source account
      of the operation").
      (2) Nested deployment (factory — `CreateContract` inside a
      `SorobanAuthorizationEntry` tree): the party in that entry's
      `SorobanCredentials` — `SOURCE_ACCOUNT` → effective op source;
      `ADDRESS`/`_V2` with `SC_ADDRESS_TYPE_ACCOUNT` → that account;
      `_WITH_DELEGATES` → the primary account.
      (3) `SC_ADDRESS_TYPE_CONTRACT` → **NULL, and NULL is the correct answer.**
      `SCAddressType` really does have a contract variant (verified in
      `Stellar-contract.x`), and the docs' "Contract Invoker" rule says a calling
      contract authorizes implicitly. No account authorized it, so writing the
      fee payer there would be a fabrication.
      VERIFIED: case (1) only — 30 contracts sampled deterministically, each
      checked against Horizon's `source_account`, **30/30 match**, 0 fetch
      errors. Separately, 54 of 131,314 contracts (0.04%) carry no deployer,
      which case (3) predicts.
      NOT VERIFIED, and the reason this stays open: cases (2) and (3) have no
      test. Two attempts failed by construction — the first compared the deployer
      against the source of a LATER invocation (the sampled contracts were
      deployed in March 2024), the second looked for deployment ops in
      `operations_appearances`, which does not record them (**102 of 1,520**
      recently-deployed contracts appear there at all). Settling it needs the raw
      transaction XDR from RPC, comparing the auth tree against what was stored.
      NOTE ON EXTERNAL COMPARISON: stellar.expert's `creator` disagrees with ours
      and is NOT a valid oracle here — for factory-deployed contracts it returns
      the same account across different contracts (the factory operator), while
      the definition above returns the authorizer. Horizon is the right reference
      for case (1). This matters because 0252, the parent task, uses
      stellar.expert for Soroban entities by design — so the original harness may
      have carried this same conflation.
      Also surfaced, NOT part of this task: 4 of those 54 appear in `asset_sac`
      while their own row says `is_sac = false`. A stub row's defaults
      contradicting another table is the 0421 whole-row-default class, not a
      deployer issue.
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

> ⚠ **Re-scoped 2026-07-22 — see the history entry.** The script is gone
> (it lived in a tmpdir), "93%" turned out to be a within-cohort rate rather
> than an error rate, and the criteria below were unmeasurable because nobody
> had defined what a correct `deployer` is. A definition now exists, derived
> from the XDR spec. Case (1) verifies clean; cases (2) and (3) are untested.

- [x] ~~`compare_e11.py` re-run~~ — **impossible, replaced.** Verified against
      Horizon instead: 30 contracts, deterministic sample, **30/30 match** on
      `source_account`. Covers the direct-deployment case only.
- [x] Deployer mismatch rate < 0.1% — **met for the direct case**: 0/30
      mismatches, and 54/131,314 (0.04%) carry no deployer, which the definition
      predicts for contract-authorized deployments.
- [ ] **NEW, blocking:** verify the factory case against raw transaction XDR from
      RPC — does the stored deployer equal the `SorobanAuthorizationEntry`
      signer? Neither of the two attempts could test this
      (`operations_appearances` does not record deployments — 102 of 1,520).
- [ ] **NEW:** confirm that contract-authorized deployments are the reason for
      all 54 NULLs, rather than only some of them.
- [ ] ~~Result + sample size recorded in task body; original 0255~~ — recorded in
      the 2026-07-22 history entry, including what the 93% figure actually meant.
      size ≥ the original cohort.

## Notes

- The 0.1 % bound allows for stellar.expert classification edge
  cases (e.g. brand-new contracts whose deployer the API has not yet
  resolved). Be generous on judging "edge case" vs "real mismatch" —
  spot-check the largest outliers individually before declaring a
  regression.
- If the rate is still substantially > 0, the fix has a gap; do not
  silently relax the bound.
