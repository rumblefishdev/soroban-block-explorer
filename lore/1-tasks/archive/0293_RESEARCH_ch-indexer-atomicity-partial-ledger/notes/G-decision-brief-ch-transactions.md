---
prefix: G
title: 'Decision brief — introducing ClickHouse transactions in the indexer'
status: mature
spawned_from: '0293'
date: '2026-06-17'
who: karolkow
audience: team (for joint decision)
---

# Decision Brief — Should we introduce ClickHouse transactions in the indexer?

**Status:** For team discussion → joint decision
**Scope:** ONLY the question "do we adopt ClickHouse's transaction feature
(`BEGIN TRANSACTION … COMMIT`) to make the per-ledger multi-table write atomic?"
Other hardenings (backups, read-side dedup, etc.) are out of scope here — they
are covered separately in task 0298.
**Prepared:** 2026-06-17, from the task 0293 research (incl. a sourced
red-team/blue-team review).

---

## 1. TL;DR

**Question.** Today the indexer writes ~18 tables per ledger as 18 separate
INSERTs, then a `ledgers` marker row last — there is **no transaction**, so a
crash (or a backup) can catch the DB mid-write. Should we wrap the per-ledger
write in a ClickHouse **transaction** so all 18 tables + the marker commit
atomically (all-or-nothing)?

**Recommendation: NO.** It is _technically possible_ on our topology (single-node
non-replicated is the only supported case), but:

- ClickHouse transactions are **experimental** (4 years, roadmap parked),
- **durability is not guaranteed by default** — a _committed_ transaction can come
  back **partially applied** after a hard crash unless we enable fsync (a write
  throughput hit), so it re-introduces the very inconsistency we want to remove,
- ClickHouse **closed the cross-table crash-durability request as WONTFIX**,
- there are **open server-crash bugs in the commit path filed this month**,
- it needs **ClickHouse Keeper** even on one node (we run none today),
- our Rust client **can't drive transactions without a fork**, and
- it gives atomic _visibility_, not deduplication — so it **doesn't even replace**
  the `ReplacingMergeTree` logic we already have.

The current design already handles crashes safely by **idempotent replay**
(re-run a half-written ledger → identical rows → `ReplacingMergeTree` collapses
duplicates). Transactions would add cost and risk without removing anything.

---

## 2. Why this came up — the problem transactions would solve

ClickHouse has no atomic multi-table write on the standard engine. Per ledger the
indexer issues ~18 separate INSERTs (one per entity table), then writes one
`ledgers` marker row **last** (`crates/db-clickhouse/src/persist/writer.rs:130-318`):

```
INSERT accounts; INSERT transactions; … (18 entity tables) ; INSERT ledgers(N)  ← marker last
```

If the process dies between the first entity INSERT and the marker, ledger N's
rows exist but there is **no marker** for N → a "torn"/partial ledger. The same
torn state can be captured by a **backup** taken mid-write.

A transaction would make this atomic: wrap all 18 + the marker in
`BEGIN TRANSACTION … COMMIT`; on a crash, nothing from N is visible; on success,
all of N appears together.

**How we handle it today (no transaction):** the marker is written last, so a
torn ledger has no marker; on restart the indexer resumes from
`SELECT max(sequence) FROM ledgers` and **re-indexes** the torn ledger. Because
state is the absolute XDR post-image (not a delta) and keys are deterministic
(`cityhash64`), the re-run produces **byte-identical rows**, which
`ReplacingMergeTree` collapses on the next merge. Net: crashes **self-heal**.
(Verified in code + e2e test `crates/db-clickhouse/tests/persist_e2e.rs:209-216`.)

---

## 3. Would transactions even work on our deployment? — YES

