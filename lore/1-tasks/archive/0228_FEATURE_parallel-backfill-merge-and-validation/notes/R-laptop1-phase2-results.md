---
title: 'Laptop 1 Phase 1/2 execution results — full 2/5 Soroban-era backfill'
type: research
status: mature
spawned_from: notes/S-approved-plan.md
spawns: []
tags: [laptop1, phase-1, phase-2, backfill, clickhouse, baseline-metrics]
links:
  - docs/runbooks/artifacts/laptop1_pre-export-metrics.json
  - docs/runbooks/backfill_soroban_2of5_fresh_machine.md
  - docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md
history:
  - date: '2026-05-18'
    status: mature
    who: stkrolikiewicz
    note: >
      Captured after Phase 1 (backfill) + Phase 2 (cleanup + invariants)
      completed on laptop 1 local CH. All gates green. Ready to wait for
      machine 2 + laptop 3 + Hetzner readiness before Phase 3 export.
---

# Laptop 1 — Phase 1/2 execution results

Status snapshot of the 2/5 Soroban-era backfill on laptop 1 local
ClickHouse. Captured 2026-05-18 after Phase 2 invariants closed.

Full structured artifact:
[`docs/runbooks/artifacts/laptop1_pre-export-metrics.json`](../../../../docs/runbooks/artifacts/laptop1_pre-export-metrics.json).

This note narrates the artifact + records decisions / anomalies the JSON
schema does not carry.

## Phase 1 — backfill run

| Metric                | Value                                    |
| --------------------- | ---------------------------------------- |
| Range                 | 50,457,424 → 55,103,999                  |
| Partitions            | 73 (CH-partition IDs 788–860)            |
| Ledgers indexed       | 4,646,576 (== `end - start + 1`)         |
| Ledgers already in DB | 0 (clean run, no resume)                 |
| Total bytes processed | 712,056,025,041 (~712 GB)                |
| Wall-clock elapsed    | 130,696 s (~36 h 18 min)                 |
| Parse total           | 6,735,881 ms                             |
| Persist total         | 100,653,662 ms                           |
| Per-ledger time       | 0 ms / 131 ms (min / max)                |
| Final RPC warning     | 1× `bootstrap_account_state` 429 at tail |

Density: 712 GB / 4.646 M ledgers ≈ **153 GB / M ledgers** at backfill
write-out (pre-collapse parts). Higher than plan estimate (~64.5 GB/M
post-collapse) because RMT parts are not yet compacted; relevant for
machine 2 / laptop 3 disk-pressure forecasting if planning local
`OPTIMIZE FINAL` before export.

## Phase 2 — cleanup + invariants

### 2a. Bootstrap top-up (skeleton accounts via Soroban RPC)

Tail `bootstrap_account_state` 429 in Phase 1 → ran `bootstrap` subcommand
post-backfill against `gateway.fm` RPC (public Cloudflare-protected RPC
throttled). Two passes:

| Pass | Discovered | Fetched |  Staged | RPC errors | Outcome                                      |
| ---- | ---------: | ------: | ------: | ---------: | -------------------------------------------- |
| 1    |    791,948 | 602,282 | 602,282 |          0 | Top-up after 429-truncated in-run bootstrap  |
| 2    |    189,666 |       0 |       0 |          0 | Floor confirmed — residual = merged accounts |

Pass 2 `discovered == pass1 (discovered − fetched)` exactly → Phase 2 incremental
gate (`bootstrap.rs:58-66`) correctly excluded already-enriched rows.

`fetched=0` in pass 2 ⇒ RPC has no `AccountEntry` for those 189,666 keys;
strongly implies they are `AccountMerge`'d accounts whose chain state was
deleted between the backfill window and current chain tip. They remain
permanent skeletons.

### 2b. SAC drain — `nfts_pending`

Runbook 0221 `ALTER ... DELETE` mutation:

| State      |             Row count |       Leaked SAC residual |
| ---------- | --------------------: | ------------------------: |
| Pre-drain  |            45,007,263 | (not captured separately) |
| Post-drain |            21,883,579 |                         0 |
| Δ          | −23,123,684 (−51.4 %) |                           |

Drain ratio higher than the 0220 pilot (25.7 % on 64k pilot) — sensible
for a 4.6 M-ledger range with more SAC contract churn.

### 2c. SAC drain — `nft_ownership_pending` (NEW finding)

Runbook 0221 §"What this runbook does NOT do" flags `nft_ownership_pending`
as candidate for the same leak. **Confirmed: same 51.4 % leak ratio.**

| State      |             Row count | Leaked SAC residual |
| ---------- | --------------------: | ------------------: |
| Pre-drain  |            97,952,883 |          50,325,776 |
| Post-drain |            47,627,107 |                   0 |
| Δ          | −50,325,776 (−51.4 %) |                     |

Math: 97,952,883 − 50,325,776 = 47,627,107 ✓ exact.

**Recommendation (spawned follow-up)**: update `0221_ch_drain_sac_from_nfts_pending.md`
to make `nft_ownership_pending` drain a **required** standard step of §8b,
not a "verify whether it carries the leak" optional. See Future Work below.

### 2d. §8c baseline metrics

| Metric                    |      Value | Expected (runbook)     | Verdict                                                                  |
| ------------------------- | ---------: | ---------------------- | ------------------------------------------------------------------------ |
| `accounts` (FINAL)        |  6,683,666 | 5–15 M                 | ✓ in band                                                                |
| `accounts.skeleton_pct`   |    2.859 % | < 1 % (post-bootstrap) | **floor** (merged accounts)                                              |
| `classic_credits`         |    172,092 | 100k–200k              | ✓ in band                                                                |
| `soroban_contracts` total |    174,644 | —                      | —                                                                        |
| `soroban_contracts` SAC   |    168,680 | 50k–150k               | ✓ in band (high)                                                         |
| SAC ratio                 |    96.59 % | —                      | Expected for 2/5 era — Soroban early, mostly classic-emulation contracts |
| `nfts_hot`                |          0 | 0 (post-0118)          | ✓                                                                        |
| `nfts_pending` post-drain | 21,883,579 | scale-prop             | ✓ (0118 P3 will reclassify)                                              |

Skeleton percentage exceeds the runbook's < 1 % target. **This is the
correct floor for this range, not a defect.** Runbook target is calibrated
for tip-adjacent backfill where the merged-accounts subset is tiny;
historical mid-Soroban range catches more accounts that merged between
their participation in the range and current tip.

### 2e. Hard correctness gates

| Gate                  | Expected                                             | Actual                                     | Verdict |
| --------------------- | ---------------------------------------------------- | ------------------------------------------ | ------- |
| Ledger continuity     | `min=50457424, max=55103999, count=4646576, gap=0`   | identical                                  | ✓       |
| Fact parity (tx)      | `Σ ledgers.transaction_count == count(transactions)` | both = 1,458,788,880 (bit-identical)       | ✓       |
| Parser SHA (laptop 1) | matches repo `develop` HEAD                          | `26d75f33bf2f4135f8ecbf3a93bb9c0b27b14d4a` | ✓       |

1.458 B transactions over 4.646 M ledgers = ~314 tx/ledger avg. Consistent
with Soroban-era arbitrage-bot peak activity (5–10 M tx/day mainnet).

## Decisions

### From plan

