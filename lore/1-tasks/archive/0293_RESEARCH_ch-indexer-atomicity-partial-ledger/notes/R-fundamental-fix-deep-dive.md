---
prefix: R
title: 'Deep dive — can we fundamentally fix atomicity (commits) instead of band-aids?'
status: mature
spawned_from: '0293'
date: '2026-06-17'
who: karolkow
---

# Research — fundamental atomicity fix vs band-aid

Triggered by user pushback on the Step-5 recommendation: _"check whether we can
fundamentally solve this with commits instead of band-aids."_ Adversarial
engineering review. **Verdict: the band-aid is the correct engineering call on
this deployment; no fundamental rewrite is justified.** The design is already a
root-cause fix — idempotency lives in the data model (absolute state +
deterministic keys + commit marker), not in a write-time atomic primitive.

## Two gating facts (new evidence)

**Fact 1 — Topology: single-node, NON-replicated, ClickHouse 26.3, local SSD.**
Schema is plain `MergeTree`/`ReplacingMergeTree`, never `Replicated*`; no
`ON CLUSTER`, no keeper/ZooKeeper. One Hetzner box (`ch-prod-01`, native ports
on loopback). The "3 machines" of task 0266 are ephemeral parser workers all
INSERTing into the one CH — not replicas. Table storage is local SSD; S3 holds
only XDR _input_.

