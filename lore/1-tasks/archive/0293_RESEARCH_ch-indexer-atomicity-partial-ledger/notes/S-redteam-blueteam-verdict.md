---
prefix: S
title: 'Red/blue team verdict — CH transactions, backup consistency, the endpoint-guard tax'
status: mature
spawned_from: '0293'
date: '2026-06-17'
who: karolkow
---

# Synthesis — adversarial verdict on a fundamental atomicity fix

Round 3. Owner asked: run red-team/blue-team on doing real transactions/atomic
commits in CH; the trigger was **backup consistency** (a backup catching the DB
between the entity writes and the `ledgers` marker), and frustration with the
"guard (FINAL) in every API endpoint" tax. Three agents (blue=for, red=against,
backup=neutral fact-find). All sourced. Verdict below.

## Headline: a fundamental fix IS available — but NOT transactions

- **Backups → quiesce-then-backup.** Fundamental, one-line, non-experimental.
- **Endpoint-guard tax → centralize read dedup (views).** It is NOT a
  write-atomicity problem and transactions cannot remove it.
- **`ledgers`/`wasm` MergeTree → RMT.** Removes the one non-self-healing residue.
- **CH transactions → REJECT.** Both teams agree; blue (the advocate) conceded.

## Backup consistency — the owner's worry is concretely REAL (and fixable)

The repo runs a **daily `BACKUP DATABASE default` at 03:30** → Borg → Hetzner
Storage Box (`infra-hetzner/ansible/roles/backup/templates/ch-backup.sh.j2:136-148`;
schedule `group_vars/all.yml:101-102`), and it runs **without quiescing the live
indexer** (deployed task 0241). So a daily archive CAN capture a torn state:
entity rows for ledger N present, `ledgers` marker for N absent.

Confirmed facts:
- CH `BACKUP` of multiple tables is **"partially consistent" by design** —
  ClickHouse#13953 (Milovidov): "not currently possible to ensure that their
  state correspond to a single point of time." Freezes are per-table, sequential.
- BUT it freezes only **committed immutable parts** → torn at the row/marker
  granularity (whole valid parts, missing marker), **not** byte-level corruption.
  A raw FS/LVM snapshot would be strictly worse (half-written parts).
- **Self-heals on restore:** restore → indexer resume (`handler/mod.rs:204-220`,
  cursor = `max(sequence)`, marker written last) re-processes ledger N → RMT
  collapses the 17 entity tables. Verified mechanism.
- **Re-derivable from S3 XDR** → worst case is "re-index the tail," never data
  loss. Backup buys RTO (hours) vs full re-index (~6 days, ADR 0045), not durability.
- The team already relied on quiesce implicitly: baseline 0260 was taken with the
  write path off ("consistent because writes off", `G-snapshot-runbook.md:249`).

**Fix (fundamental, not transactions):** quiesce the writer during the daily
backup — pause the SQS doorbell / set the indexer Lambda reserved-concurrency to
0, run `BACKUP`, resume. ~10-min window, zero correctness cost (cursor-resume).
One-line addition to `ch-backup.sh.j2`. Makes every daily archive a true
point-in-time snapshot; the torn window is eliminated at the source.

**Residue:** the 2 plain MergeTree tables (`ledgers`, `wasm_interface_metadata`)
never dedupe on merge → a torn-restore re-run can leave a DUPLICATE `ledgers`
marker row (and it doubles rows in `ledgers` JOINs). Both the backup agent and
the red team flagged this independently. Fix: convert both to RMT keyed by their
ORDER BY (no version needed) — cheap (`ledgers` ~11M rows).

## CH transactions — REJECT (blue conceded)

Blue (advocate) conceded the write-transaction path loses:
- Durability not guaranteed by default — a committed txn can come back partially
  applied after a hard crash unless fsync is on (throughput hit). CH docs +
  meta-issue ClickHouse#48794.
- Open commit-path server-crash bug THIS MONTH — ClickHouse#107446 (2026-06-14,
  v26.6); plus #85468, Keeper leak #106534.
