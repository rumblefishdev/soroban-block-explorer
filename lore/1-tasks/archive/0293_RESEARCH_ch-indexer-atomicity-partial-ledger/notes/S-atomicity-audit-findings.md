---
prefix: S
title: 'Atomicity audit findings — partial-ledger crash recovery + re-run idempotency'
status: mature
spawned_from: '0293'
date: '2026-06-17'
who: karolkow
---

# Synthesis — ClickHouse indexer atomicity audit

## Verdict (TL;DR)

The **commit-marker + `ReplacingMergeTree` (RMT)** design is **sound for the
crash case it was built for**. The audit confirms the intended guarantee holds
and there is **no latent data-corruption (double-apply) bug**. The gaps that
exist are **LOW severity**: one needs a code/parser change to land *between* a
crash and its re-run, the other is a *transient* read-side duplicate that
disappears on the next background merge.

Answers to the team's three questions:

1. **Half-written ledger (rows present, no `ledgers` marker): we LEAVE the
   orphan rows** — there is no delete path. Correct by design.
2. **Re-running backfill on a "missed" ledger is SAFE.** Backfill skips ledgers
   already present in `ledgers`; a marker-less ledger is re-processed in full.
   Pre-existing partial rows do **not** break the re-run.
3. **The re-run REPLACES (collapses), it does not durably duplicate.** Re-insert
   produces deterministic surrogate keys → RMT merges old+new to one row. Two
   caveats: a *transient* duplicate exists between insert and the next merge, and
   a *narrow* orphan case survives if the emitted key set changes between
   attempts (see Step 3).

---

## Step 1 — Crash window + orphan semantics

**Write order (`crates/db-clickhouse/src/persist/writer.rs`).** `write_ledger`
buffers all entity rows and holds the `ledgers` row(s) in
`PartitionWriter::ledger_rows`. `commit()` runs in two strict phases:

- **Phase 1** (`writer.rs:284-307`): `end()` (a separate HTTP `INSERT` finalize)
  for all 18 entity tables in FK-ish order: accounts → wasm → contracts →
  transactions → hash_index → participants → pools → snapshots → lp_positions →
  operations → events → invocations → assets → nfts → nft_ownership →
  nfts_pending → nft_ownership_pending → balances.
- **Phase 2** (`writer.rs:309-317`): open the `ledgers` `INSERT`, write the
  buffered rows, `end()` — **strictly last**.

`ledgers.sequence` is therefore the **sole "fully indexed" marker**; there is no
separate status/state table (confirmed `writer.rs:275-278` doc + code).

**Crash window = the whole of `commit()` before the final `ledgers` `end()`.**
Each table is an independent HTTP request; **there is no transaction and no
rollback**. Failure points that leave orphan entity rows with no marker:

- Any of the 18 `end(...).await?` calls (`writer.rs:284-307`).
- `ledgers` open / write / end (`writer.rs:312 / 314 / 316`).
- Any panic / `SIGKILL` at any `.await?`.

On a per-ledger write error the caller invokes `abort()` (`writer.rs:328-334`),
which drops every `Insert` handle (crate aborts the in-flight request on drop)
and **writes no `ledgers` row** → resume redoes the partition cleanly. Tables
whose `end()` already ACK'd remain persisted (the orphans).

**Resume paths both trust the marker only (no orphan detection):**

- **Backfill:** `crates/backfill-runner/src/resume.rs:19`
  `SELECT sequence FROM ledgers WHERE sequence BETWEEN $1 AND $2` builds a
  `completed` set; per-ledger skip at `crates/backfill-runner/src/ingest.rs:171`
  (`if completed.contains(&seq) { continue }`). A ledger with orphan entity rows
  but **no marker** is **re-processed**.
- **Live tail:** `crates/indexer/src/handler/mod.rs:204` `SELECT max(sequence)
  FROM ledgers`, resumes from `max+1` (`mod.rs:220`).
- **No "rows exist but no marker" guard exists anywhere** (grep for `orphan`
  finds only unrelated XDR-diag and LP-sentinel code).

