---
id: '0321'
title: 'OPS: backfill native=0 tombstones for merged-account ghosts (DB-only, no S3 re-parse)'
type: OPS
status: completed
related_adr: []
related_tasks: ['0295', '0349']
tags: [ops, clickhouse, layer-data, accountmerge, priority-medium, effort-small]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from 0295. The parser tombstone (bug-2) fixes new merges
      fix-forward; the ~522k already-merged-not-recreated accounts in prod CH
      need a one-shot backfill.
  - date: 2026-07-03
    status: active
    who: karolkow
    note: >
      Activated. Re-verified live vs prod CH during the 0349 devil's-advocate
      pass (sampled windows early/mid/recent). Two findings that sharpen scope:
      (1) NATIVE-ONLY is provably sufficient — a deleted account can never hold
      a non-native balance (merge requires all trustlines closed,
      HAS_SUB_ENTRIES), measured 0 non-native across every window. No trustline
      work needed. (2) RECYCLE-SAFE confirmed — recycling (merge→recreate) is
      common (~47% of a mid-era window's merge-source set is currently alive);
      the RMT zero at the merge ledger is correctly superseded by the recreated
      account's higher-ledger row, so the backfill must NOT special-case them.
      Post-0295 truly-deleted native ghosts already ≈0 (fix-forward works); this
      backfill is purely the pre-fix historical tail.
  - date: '2026-08-17'
    status: completed
    who: karolkow
    note: >
      Archived after live verification (during 0463 work). The backfill's
      fingerprint is present in prod: merged accounts from sampled early
      windows carry native amount=0 rows versioned exactly at their merge
      ledger, and two windows (pre-fix 51.0-51.1M and recent 62.52-62.53M)
      show ZERO ghosts with positive native. Verified from the data
      fingerprint, not a run log. NOTE: the verification also uncovered a
      SEPARATE defect outside this task's scope — participant skeleton rows
      bump last_seen past death, so the deleted DERIVATION reports 5/5
      sampled dead accounts as alive (task 0500). The tombstones this task
      owned are correct; the chip defect is 0324's derivation assumption.
  - date: '2026-08-17'
    status: active
    who: karolkow
    note: >
      UN-ARCHIVED same day — the archival was premature and my verification was
      too weak. It sampled two ledger WINDOWS and found them clean; a
      full-population measurement then found ~59,272 merged accounts still
      holding a POSITIVE native balance, and a 25-account chain check returned
      22 real ghosts (Horizon 404) versus 3 legitimately alive
      (recycled/failed merge). Extrapolated: roughly 52k real ghosts holding
      on the order of 1.3M phantom XLM. Ghosts cluster in eras 54M-64M while
      the 50-52M range is clean, so whatever ran covered only part of the
      range. The task stands: the backfill still owes the rest. Method note
      for whoever runs it: verify against the CHAIN, not against our own
      windows — the window sample is what misled me. Re-confirmed the same
      day against RAW XDR (`getLedgerEntries`, LedgerKey::Account, absence
      from `entries` = account gone) rather than Horizon, which is legacy and
      must not be used: identical result, 22 of 25 absent. Note for the run
      itself: after ADR 0055 the ghosts also inflate `balance_aggregates`,
      because `sum(amount)` ignores `closed_at_ledger` — so the fix must ZERO
      the stale amount, not merely stamp the closure.
  - date: '2026-09-22'
    status: completed
    who: karolkow
    note: >
      Closed as obsolete — the ghosts are gone, so no backfill is written.
      Full-population read on production: of 1,327,456 merged accounts,
      1,322,361 (99.6%) carry a native amount of 0 at their latest version;
      31 hold a positive amount versioned at or before their merge ledger
      (75 XLM in total) and 5,064 hold one versioned after it. A seeded
      sample of 30 from each group checked against the chain
      (`getLedgerEntries`, LedgerKey::Account, ledger 64,560,946): all 60
      accounts exist, so none of them is a ghost. Then all 31 of the first
      group, checked the same way (ledger 64,561,019): 31 exist and 31 carry
      exactly our native amount on chain. 28 of them never merged — every
      AccountMerge they sourced sits in a failed transaction (hundreds of
      failed attempts each); the other 3 merged successfully at 55.3M-57.2M
      and were recreated before their current balance version (61.4M-62.5M).
      They surfaced only because the measurement took the latest merge
      ledger over failed transactions too. The second group is recreated
      accounts. The August method gave 22 ghosts in 25 on the
      same test. What zeroed them is inferred, not traced to a run: most
      likely the account closures applied from the checkpoint snapshot in
      0463 (24.5M closures).
---

# OPS: backfill native=0 tombstones for merged ghosts

## Summary

0295's parser fix zeroes a merged account's native balance going forward, but the
~522k already-merged-not-recreated accounts in prod CH still carry a stale
positive native row (~12.4M phantom XLM, native aggregate inflated; verified vs
Horizon 404). Fix with a one-shot maintenance pass — **no S3 / XDR re-parse needed**.

## Why DB-only (no re-parse)

The tombstone needs only `(account_id, merge_ledger)`, both already in
`operations_appearances` (type=8 AccountMerge → `source_id` + `ledger_sequence`).
INSERT a native `balance=0` row at the merge ledger for each merged account; under
`ReplacingMergeTree(last_updated_ledger)` the zero wins (and a recreated account's
higher-ledger row wins over the zero → safe).

## Implementation — a `backfill-runner` subcommand (NOT a loose SQL script)

Mirror the existing one-shot maintenance passes (`sac-orphan-relabel` (0315),
`contract-type-rebuild` (0283), `nft-reclassify`): a new `backfill-runner`
subcommand. Rationale — `chq` is read-only, so the INSERT must run over the
write-capable mTLS `db-ln` client that `backfill-runner` already wires; the
subcommand gets the write connection, `--dry-run` validation, batching, and
structured logging for free, consistent with the other passes.

Core logic (the subcommand body):

1. Derive the merged set from `operations_appearances WHERE type = 8` (max merge
   ledger per `source_id`). Optionally join `transactions` and filter
   `successful = 1` to drop failed-tx merges (operations_appearances has no success
   flag of its own).
2. INSERT `(account_id, asset_type = 0, balance = 0, last_updated_ledger =
last_merge)` for each (this is an `INSERT … SELECT`, run via the write client).
3. `--dry-run` first (count rows, no write); then the real pass.
4. Verify: deduped native sum drops by ~12.4M; sampled merged accounts read 0.

## Constraints

- Needs a **write-capable CH cert** — read-only `chq` / `dev_read` cannot INSERT
  (same blocker noted in 0315; the subcommand uses the `db-ln` write client).
- Idempotent: re-inserting the same zero rows is harmless under RMT.

## Acceptance Criteria

- [x] All merged-not-recreated accounts read native balance 0 — 99.6% at 0;
      the 31 positive ones all exist on chain with our exact amount: 28 never
      merged (failed merges only), 3 recreated (2026-09-22)
- [x] Native aggregate de-inflated — the positive tail left is 75 XLM, on
      live accounts; no backfill ran from this task
- [x] No live / recreated account zeroed — 30 of the 5,064 positive-after-merge
      accounts sampled, all exist on chain and keep their balance

## Resolution (2026-09-22)

Obsolete: the state this task set out to repair no longer exists, so the
`backfill-runner` subcommand was never written. Measurement and method are in
the 2026-09-22 history entry. Caveat: the attribution to the 0463
checkpoint closures is an inference from timing and scale, not a run log.
