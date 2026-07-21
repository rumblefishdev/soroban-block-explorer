---
id: '0427'
title: 'RESEARCH: ClickHouse engine choice per data nature — stop making dedup every query job'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0420', '0421', '0422', '0423']
tags:
  [
    'area-clickhouse',
    'architecture',
    'adr-candidate',
    'effort-medium',
    'priority-medium',
  ]
links:
  - https://clickhouse.com/docs/engines/table-engines/mergetree-family/replacingmergetree
  - https://clickhouse.com/docs/guides/developer/deduplication
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Three findings folded in from the 0425 backfill audit — section C. They came
      from auditing the **write** path rather than engine capabilities, and they
      land on this task's immutable/mutable split from the other side.
      The immutable half gets a retirement: `docs/backfills.md` rule 4 claimed a
      re-parse with a different parser build could keep the stale row on
      version-less RMT. Measured on a 26.3 server and then confirmed read-only on
      prod — it keeps the **last row inserted**, so a re-parse always wins. Rule 4
      is rewritten and `run --reindex` is now the sanctioned repair mechanism
      (0425). The residual hazard moved to the parser: one row per key per insert.
      The mutable half gets a writer-side rule this task did not have: a whole-row
      write that defaults missing fields is safe **only if it carries the lowest
      version**. `soroban_contracts` satisfies it by accident; `accounts` violates
      it and 61.7% of recent transaction senders carry `sequence_number = 0` as a
      result. All eight state-table builders were audited — `accounts` is the only
      broken one.
      Note for whoever picks this up: an ADR was written on the finding-9 material
      and then **deleted the same day**, on the owner's call, precisely because this
      task says "no ADR should be written before [the open questions] are
      [measured]". That instinct was right and the ADR was premature. This task
      remains the ADR's home once its four open questions are answered.
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0420. Deep-research pass run against official ClickHouse docs,
      engine source and production write-ups: 103 agents, 21 sources, 105 claims,
      top 25 put through 3-reviewer adversarial verification (2 of 3 refutes
      killed a claim). Findings below are the survivors; four questions did not
      survive and need measurement on our own data before any ADR is written.
---

# RESEARCH: engine choice per data nature

## Why this exists

Task 0420 fixed eleven read paths that returned duplicated rows or inflated
counts. Every fix was correct, and every fix was also **the same fix applied
again** — because deduplication is currently the responsibility of whoever
writes the next query. Forget it once and you ship a bug that looks like
something else entirely (the original report read as a frontend memory leak).

This task asks the level-up question: **what would make dedup stop being a thing
anyone has to remember?**

## The split that organises everything

Our tables are all `ReplacingMergeTree`, but they hold two different kinds of
data, and the answer differs completely between them:

|                         | **A — immutable**                         | **B — mutable state**                                      |
| ----------------------- | ----------------------------------------- | ---------------------------------------------------------- |
| tables                  | ledgers, transactions, events, operations | accounts, assets, contracts, pools                         |
| does a row ever change? | **never**                                 | constantly (~8.46M rewrites/day for ~136k active accounts) |
| what RMT buys           | **nothing**                               | "latest wins", but lazily                                  |
| a duplicate means       | **a write bug**                           | normal operation                                           |

## Verified findings

Confidence as returned by the verification pass.

### A — immutable data

1. **Write-side deduplication is an official engine feature, and it is dead on
   our tables** (high). `non_replicated_deduplication_window` defaults to **0**;
   `replicated_deduplication_window` defaults to 10000. The asymmetry is
   compiled in (`MergeTreeSettings.cpp`), kept "for backward compatibility" —
   not a config oversight. So plain `MergeTree` + write-side dedup is a
   legitimate shape for immutable tables, and would remove read-side dedup from
   them entirely.

2. **But the window is a CORRECTNESS parameter, not a tuning knob** (high).
   Dedup is per INSERT block, keyed by a `block_id` hash of the block contents,
   held in a **finite** log. Docs, verbatim: _"If more than
   `_\_deduplication_window` other insert operations occur during the retry
   sequence, deduplication may not work as intended. In this case, the same data
   can be inserted multiple times."\* No error, no warning. For non-replicated
   tables there is **no time-based variant** of the window at all.

3. **26.3 turned insert dedup on by default — but only for `Replicated*`**
   (high). The new `deduplicate_insert` setting (26.2, supersedes
   `insert_deduplicate` / `async_insert_deduplicate`) is scoped to replicated
   tables. On our non-replicated MergeTree, `non_replicated_deduplication_window`
   still rules and dedup stays off regardless.

### B — mutable state