**Fact 2 — Two write paths with different commit granularity (CORRECTS the
S-note's per-ledger framing).**

|                            | Live-tail (indexer)                 | Backfill (backfill-runner)                                                 |
| -------------------------- | ----------------------------------- | -------------------------------------------------------------------------- |
| Write unit per open→commit | **1 ledger** (`persist.rs:100-105`) | **64 000 ledgers** (one S3 partition; `ingest.rs:168` open, `:206` commit) |
| Commit marker              | 1 `ledgers` row                     | ~64k `ledgers` rows at partition end                                       |
| Crash window               | 1 ledger's ~18 inserts              | up to 64k ledgers of streamed rows                                         |

**Partition-size mismatch:** backfill work-unit `PARTITION_SIZE = 64_000`
(`partition.rs:12`) vs CH table partition `intDiv(ledger_sequence, 500000)`
(`init.sql:269`). 500000/64000 = 7.8 → **not aligned**; ~8 backfill writers feed
each CH partition.

## Approaches evaluated

1. **Per-partition staging + atomic `ATTACH/MOVE/EXCHANGE PARTITION`** —
   **REJECT.** Atomic partition publish needs one writer to own a whole 500k CH
   partition; today ~8 independent 64k writers contribute (and task 0145 plans
   them _parallel_). Would require re-aligning the backfill unit + S3 sync layer
   from 64k→500k and serializing contributors. Live-tail (1 ledger) can't use a
   500k-partition swap at all. No `*_PARTITION` op exists in the codebase today
   (only whole-table `EXCHANGE TABLES`). Net-new machinery, not reuse.

2. **Block-level insert idempotency (`insert_deduplicate` /
   `insert_deduplication_token`)** — **REJECT.** `non_replicated_deduplication_window`
   _is_ available on a single node (audit's "replication-only" instinct was
   outdated), but: (a) count-windowed, no time component → early blocks of a
   multi-minute 64k stream age out before it finishes; (b) block-hash dedup needs
   byte-identical re-chunking, which HTTP stream framing doesn't guarantee;
   (c) the deterministic-`token` variant dedups a whole INSERT as a unit but is
   per-partition + windowed, forcing small fixed batches → reintroduces the
   ~200M-parts merge storm the streaming design exists to avoid
   (`writer.rs:15-35`). Trades a LOW transient-orphan problem for a HIGH
   merge-storm problem. The existing `insert_deduplicate = 0` comment
   (`writer.rs:388-390`) under-sells the real reason (stream-resume uselessness)
   but its conclusion stands.

3. **Version column on every table (9 event-log + `assets`)** — **REJECT now,
   defer to next forced rebuild.** Closes the Step-3 determinism gap (arbitrary
   winner among equal/absent versions), but: no CH migration runner (only
   idempotent `init.sql` re-apply, `lib.rs:18,91-96`); CH cannot `ALTER` an
   engine's version clause in place → `CREATE new AS old` + `INSERT … SELECT …
FINAL` + `EXCHANGE TABLES` + `DROP` per table = full re-materialization of
   multi-billion-row tables (~800 GB, ADR 0045), days of IO, to harden a gap that
   needs a deploy-inside-a-crash-window coincidence with one-ledger blast radius.
   Cheap path: add the column at the next unavoidable full re-backfill, free at
   rebuild time.

4. **Band-aid is correct (steelman)** — **ACCEPT as baseline.** No double-apply
   (absolute XDR post-image, `stage.rs:1113-1114`); deterministic
   `cityhash64(natural_key)` surrogate keys (`init.sql:31-51`); live crash window
   = 1 ledger; backfill resume redoes the whole 64k unit. The only durable gap
   (Step-3 orphan) needs cross-attempt nondeterminism (code/parser deploy inside
   a crash window).

5. **Other native levers** — `OPTIMIZE TABLE … PARTITION p FINAL` forces a
   synchronous single-partition RMT collapse on a non-replicated table; too
   expensive to wire into the hot path on the 3.6B-row `transactions` table
   (ADR 0044), but a good **manual ops-runbook lever** after a known bad crash.
   `ALTER … DELETE WHERE ledger_sequence = N` for surgical orphan removal — async
   - heavy, reserve for the guarded resume path (task 0298). `DROP PARTITION`
     can't isolate one ledger (partition = 500k ledgers).

## Recommendation (refines task 0298)

Keep the write path. Ship 0298 as scoped, plus:

- Add `OPTIMIZE TABLE <t> PARTITION <p> FINAL` to the **ops runbook** as the
  manual lever to collapse duplicates after a known crash (not automatic).
- Defer the version-column determinism fix to the next forced full re-backfill.

"Stop patching, fix the root cause" does not apply: every fundamental option
either reintroduces the parts-explosion the design prevents, requires
re-architecting the 64k→500k backfill+S3 layer, or costs days of full rebuild to
harden a LOW-severity, one-ledger-blast-radius gap. Idempotency-by-construction
already IS the root-cause fix.

## Round 2 — sourced web research (CH transactions + alternatives + industry)

### CORRECTION to round 1 / audit-1

Audit-1 (S-note Step 5) rejected experimental CH transactions as "single-node /
non-replicated only — incompatible with the Hetzner multi-node topology". **That
reasoning was wrong:** the deployment IS single-node non-replicated, which is
**the only SUPPORTED topology** for CH transactions. The conclusion (don't use
them) stands, but for stronger, correct reasons below.

### CH experimental transactions — mechanically viable here, but REJECT

- Multi-table all-or-nothing across ~18 tables + marker in one `BEGIN…COMMIT` is
  the explicit supported capability on single-node non-replicated MergeTree
  (docs `guides/developer/transactional`; RFC ClickHouse#22086).
- **Killers:** (1) **durability NOT guaranteed by default** — a committed txn can
  come back _partially applied_ after a hard crash without fsync (meta-issue
  ClickHouse#48794; RFC#22086) → reintroduces the inconsistency we want gone.
  (2) Experimental 4 yrs, roadmap parked (P3; "Replicated txns" unchecked 3 yrs:
  ClickHouse#58392/#74046/#93288). (3) **Open commit-path server-crash bugs this
  month** — ClickHouse#107446 (2026-06-14), #85468, #106534 (Keeper leak).
  (4) Requires Keeper even single-node. (5) `clickhouse` crate 0.15.0 = HTTP-only,
  no txn API, HTTP sessions single-request-locked → would need a client fork +
  hand-rolled `session_id`/373-retry. (6) Gives atomic _visibility_, not dedup —
  wouldn't replace RMT.
- **Trade verdict:** swaps a known/bounded/controllable cost (marker+RMT) for a
  rare-but-severe uncontrollable tail (commit crashes, partial-rollback-on-crash,
  Keeper ops) on unmaintained code. Wrong trade for a production indexer.

### True-atomicity alternatives (no transactions) — also insufficient

- `REPLACE PARTITION` / `EXCHANGE TABLES` are atomic **per single table only**;
  **no atomic multi-table swap exists** (open FR ClickHouse#37783). 18 tables = 18
  independent atomic ops → torn (but recoverable) state. Partition axis is
  misaligned (64k = S3 folder, NOT the 500k CH partition), ~30 hardcoded `500000`
  sites in API, and 8 unpartitioned state tables can't `REPLACE PARTITION` at all.
  High cost, still not atomic.
- **There is NO atomic 18-table commit on ClickHouse MergeTree, via any
  mechanism.** Confirmed.

### Two genuine CH-native (non-experimental) wins surfaced

- **`insert_deduplication_token`** (`ledger-N-table`) → insert-time idempotency:
  a duplicate **never lands** (vs RMT which lets it land then merges). Works for
  **live-tail** (1 ledger fits one block); not clean for the 64k backfill stream
  (many blocks). Constraints: single-block-per-partition, identical retry
  settings, `non_replicated_deduplication_window` sized > retry lag; verify the
  crate transmits the token (clickhouse-java#1877 dropped it historically).
- **Orphan-guard lightweight DELETE on resume** — mechanism already in repo
  (`ALTER … DELETE … mutations_sync=1`: `bootstrap.rs:477`, `sink.rs:298`,
  `nft_reclassify.rs:54-60`). Deterministic immediate cleanup of the Step-3
  orphan. ~1-2 days, resume-path only, zero schema/API change.

### Industry validation

No blockchain indexer on ClickHouse uses CH transactions. **Goldsky / CryptoHouse
use exactly this codebase's approach** (ReplacingMergeTree + at-least-once,
dedup-on-merge eventually-consistent). Subsquid / Substreams use a
changelog-table + cursor + finality-gated inverse-op replay (for reorgs). So the
marker+RMT design is the industry-standard answer, not a shortcut.

### Adjacent finding (not atomicity, but consistency)

`repair_tier1.rs` exists because the **unpartitioned STATE tables** (accounts,
account_balances_current, …) can "silently corrupt under cross-machine RMT
collapse" in parallel backfill (`main.rs:148-157`). This is a separate, possibly
higher-impact consistency concern than the Step-3 orphan — worth confirming the
repair pass is wired into the K-parallel backfill flow.

### Sources

CH docs `guides/developer/transactional`, `deduplicating-inserts-on-retries`,
`sql-reference/statements/alter/partition`; ClickHouse issues #22086, #48794,
#37783, #107446, #85468, #106534, #58392/#74046/#93288; Altinity KB
`atomic-insert` / `insert_deduplication`; Goldsky+ClickHouse blog; Subsquid SDK;
Substreams reorg-handling docs.

## Key file:line

- Topology: `docker-compose.prod.yml`, `docker-compose.yml:47` (CH 26.3),
  `init.sql` (no `Replicated*`), task 0266 README architecture diagram.
- Granularity: `crates/db-clickhouse/src/persist.rs:100-105` (live per-ledger),
  `crates/backfill-runner/src/ingest.rs:168,206` (backfill per-64k),
  `crates/backfill-runner/src/partition.rs:12` vs `schema/init.sql:269`.
- Settings: `crates/db-clickhouse/src/persist/writer.rs:388-390,412-421`.
- EXCHANGE TABLES precedent: `crates/backfill-runner/src/asset_aggregates.rs:147`,
  `repair_tier1.rs:357-399`.
- No migration runner: `crates/db-clickhouse/src/lib.rs:18,91-96`.