**Retry envelope** (`crates/indexer/src/handler/mod.rs:113`):
`RETRY_BACKOFF_MS = [50, 200, 800]` → 1 initial + 3 retries (~1.05 s) for
transient errors (`retry_with_backoff`, `mod.rs:506-537`). On exhaustion
`reconcile` returns `Err`, the doorbell is reported as a batch-item-failure
(`mod.rs:171-184`) → SQS redelivers (→ DLQ after `maxReceiveCount`). The ledger
is **never silently skipped**; the cursor stays at `max+1`. So the crash window
is hit only on a sub-second outage that *also* falls between the first entity
`end()` and the marker — rare, but non-zero.

---

## Step 2 — Re-insert idempotency per table class

**Schema (`crates/db-clickhouse/schema/init.sql`): 19 tables, 17 RMT + 2 plain
`MergeTree`** (`ledgers`, `wasm_interface_metadata`). Claim "17 of 19 RMT"
**verified**.

### Class A — event-log RMT, keyed by `(ledger, tx, …)`, NO version column (9)
`transactions`, `transaction_hash_index`, `operations_appearances`,
`transaction_participants`, `soroban_events`,
`soroban_invocations_appearances`, `nft_ownership`, `nft_ownership_pending`,
`liquidity_pool_snapshots`.

Surrogate keys are **content-derived and deterministic** (no time/random input);
a re-run of the same XDR produces **byte-identical** rows → RMT collapses
old+new on merge. With no version column the "winner" is arbitrary, but since
the rows are byte-identical that is harmless. **Idempotent. ✅**

### Class B — current-state RMT, keyed by entity, version = `last_*_ledger` (7)
`accounts` (`last_seen_ledger`), `soroban_contracts`
(`wasm_uploaded_at_ledger`), `account_balances_current` (`last_updated_ledger`),
`nfts` (`current_owner_ledger`), `nfts_pending` (`current_owner_ledger`),
`liquidity_pools` (`last_updated_ledger`), `lp_positions`
(`last_updated_ledger`).

**HIGHEST-RISK question — absolute state vs delta-accumulated — RESOLVED:
ABSOLUTE. ✅** The row value is the **post-image read straight from the ledger's
XDR `TransactionMeta`**, never `previous + delta`. There is **no
read-modify-write**, no in-memory balance carried across ledgers, no
`SELECT … FROM <state table>` before writing. Evidence:

- `account_balances_current`: `crates/db-clickhouse/src/persist/stage.rs:1113-1114`
  `balance = decimal7_string_to_i128(b.get("balance"))` — the absolute XDR
  balance. Removed trustlines emit an explicit `balance: 0` row
  (`stage.rs:1181-1188`), not a decrement. `last_updated_ledger = watermark =
  st.last_seen_ledger`. In-ledger dedup keeps the max-watermark
  (`stage.rs:1147-1157`).
- `accounts`: same `extract_account_states` source; absolute post-state.
- `lp_positions`: `shares` from the pool-share trustline `balance`
  (`crates/xdr-parser/src/state.rs:703,712`), row built at `stage.rs:641-647`.
- `nfts`: owner from the Transfer/Mint event's `to` (absolute), `stage.rs:1009-1020`.
- `assets` is RMT with **NO version column** (anomaly), keyed
  `(asset_type, asset_code, issuer_id, contract_id)`. Identity columns re-insert
  byte-identical. Its `total_supply` / `holder_count` are **not** maintained
  per-ledger; a separate **absolute recompute** pass
  (`crates/backfill-runner/src/asset_aggregates.rs`) rebuilds them with
  `sum(balance)` / `countIf(balance>0)` over `account_balances_current FINAL`,
  then `EXCHANGE TABLES`. Recompute-from-state is inherently idempotent. **✅**

Re-running ledger N re-parses the identical `LedgerCloseMeta` → identical
`(key, version, value)` → RMT collapses the re-insert onto itself. **No
double-apply.**