4. **`min`/`max` are expressible; `argMax` is a dead end** (high).
   `SimpleAggregateFunction` accepts a closed, compiled-in whitelist enforced at
   CREATE TABLE (BAD_ARGUMENTS, code 36). `min`/`max` are on it — so
   `first_seen_ledger` becomes `SimpleAggregateFunction(min, Int64)`, exactly the
   0421 recommendation. `argMax`/`argMin` are **not**, and this is deliberate:
   present briefly in 2021, then **removed by a ClickHouse co-founder because
   they crashed the server** (PR #23393). Withdrawn, not unimplemented — it will
   not quietly come back.

5. **AggregatingMergeTree does NOT remove read-side dedup** (medium) — the most
   important finding for expectation-setting. `SimpleAggregateFunction` columns
   read like normal columns (no `-Merge`/`-State`), but the engine **merges just
   as lazily as RMT**. A correct read is still `min(col), max(col) … GROUP BY
key`, or FINAL. It changes the per-column semantics; it does not change whose
   job dedup is.

6. **`SimpleAggregateFunction(max, Tuple(version, value))` as a last-value
   workaround** (low — recorded deliberately). Lexicographic tuple comparison
   yields the newest value while staying in the cheap same-type regime; write
   side is `maxSimpleState(tuple(ledger, value))`. The claim as submitted was
   refuted 0-3 (it is **not** equivalent to argMax on version ties or NULLs), but
   the idiom surfaced independently in two other reviewers' notes, once as a
   "widely used production idiom". Treat as a lead to test, not a plan.

### Taking state out of MergeTree — both exits are closed

7. **`EmbeddedRocksDB` gives a true upsert and is still disqualified** (high).
   Real last-write-wins, so zero read-side dedup. But: single-column primary key
   only, no replication (open issue #86102), not supported in ClickHouse Cloud,
   **degrades to a full scan for anything but point lookups** — which rules out
   our lists and pagination over 14M entities — and a blind upsert **destroys the
   per-column min semantics** we need for `first_seen_ledger`.

8. **The `Join` engine is a Cloud-only story** (high). "Suitable for frequent
   updates" applies to ClickHouse Cloud, where Join tables are transparently
   backed by MergeTree. In open-source there is **no background compaction** —
   `StorageSet.cpp` writes a new `.bin` per INSERT and `restore()` replays every
   one at startup. Millions of daily rewrites would accumulate files forever.

### Read-side enforcement

9. **Our pain is documented behaviour, not misconfiguration** (high). The vendor
   states RMT _"does not guarantee the absence of duplicates"_, that merging
   happens _"at an unknown time, so you can't plan for it"_, and that `count(*)`
   may return different results across runs. A statement against their own
   product, so not marketing. We relied on a promise that was never made.

10. **There is a fifth enforcement pattern we did not know about** (high). The
    documented set is: plain views (hide FINAL, store nothing), row policies
    (hide a filter), refreshable materialized views (compute the deduplicated
    result once per cycle) — **and the `final` setting, applicable per query OR
    per session, therefore pinnable in a user profile server-side**. Only the
    last is actual enforcement; 1–3 hide syntax without preventing anyone from
    querying the base table directly.

11. **The `readonly=1` blocker is not absolute** — this corrects an earlier
    conclusion in 0420 (high). A `changeable_in_readonly` constraint declared in
    the user profile lets that user change **one named setting** despite
    `readonly=1`, granting no broader write or settings permission. Verified in
    `SettingsConstraints.cpp`, not just docs. So
    `do_not_merge_across_partitions_select_final` **can** be enabled for the read
    user after all.

### C — measured 2026-07-21, from the 0425 backfill audit

Three findings arrived from the other direction — auditing the **write** path
rather than the engine — and they land squarely on this task's split.

9. **Version-less RMT keeps the last row INSERTED, not an arbitrary one** (high —
   measured, then confirmed on prod). This is the immutable half, and it retires
   `docs/backfills.md` rule 4, which claimed a re-parse with a different parser
   build could keep the stale row and therefore made `run --reindex` look unusable.
   Measured on a CH 26.3.17.4 **server** with background merges live: 40 unmerged
   old parts plus a 4-way concurrent re-parse read through `FINAL` → new value
   wins, zero survivors; background merges with no `OPTIMIZE` → new value wins;
   already-collapsed data re-parsed → new value wins; partial re-parse → the
   untouched keys correctly keep their old value. Confirmed **read-only on prod**:
   `operations_appearances` was first ingested by a pre-0261 parser emitting no
   `pool_ids`; in ledgers 50,500,000–50,510,000 the range is fully merged
   (4,429,575 rows = 4,429,575 distinct keys) and 127k path-payment ops carry
   `pool_ids`, so the later write beat the original one at scale. The real hazard
   is narrower and belongs to the parser: **two rows for one key inside a single
   insert**, where "last" is emission order (0356, pool reserves).

10. **A defaulted whole-row write is safe only if it carries the LOWEST version**
    (high — the writer-side counterpart to finding 5). `SimpleAggregateFunction`
    fixes per-column merge semantics, but nothing protects a table whose writer
    emits placeholder values on a path that also bumps the version.
    `soroban_contracts`' stub writer (`stage.rs:1761`) emits an all-NULL row and
    stamps `wasm_uploaded_at_ledger = 0`, so it always loses — safe **by accident
    of which column happens to be the version**, not by design. `accounts` breaks
    the rule: its version is `last_seen_ledger`, bumped by the very write that
    empties the row, so the emptied row wins. Version-less tables are exposed by
    construction, since there the last write always wins (finding 9).

11. **The blast radius is one builder** (high — audited, not assumed). All eight
    state-table row builders were checked against finding 10: `AccountRow` is the
    only broken one. `BalanceRow` (2 sites) carries a real amount on both paths;
    `AssetRow` writes NULL only into columns that are DEAD by design (0293/0310);
    `NftRow`, `LpPositionRow`, `LiquidityPoolRow` and `WasmInterfaceMetadataRow`
    have a single construction site each, so no partial-write variant exists.
    The `accounts` damage is measured: **61.7%** of accounts that were a
    transaction _source_ in ledgers 63,400,000–63,500,000 carry
    `sequence_number = 0`, which cannot happen on chain. `first_seen_ledger`,
    `sequence_number` and `home_domain` all ride that one write — see 0421.

## Open questions — measure, do not read

None of these survived verification, and all four are answerable on our own data.
**No ADR should be written before they are.**

- [ ] **Does `insert_deduplication_token` do anything when the window is 0?**
      Reviewer notes contradict each other: one says the token only substitutes
      the hash and does not cause hashes to be recorded (useless at window 0),
      another calls it a "robust lever" for at-least-once delivery. This is the
      decision fork for the whole immutable-data path.
- [ ] **Is a queue redelivery that batches rows differently recognised as a
      duplicate?** The mechanism hashes block CONTENT, so intuition says no — but
      three claims describing this were refuted on source quality (an eBay
      engineering blog) and the docs never state it outright. If the answer is
      no, write-side dedup requires **deterministic batching in our writer**,
      which is an architectural cost rather than a switch.
- [ ] **What does FINAL actually cost with
      `do_not_merge_across_partitions_select_final = 1`?** Our 19× was measured
      WITHOUT it. Without this number we cannot compare "FINAL enforced globally
      in the profile" against the over-fetch + Rust dedup that currently wins.
      Measure the newer FINAL machinery too (skip indexes on by default since
      25.6; automatic cross-partition decision added in 26.2).
- [ ] **How does non-replicated write dedup interact with materialized views?**
      Open issue ClickHouse#34620 reports **rows going missing** in dependent MVs
      under `non_replicated_deduplication_window`, where the replicated variant
      behaves. We keep deduplicated state in a refreshable MV (`accounts_recent`),
      so these two mechanisms would meet in one place.

## Where this points (not yet a decision)

- **Immutable tables** are the big win and the cheap one: plain `MergeTree` plus
  write-side dedup removes the whole class for ledgers, transactions, events and
  operations at once — **and would unlock projections**, since the Code 344
  refusal is specifically an RMT restriction (the reason `accounts_recent` exists
  as a refreshable-MV workaround, see 0353).
- **Mutable state** has no clean answer inside ClickHouse. `AggregatingMergeTree`
  fixes `first_seen_ledger` structurally (0421) but leaves read-side dedup in
  place; every exit from MergeTree is disqualified for our access patterns. The
  honest framing is that "current state per entity, 14M rows, 8.46M rewrites/day"
  is an OLTP-shaped workload in an OLAP database with no UPDATE.
- **Enforcement** is the cheapest immediate lever: `final` pinned in the read
  user's profile, unblocked by `changeable_in_readonly`. Cost unknown until the
  third open question is measured.

## Caveats carried from the research pass

- Insert-dedup behaviour **changed three times in six months** (26.1 decoupled
  the settings, 26.2 introduced `deduplicate_insert` and turned dedup on by
  default, 26.4 added a performance fix). Conclusions here can age.
- Most public writing on ClickHouse insert dedup describes `Replicated*` tables.
  Ours are not replicated. Check which engine any source means before believing
  it applies.
- The `Join`-engine material came from a ClickHouse marketing post arguing
  open-source limitations to sell Cloud; the reviewer neutralised it against
  Apache-2.0 source and a real issue, so the facts hold but the framing is
  partisan.
- **Nothing here was measured on our data.** The 19× and 3.3× figures come from
  the 0420 audit, not this research. The cost of EmbeddedRocksDB,
  AggregatingMergeTree, and cross-partition-disabled FINAL on our 14M entities is
  unknown, and no source prices the migration of live tables to a different
  engine under a continuous write stream.