- Used `https://soroban-rpc.mainnet.stellar.gateway.fm` instead of the
  public Cloudflare-fronted RPC for bootstrap. Plan §466 (OQ #10) lists
  this as one of three viable strategies for parallel-worker RPC routing.

### Emerged

1. **Accepted floor `skeleton_pct = 2.86 %` for laptop 1 range** instead of
   chasing the runbook's < 1 % target. Reason: the residual 189,666 accounts
   verified-empty via RPC (pass 2 fetched=0); they are merged accounts whose
   `AccountEntry` no longer exists on chain. Further bootstrap passes would
   not change this. Phase 6 acceptance criterion (skeleton_pct < 1 % on
   Hetzner) remains achievable because the union over all three workers can
   recover some accounts via cross-range visibility, and post-Phase-5
   bootstrap runs against full chain state.

2. **Did NOT run local `OPTIMIZE TABLE … FINAL`** on RMT tables before
   completion. Reason: plan §152 lists this as "Hetzner only" post-merge
   step. Hardlinks-based FREEZE will preserve current part topology — local
   OPTIMIZE would burn ~hours of IO without proportional rsync-time savings.
   Reversible: if disk pressure forces it, `OPTIMIZE` is idempotent.

3. **`nft_ownership_pending` drain executed without runbook precedent**.
   0221 marks it "verify whether it carries the leak"; this run confirmed
   yes (50.3 M leaked rows) and executed the same drain pattern. See Future
   Work for runbook upgrade.

## Issues encountered

- **Public Soroban RPC 429 (Cloudflare 1015) at backfill tail.** Single WARN
  in Phase 1 logs (`bootstrap_account_state: RPC fetch failed; bailing out
err=rpc http status 429 body: error code: 1015`). Bootstrap is
  opportunistic (`bootstrap.rs:185`), so this did not fail the run — but
  it skipped one window's worth of skeleton enrichment, raising starting
  `skeleton_pct` to ~11.87 %. Resolved by post-backfill `bootstrap`
  subcommand against alt RPC. Confirms plan §466 risk: under N-way
  parallel, the same RPC will be hit harder; route workers to different
  endpoints or use a private RPC tier.

- **Shell pasted Polish prose into `--query` payload**, causing CH to error
  with `Code: 62 ... Unrecognized token (ś)`. Operator artifact, not a
  product issue. The drain SQL ran correctly before the prose token
  position — `nft_ownership_pending` pre-drain counts were captured even
  with the syntax error.

## Future work → spawn backlog tasks

1. **Upgrade `0221_ch_drain_sac_from_nfts_pending.md` runbook** to require
   `nft_ownership_pending` drain as standard, not optional. Backed by the
   51.4 % leak finding on laptop 1's range.
2. **Add `verify-local` subcommand to `backfill-runner`** that runs Phase 2
   gates (continuity, fact-parity, skeleton-floor sanity, drain residuals)
   automatically and exits non-zero on failure. Replaces this manual
   checklist. Plan 0228 already names this in Phase 2 §266 — concrete
   ticket needed.

## Status for laptop 1

**Ready for Phase 3 (FREEZE + rsync to Hetzner) when:**

- Task 0216 (Hetzner CH operational) is done — `ch-prod-01` reachable,
  schema sidecar applied, mTLS CA in place, `dict_reader` user fix
  deployed.
- Machine 2 + laptop 3 have completed their own Phase 2 and produced
  matching per-worker pre-export-metrics artifacts.

Laptop 1 local CH is now a frozen-in-process snapshot — do not run
ingest, do not run `OPTIMIZE`, do not run schema migrations on it until
Phase 3 begins.

## References

- Artifact: [`docs/runbooks/artifacts/laptop1_pre-export-metrics.json`](../../../../docs/runbooks/artifacts/laptop1_pre-export-metrics.json)
- Approved plan: [`notes/S-approved-plan.md`](S-approved-plan.md)
- Runbook (per-worker): [`docs/runbooks/backfill_soroban_2of5_fresh_machine.md`](../../../../docs/runbooks/backfill_soroban_2of5_fresh_machine.md)
- SAC drain runbook: [`docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md`](../../../../docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md)
- Bootstrap source: [`crates/backfill-runner/src/bootstrap.rs`](../../../../crates/backfill-runner/src/bootstrap.rs)