- Requires Keeper even single-node (new stateful dependency; repo has none today).
- Crate `clickhouse 0.15.0` is HTTP-only, no txn API, sessions single-request-
  locked → would need a client fork + hand-rolled session/retry.
- Gives atomic VISIBILITY, not dedup → would not even retire RMT.

Red's kill shot: ClickHouse#104661 ("multi-table INSERT not crash-durable as a
cross-table operation") **closed as WONTFIX**. ClickHouse itself states cross-table
crash-durability is not coming. Transactions would *introduce* the inconsistency
the owner wants gone. Topology fits (single-node non-replicated is the supported
case — correcting audit-1), but the feature is the wrong trade.

## The endpoint-guard tax is RMT-structural — transactions can't touch it

The single most-cited reason for wanting a "fundamental fix" — stop writing
`FINAL` in every endpoint — is **unachievable by any write-atomicity change**:

- Every current-state table is `ReplacingMergeTree(version)` keyed by entity
  (`account_balances_current` RMT(last_updated_ledger), `accounts`
  RMT(last_seen_ledger), …). Each ledger that touches an entity writes a NEW
  version row. Multiple versions coexist until an async background merge **by
  design, in normal zero-crash operation**.
- Reads need `FINAL`/`argMax`/`LIMIT 1 BY` to pick the latest version. This is
  attributed to RMT versioning, not crashes, throughout the repo
  (`docs/architecture/database-schema/endpoint-queries-clickhouse/README.md:20,72-78`;
  ~14 guarded handlers across 6 `queries_ch.rs` modules).
- After a transactional ledger-N write commits, the table STILL holds version
  N-1 and N until merge → still need FINAL. Transactions = atomic visibility, not
  version collapse.

**Fix = read model, not write atomicity.** Options:
- `<final>1</final>` on the `read_only` profile (`profiles.xml:23-28`) — one line,
  retires ~55 guards, BUT blanket FINAL on the 3.6B-row `transactions` table
  worsens the `read_rows` quota blowups (live tasks 0290/0198). Both teams flagged
  this. Not safe as a global flag.
- **Better: dedup-correct VIEWS per current-state table** (a view that wraps
  FINAL/argMax once; endpoints read the view). Or a maintained "latest-state"
  table refreshed on a schedule. Centralizes the dedup so developers stop
  hand-writing it per endpoint — without forcing FINAL on the huge fact tables.

## Is it even a problem? (owner's question)

- **Crashes — no.** Self-heals; verified in code + e2e `persist_e2e.rs:209-216`.
- **Backups — real but immaterial today** (unquiesced cron catches a window;
  restore self-heals + S3 re-derivable). Quiesce closes it cleanly.
- **Endpoint tax — real and ongoing, but not an atomicity/crash problem** — it's
  RMT-current-state by design; fix at the read layer.

## Recommended fundamental fixes (no transactions)

1. **Quiesce-then-backup** (`ch-backup.sh.j2`) — root-cause for torn backups. Top.
2. **`ledgers` + `wasm_interface_metadata` → RMT** — kills the non-self-healing
   duplicate-marker residue + `ledgers`-JOIN doubling.
3. **Centralize read dedup via per-table views** — retires the endpoint-guard tax
   without a blanket `final=1` quota blowup.
4. **Restore drill** — 0260 never tested restore (`README:312`). High value.
5. (round 2) orphan-guard DELETE on resume + `insert_deduplication_token` (live).

## Sources
ClickHouse#13953, #4022, #104661 (WONTFIX), #107446, #85468, #106534, #48794,
#37783, #67646; CH docs transactional / ReplacingMergeTree / backup overview /
achieving-atomic-inserts / settings-query-level (final); Altinity backups +
insert_deduplication KBs. Repo: `ch-backup.sh.j2`, `group_vars/all.yml`,
`handler/mod.rs:204-220`, `writer.rs:40-48,279-318`, `profiles.xml:23-28`,
`services.xml:70-80`, `persist_e2e.rs:209-216`,
`docs/architecture/.../endpoint-queries-clickhouse/README.md:20,72-78`,
0260 runbook, ADR 0045.