This corrects an earlier assumption ("transactions need multi-node / are
incompatible"). The opposite is true:

- ClickHouse transactions are supported **only** on **single-node, non-replicated
  MergeTree** — which is **exactly** our deployment (one Hetzner box, ClickHouse
  26.3, non-replicated, local SSD).
- A single `BEGIN TRANSACTION … COMMIT` **can** wrap INSERTs into many tables and
  commit them all-or-nothing — this is the documented, headline capability, not a
  limitation.

So "it can't run here" is false. The case against is **maturity, durability, and
cost** — §5.

---

## 4. What introducing them would require

| Requirement                        | Detail                                                                                                                                                                                                                                                         |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **ClickHouse Keeper**              | The transaction commit log (CSN) lives in Keeper/ZooKeeper — **required even on a single node**. We run none today → a new mandatory stateful service to deploy, monitor, back up.                                                                             |
| **Server config + restart**        | `allow_experimental_transactions = 1` (server-level setting).                                                                                                                                                                                                  |
| **Synchronous inserts only**       | `async_insert` is **forbidden inside a transaction** (`NOT_IMPLEMENTED`).                                                                                                                                                                                      |
| **fsync for durability**           | Without `fsync_after_insert`, a committed txn can come back partially applied after a hard crash (see §5). Enabling fsync costs write throughput.                                                                                                              |
| **A client fork / raw-HTTP layer** | The `clickhouse` crate 0.15.0 is HTTP-only, has **no transaction API**, and HTTP sessions are single-request-locked (can't hold 18 inserts open on one session). We'd hand-roll `BEGIN`/`COMMIT` over raw queries with a shared `session_id` + retry handling. |
| **Rewrite the write path**         | `writer.rs` would move from "18 independent inserts + marker-last" to "open txn → 18 sync inserts on one session → COMMIT", with rollback handling.                                                                                                            |

---

## 5. The problems (why NOT to introduce them)

| Problem                                          | Detail                                                                                                                                                                                                                  | Source                                                                                                                                 |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **Durability not guaranteed by default**         | A **committed** transaction can return **partially applied** after a hard crash unless fsync is on. This re-introduces the inconsistency we want gone, just at the storage layer.                                       | CH transactional docs; meta-issue ClickHouse#48794 ("durability is not guaranteed (and probably will never be) with default settings") |
| **Cross-table crash-durability = WONTFIX**       | "multi-table INSERT not crash-durable as a cross-table operation … each per-table commit is atomic, but the chain isn't" — **closed as not planned** by ClickHouse.                                                     | ClickHouse#104661                                                                                                                      |
| **Open commit-path server crashes — this month** | Server abort / `LOGICAL_ERROR 'txn'` on cancelled-transaction commit, filed **2026-06-14** (v26.6); plus a `std::terminate()` in commit-finalize and a Keeper memory leak.                                              | ClickHouse#107446, #85468, #106534                                                                                                     |
| **Experimental 4 years, parked**                 | Introduced 22.4 (2022), still experimental; only roadmap item ("Replicated txns") unchecked 3 years running; meta-issue closed P3; not mentioned in the 2025 roundup. Docs warn "backward compatibility can be broken." | ClickHouse#22086, #58392/#74046/#93288, #48794                                                                                         |
| **Keeper operational burden**                    | New stateful quorum service even on one node; another thing that can wedge/leak/crash and gate ingestion.                                                                                                               | CH transactional docs                                                                                                                  |
| **Client can't drive it**                        | Crate 0.15.0 HTTP-only, no txn API, sessions single-request-locked → needs a fork + manual session/retry.                                                                                                               | `Cargo.toml:43`; crate source                                                                                                          |
| **Throughput**                                   | Forced synchronous inserts + fsync + a Keeper round-trip per commit → write-path regression for a high-volume indexer.                                                                                                  | RFC ClickHouse#22086                                                                                                                   |
| **Doesn't replace RMT**                          | Transactions give atomic _visibility_, not deduplication — `ReplacingMergeTree` still dedups on merge independently. So we'd **add** a transaction layer on top, removing nothing.                                      | —                                                                                                                                      |

**The core trade:** we would swap a **known, bounded, self-healing** cost (marker

- RMT re-run reconciliation, which we control on stable code) for a **rare but
  severe, hard-to-debug** failure class (commit-path crashes, partial-apply-on-
  crash, Keeper failures) on an **experimental, unmaintained** code path. For a
  system whose goal is "no inconsistent data," that is a net **increase** in risk.

---

## 6. Options compared (transactions vs not)

| Option                                                    | Atomic write?                    | Durable on hard crash?      | New infra | Client change | Maturity                       | Verdict                                                           |
| --------------------------------------------------------- | -------------------------------- | --------------------------- | --------- | ------------- | ------------------------------ | ----------------------------------------------------------------- |
| **0. Keep as-is** — marker-last + RMT + idempotent replay | No (self-healing replay instead) | Yes (committed = on disk)   | none      | none          | stable, proven                 | **Recommended**                                                   |
| **1. Transactions, no fsync**                             | Visibility-atomic                | **No** (can return partial) | Keeper    | fork          | experimental                   | Reject — defeats the purpose                                      |
| **2. Transactions + fsync**                               | Visibility-atomic                | Yes (at a throughput cost)  | Keeper    | fork          | experimental + open crash bugs | Reject — risk/cost ≫ benefit                                      |
| **3. Transactions, live-tail only**                       | Visibility-atomic (live path)    | depends on fsync            | Keeper    | fork          | experimental                   | Reject — same risks for a 1-ledger window that already self-heals |

Note: even Option 2 (the "done right" variant) inherits the open commit-path
crash bugs (#107446) and the WONTFIX cross-table-durability stance (#104661), and
still doesn't remove the read-side `FINAL` dependency. There is **no atomic
18-table commit on ClickHouse** via any mechanism (transactions = visibility, not
durable-cross-table; partition swaps are per-table only — #37783/#67646).

**Industry check:** no blockchain indexer on ClickHouse uses CH transactions.
Goldsky/CryptoHouse use exactly our approach (ReplacingMergeTree + at-least-once

- idempotent keys). So "keep as-is" is the standard, not a shortcut.

---

## 7. Recommendation

**Do not introduce ClickHouse transactions.** Keep the current marker-last + RMT +
idempotent-replay design (Option 0). It is correct for crashes, self-healing,
proven by an e2e test, durable by default, needs no Keeper, and matches industry
practice. Transactions are experimental, not durable by default, carry live
commit-path crash bugs, require new infra and a client fork, and would not even
remove the read-side dedup we already rely on.

If the underlying worry is **partial data in backups** specifically, the
proportionate fix is to **quiesce the indexer during the nightly backup** (so no
ledger is half-written at snapshot time) — not transactions. (Tracked separately;
see task 0298 / the backup note.)

---

## 8. Open questions for the group

1. **Do we accept "no transactions"?** Or is there a scenario that justifies the
   experimental-feature + Keeper + fsync cost? (Recommendation: no.)
2. If anyone wants to keep transactions on the table: are we willing to **run and
   operate ClickHouse Keeper**, **fork/patch the client** for transaction support,
   and **enable fsync** (accepting the write-throughput hit) — to protect against
   a crash window that the current design **already self-heals**?
3. Is the real concern actually **backup consistency** (which has a much cheaper
   fix — quiesce the backup) rather than write atomicity? If so, transactions are
   solving the wrong problem.

---

## 9. Sources

**ClickHouse docs:** Transactional (ACID) support; ReplacingMergeTree.

**ClickHouse GitHub issues:**

- #22086 — transactions design RFC (single-node non-replicated; multi-table all-or-nothing).
- #48794 — meta-issue; "durability not guaranteed … with default settings"; closed P3.
- #104661 — multi-table INSERT not crash-durable as a cross-table operation — **WONTFIX**.
- #107446 (2026-06-14), #85468, #106534 — open commit-path crashes / Keeper leak.
- #58392 / #74046 / #93288 — "Replicated transactions" roadmap, unchecked 3 years.
- #37783, #67646 — no atomic multi-table swap (for context: even non-transaction atomicity isn't available).

**Our code (worktree-relative):**

- `crates/db-clickhouse/src/persist/writer.rs:130-318,40-48` — 18-insert + marker-last write path.
- `crates/indexer/src/handler/mod.rs:204-220` — resume cursor (`max(sequence)`), self-heal on restart.
- `crates/db-clickhouse/src/persist/{stage.rs:1113,ids.rs:61-109}` — absolute state + deterministic keys (idempotent replay).
- `crates/db-clickhouse/tests/persist_e2e.rs:209-216` — replay-dedup e2e test.
- `Cargo.toml:43` — `clickhouse = "=0.15.0"` (HTTP-only, no txn API).

**Deployment facts:** single Hetzner node, ClickHouse 26.3, non-replicated, no
Keeper today; data fully re-derivable from the S3 XDR archive.

**Fuller research detail (optional reading):**
`notes/R-fundamental-fix-deep-dive.md`, `notes/S-redteam-blueteam-verdict.md`.