### Class C — plain `MergeTree`, never dedups (2)
`ledgers`, `wasm_interface_metadata`. A **duplicate `ledgers` marker row** is
*not* produced by the normal crash→resume path (resume only re-runs when the
marker is **absent**; if attempt 1 wrote the marker, resume skips). It can only
arise from **overlapping backfill/live ranges or a manual re-run**. Impact if it
happens: `resume` (`SELECT sequence …` membership) and `max(sequence)` are both
duplicate-tolerant; the only cosmetic exposure is network-stats TPS
`sum(transaction_count)` double-counting that ledger inside its 200-ledger
window (`crates/api/src/network/queries_ch.rs`). LOW.

---

## Step 3 — The orphan-that-never-dies case

RMT only replaces a key that the re-run **also emits**. An attempt-1 row for key
`K` survives forever iff **attempt 2 does not emit `K`**.

- Under **identical code + deterministic parse this is impossible**: attempt 2's
  emitted set within a table is the complete deterministic set ⊇ attempt 1's
  partial set, and keys are content-derived → every `K` reappears and collapses.
- **Reachable only via cross-attempt nondeterminism:**
  1. **Code/parser change between the crash and the re-run** (a deploy lands, the
     surrogate-key derivation or row-emission logic changes) → attempt 2 emits
     `K' ≠ K`; `K` orphan persists. **Real but narrow** (requires a deploy
     boundary to fall inside a crash window).
  2. A genuinely nondeterministic parse — **none found** (event ordering is
     deterministic; multi-transfer-in-one-ledger resolves to the same final
     owner each run).

**Blast radius:** stale rows from one crashed ledger that never overwrite — for
event-log tables a duplicate/extra event row for that ledger; for current-state
a stale extra key. Bounded to the single crash ledger. **Severity LOW.**

---

## Step 4 — "Dedup only on merge" read-side exposure

`insert_deduplicate = 0` (`writer.rs:418`) + RMT-dedup-on-merge means that
between a re-run insert and the next background merge, duplicate rows coexist.
Which reads are exposed?

**SAFE (FINAL / `argMax` / aggregate-immune):**

- `account_balances_current` canonical query (task 0198):
  `crates/api/src/accounts/queries_ch.rs:213-246` — `account_balances_current
  abc FINAL … LIMIT 1 BY (asset_type, asset_code, issuer_id)`. **Not exposed.**
- Account list (`accounts/queries_ch.rs:121`, FINAL), asset list
  (`assets/queries_ch.rs:86`, FINAL), contract list
  (`contracts/queries_ch.rs:138`, FINAL), ledger transactions
  (`ledgers/queries_ch.rs:217`, FINAL).
- Pool snapshots — `argMax(field, ledger_sequence) … GROUP BY pool_id`
  (`liquidity_pools/queries_ch.rs:643+`).
- Network TPS — `sum(transaction_count)` (immutable per ledger across attempts) +
  `system.tables.total_rows` (approximate by nature).

**EXPOSED (no FINAL → transient doubled rows in the crash→merge window):**

- `transactions` list — Statement A
  (`crates/api/src/transactions/queries_ch.rs:491`, softened by a Rust HashSet
  dedup), **Statement B** (contract filter, ~`:364`, **no dedup — highest
  exposure**), Statement C (op-type filter, ~`:426`, partial `LIMIT 1 BY t.id`).
  FINAL is **deliberately dropped** here per **ADR 0044** (no-`FINAL`-at-query-
  time invariant) because the `transactions` table is multi-billion-row and
  FINAL would scan the whole partition.

Net: read-side duplication is **only** possible (a) after an actual crash re-run,
(b) on the non-FINAL `transactions` queries, (c) until the next background merge.
Transient and rare. **Severity LOW.**

---

## Step 5 — Repair options weighed

| Option | Verdict |
|--------|---------|
| **Keep current design** (leave-orphans + RMT) | **RECOMMENDED baseline.** Correct for the common crash case; gaps are LOW + transient. Cheapest. |
| Explicit pre-insert cleanup (`ALTER … DELETE WHERE ledger_sequence = N`) | Targets the Step-3 orphan, but CH mutations are async + heavy; event-log tables `PARTITION BY intDiv(ledger_sequence, 500000)` so **`DROP PARTITION` cannot isolate one ledger** (partition = 500 k ledgers). Reserve for a guarded resume path only. |
| Experimental CH transactions (`BEGIN … COMMIT`) | **REJECT.** Experimental + single-node / non-replicated only — incompatible with the Hetzner multi-node topology (tasks 0216, 0266). |
| Two-phase staging + `MOVE/EXCHANGE PARTITION` | **REJECT.** Not on weight — on a **write-unit ≠ partition-unit mismatch**: backfill commits per 64k-ledger S3 unit, CH table partitions are 500k (`partition.rs:12` vs `init.sql:269`), ~8 writers per partition (parallel under task 0145). Atomic publish would need one writer to own a whole 500k partition + re-aligning the S3 layer. Live-tail (1 ledger) can't use it. See [R-fundamental-fix-deep-dive.md](R-fundamental-fix-deep-dive.md). |
| Read-side `FINAL` / dedup-correct queries | Viable, cheap, scoped — add `LIMIT 1 BY id` to the exposed `transactions` queries **iff** the transient dup is judged user-visible; otherwise accept per ADR 0044. |

**Recommendation:** keep the write path as-is; spawn **one** low-priority
hardening follow-up (task **0298**) bundling (1) a backfill resume **orphan
guard** for the Step-3 code-change case, and (2) a read-side decision for the
exposed `transactions` queries. No fix is *required* for correctness today.

---

## Related

- ADR 0044 (no-`FINAL`-at-query-time invariant) — `lore/2-adrs/0044_clickhouse-pilot-parallel-store.md`
- Task 0198 (canonical balances), 0217 (NFT quarantine), 0208 (LP state),
  0228 / 0194 (asset aggregates), 0216 / 0266 (Hetzner topology)
- Spawns: task 0298 (atomicity hardening)
- Deep dive (fundamental fix vs band-aid): [R-fundamental-fix-deep-dive.md](R-fundamental-fix-deep-dive.md)

---

## Addendum (2026-06-17) — corrections + RMT semantics

**Correction — commit granularity is NOT uniformly per-ledger.** Step 1 above
describes `commit()` generically; to be precise: **live-tail** opens→commits one
`PartitionWriter` **per ledger** (`persist.rs:100-105`), but **backfill** opens→
commits one `PartitionWriter` **per 64 000-ledger S3 unit** (`ingest.rs:168`
open, `:206` commit). The crash window for backfill therefore spans up to 64k
ledgers of streamed rows, not one. Resume still keys off the `ledgers` marker per
sequence. This does not change any verdict but corrects the mental model.

**Topology (gathered in the deep dive):** single-node, **non-replicated**,
ClickHouse 26.3, local SSD. No `Replicated*`, no keeper. The "3 machines" of
task 0266 are ephemeral parser workers, not replicas. This is what rules out the
replication-dependent fixes and is *favourable* for partition swaps mechanically
(no replica races) — but the 64k≠500k mismatch rules those out anyway.

**RMT dedup semantics (clarification, since the team asked).**
- A re-run `INSERT` writes a **new immutable part**; old parts are untouched, so
  **both rows coexist** until a background **merge**. A plain `SELECT` sees both;
  `FINAL` hides the dup at read time.
- On merge, for rows sharing the `ORDER BY` key, RMT **keeps the row with the
  MAX version and drops the rest** — it is a *pick-one-winner*, NOT an arithmetic
  merge and NOT "keep both". Max-version = newest ledger = correct for
  current-state (older-wins would serve stale state).
- **Equal version (same ledger re-processed) or no version column → the surviving
  row is NOT deterministic** ("last in the merge selection"). Harmless when the
  rows are byte-identical (absolute state). It becomes the **Step-3 hazard** only
  if the value differs at equal version (code/parser change between attempts),
  where RMT may keep the stale attempt-1 row. A per-attempt version column would
  give a deterministic tie-break — deferred to a future rebuild (see deep dive).

**Verdict after deep dive:** keep the write path; the fundamental alternatives
(partition swap, insert-dedup/token, version-column-everywhere) are all
**unjustified** on this single-node deployment. Idempotency-by-construction
(absolute state + deterministic keys + commit marker) already IS the root-cause
fix. Band-aid (0298) addresses the LOW residual gaps at proportionate cost.
