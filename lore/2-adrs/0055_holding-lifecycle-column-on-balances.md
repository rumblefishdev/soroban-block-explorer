---
id: '0055'
title: 'Holding lifecycle lives as a column on `balances`, never as a deleted row'
status: accepted
deciders: [karolkow]
related_tasks: ['0463', '0464', '0492', '0331', '0420']
related_adrs: ['0029']
tags: [clickhouse, data-model, balances, read-path, backfill]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-17'
    status: proposed
    who: karolkow
    note: >
      Decided after a planning map with five research tickets, each verified
      against production ClickHouse and raw chain XDR. Supersedes the
      read-time-RPC design sketched in task 0463's earlier notes and the
      `trustlines`-table shape sketched in task 0464; both are recorded below
      with the reasons they lost.
  - date: '2026-08-18'
    status: accepted
    who: karolkow
    note: >
      Accepted after the writer landed and the checkpoint-snapshot seed was
      built and dry-run verified. Two facts in the original text are corrected
      inline and struck through rather than deleted: the "~7 %" gap is really
      19,290,231 live trustlines (60%), and signers backward completeness is
      NOT free. Both errors came from the same cause — probes that sampled
      accounts we already hold cannot see what we never ingested. The
      checkpoint snapshot is the first source that can.
---

# ADR 0055: holding lifecycle is a column on `balances`

**Related:**

- [Task 0463: account detail — zero-balance trustlines + signers/thresholds](../1-tasks/active/0463_FEATURE_account-detail-zero-trustlines-and-signers/README.md)
- [Task 0464: balance history over time](../1-tasks/backlog/0464_FEATURE_balance-history-over-time.md)
- [Task 0492: RPC-seeded rows lack provenance](../1-tasks/backlog/0492_BUG_rpc-seeded-rows-lack-provenance-and-carry-synthetic-watermarks.md)
- [ADR 0029: no parsed-artifact S3 corpus](0029_lightweight-artifact-free-architecture.md)

---

## Context

On Stellar a holding is a **ledger entry with a lifecycle** — it is created,
it changes, it disappears. Our storage keeps a projection of that entry which
dropped the lifecycle dimension: the parser emits removal separately
(`crates/xdr-parser/src/state.rs:579-632`) and the write path collapses it to
`amount = 0` (`crates/db-clickhouse/src/persist/stage.rs:1824-1832`),
byte-identical to the live-but-empty case at `:1815-1821`.

Consequence: the read path cannot distinguish a live trustline holding nothing
from one that was closed, so it hides both (`crates/api/src/accounts/queries.rs:422`).
Issue #377 reports the visible half — an account whose AQUA, SHX and USDC
trustlines are live at zero and invisible on our page while a third-party
explorer shows them.

Removing the filter without restoring the lifecycle would resurrect closed
trustlines as ghosts; one sampled account carries 873 zero rows of which zero
are live. Stellar makes this structural: removing a trustline **requires** a
zero balance, so every closed trustline passed through zero on its way out.

The same ambiguity exists for Soroban token holdings (`state.rs:215-218`) and
LP positions (`state.rs:886-899`) — it is a property of how we record
disappearance, not of trustlines.

### Verified state of production (2026-08-17)

| Fact                                                                                                                                                                                                                                      | Source                                                                 |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `balances`: 76,179,053 rows, 7 active parts, 1.01 GiB                                                                                                                                                                                     | `system.parts`                                                         |
| `ENGINE = ReplacingMergeTree(last_updated_ledger) ORDER BY (holder_id, asset_id)`                                                                                                                                                         | `SHOW CREATE TABLE`, prod                                              |
| `balance_aggregates_mv` is **refreshable**: `REFRESH EVERY 2 MINUTE`, full recompute `FROM balances FINAL`, atomic swap                                                                                                                   | `SHOW CREATE`, prod                                                    |
| Readers of `balances`: exactly three — `accounts/queries.rs:237`, `:405`, and the MV                                                                                                                                                      | grep over the API crate                                                |
| Writer: exactly one (task 0331 "single-write")                                                                                                                                                                                            | `stage.rs:1786-1834`                                                   |
| No production code path deletes rows; every `ALTER … DELETE` in the repo is test cleanup                                                                                                                                                  | grep                                                                   |
| ~~~7 % of live zero trustlines have no row at all~~ **SUPERSEDED 2026-08-18: the real gap is 19,290,231 live trustlines — 60%.** The probe sampled accounts we already hold, so lines dormant since before the floor were invisible to it | task 0463 `notes/R-`, corrected by the checkpoint-snapshot measurement |
| ~~Accounts dormant since before our ledger floor: **0**~~ **SUPERSEDED**: the same blind spot. Forward-only indexing would leave ~94% of accounts without a signers row after a month; the seed carries 10,865,408 signer rows            | task 0463 map T4, corrected by the full-network fold                   |

---

## Decision

**Add a lifecycle column to `balances`. Never delete a row.**

```sql
ALTER TABLE balances ADD COLUMN closed_at_ledger Int64 DEFAULT 0;
-- 0  = the holding relationship is live
-- >0 = the ledger in which the entry disappeared from the chain
```

The read path filters on `closed_at_ledger = 0` instead of `amount != 0`.

### Why the entity is "holding", not "trustline"

This is the load-bearing argument, and it is a modelling argument rather than
a cost one.

Task 0331 deliberately unified every kind of holding — classic credit, native,
Soroban token — into `balances`, recorded in `schema/init.sql:423` as "the
single balance model for ALL asset types". A `trustlines` table would draw the
entity boundary at the _classic_ variant of a holding, serving one of three
kinds and forcing a second mechanism for Soroban entries — the outcome task
0463's scope research explicitly rejected. It would partially reverse 0331.

The entity is the **holding relationship**, and its table already exists. It
was missing one dimension. Adding that dimension where the entity already
lives is the smaller change _and_ the more correct model.

### Why append-only rather than deletion

- `ALTER … DELETE` is an asynchronous part-rewriting mutation, incompatible
  with bulk-insert ingest rates, and has zero production precedent here.
- Lightweight `DELETE` (available on our 26.3) masks rows via `_row_exists`
  and still schedules a mutation.
- `ReplacingMergeTree(ver, is_deleted)` makes correctness depend on merges
  reaching a single part, which task 0420 measured as not happening here
  (`balances` currently sits at 7 parts, and every read carries `FINAL`).
- ClickHouse's idiom is append-only with a version column and dedup on read.
  Versioning on the entry's own ledger makes convergence independent of write
  order, so the historical seed and the live parser may write in any order.

### Why `Int64` rather than a boolean flag

A column of almost-all-zeros compresses to near nothing either way, so the
timestamp is free — and it carries _when_, which the seed/parser seam needs,
debugging needs, and the UI may later want. `0` is safe as the live sentinel:
it is never a legal closure ledger.

### Write rules

The parser writes a **complete** row on removal —
`(holder_id, asset_id, amount = 0, closed_at_ledger = L, last_updated_ledger = L)`.
A later re-open at `L2 > L` writes `closed_at_ledger = 0, last_updated_ledger = L2`
and wins by RMT version, with no special-case logic.

Rows must always be written complete. ReplacingMergeTree replaces the **whole
row**, so a partial write silently discards fields it does not carry — proven
on an account whose stored `sequence_number` is 0 while the chain shows a real
one. This discipline is a precondition of any option, not of this one.

### Deployment order

The ClickHouse driver validates the struct against the table and rejects
inserts client-side when the table carries a column the struct does not know
and the column has no `DEFAULT` — this broke ingest for 9 minutes in task 0310.
Therefore:

1. `ALTER TABLE … ADD COLUMN … DEFAULT 0` — the existing writer keeps working,
   protected by the default;
2. deploy the writer that populates the column;
3. flip the read filter.

### Backward completeness

The destination requires backward completeness, and the read path alone cannot
deliver it: `getLedgerEntries` has no enumeration primitive, so holdings we
have no row for stay invisible however many times we ask.

A one-off seed from the history archive's checkpoint bucket list — measured at
**4.54 GB gzipped across 21 files**, decoded to confirm content — closes it.
The seed does **two** things:

1. **Fills the gap** (measured 2026-08-18 at **19,290,231** live trustlines,
   not the ~7 % first estimated): entries absent from our index are inserted,
   versioned on each entry's own `lastModifiedLedgerSeq`.
2. **Marks the closures**: `{our zero rows} − {live in snapshot}` are exactly
   the closed relationships, written with `closed_at_ledger`.

**Step 2 is the correctness core.** Flipping the read filter without it
resurrects every ghost — the 873-row account gets all 873 back.

Versioning on the entry's own ledger, never on a window boundary, is a hard
requirement: task 0492 records what the other pattern already cost us
(628,076 accounts and 897,448 balances carrying synthetic watermarks). The
correct pattern already exists in-repo at `balance_seed.rs:165`.

A re-parse was considered and is a **non-answer**: 78.85 % of chain history
predates our ledger floor, so it could not reach completeness at any price.

### Signers and thresholds

Independent of the above, and settled by the same map:

- Signers live in `AccountEntry`, rewritten on every account change. The
  parser already walks it and discards the fields
  (`ledger_entry_changes.rs:316`, `state.rs:470`); `rpc_snapshot.rs:402` holds
  them in memory before discarding them too. This is an **extraction gap, not
  a data gap**.
- Storage is a side table `account_entry_state` keyed by `account_id`, RMT
  versioned by ledger, with a single writer — not a column on `accounts`,
  whose whole-row replacement makes a bolt-on column unsafe.
- ~~**Backward completeness is free**~~ — **WRONG, corrected 2026-08-18.** The
  dormant-account census (0 of 123,772) only sampled accounts we already hold.
  Measured against the network: forward-only indexing leaves ~94% of accounts
  without a signers row after a month, so signers ride the same checkpoint seed
  as the holdings — 10,865,408 rows.

### Standing rule adopted with this decision

Third-party **interpretations** of chain data are forbidden in any runtime
path — Horizon and its kin publish their own indexes and their own field
semantics. Raw sources are permitted. The edge is drawn on **verifiability,
not ownership**: a response that can be checked (content-addressed, or hash-
chained like the history archive, which anchors to SCP) is acceptable from
anyone; a response taken on trust is not, even from our own node. And
_storing_ a foreign answer is categorically heavier than _decorating_ a
response with one, because storage is where provenance is lost.

The audit behind this rule found zero violations as written, 14 borderline
cases, and one path recommended for reclassification — recorded as task 0492.

---

## Consequences

### Positive

- The account page stops lying in both directions once the seed lands, and
  stops depending on any network call at render time.
- `balance_aggregates_mv` is **untouched**: it is refreshable rather than
  incremental, so it cannot be skewed by inserts, and neither
  `sum(amount)` nor `countIf(amount > 0)` changes when a column they do not
  reference is added. A zero-balance holding is still not a holder — that
  semantics is deliberate and preserved.
- One mechanism serves classic, native and Soroban holdings, so no kind is
  deferred into a second full backward pass (~24 h at 6 workers plus a
  mandatory `repair-tier1`, estimate extrapolated from task 0279).
- The migration is a metadata-only `ADD COLUMN`; no rewrite of 76 M rows.
- Native zero balances (239,087 holders) need **no special case** under this
  decision: a live account's zero XLM row carries `closed_at_ledger = 0` and
  shows, a merged account's native tombstone carries `closed_at_ledger > 0` and
  does not. This holds only if the writer stamps the **account-removal** path
  (`crates/xdr-parser/src/state.rs:426-449`) as well as the trustline paths —
  otherwise merged accounts render `XLM 0` when the filter flips.
  A standalone native exemption (`OR asset_type = 0` plus a handler rule
  dropping merge tombstones) was built, verified against production, and
  **reverted deliberately**: both halves are dissolved by the filter this ADR
  installs, so shipping them first would have meant a temporary special case
  whose removal depended on someone remembering.

### Negative / accepted

- `balances` gains a column that is meaningful only for entries that can
  disappear. Accepted: the alternative draws the entity boundary in the wrong
  place.
- The seed is a one-off external download and decode with real engineering
  cost, and it must be re-run if the closure derivation is found wrong.
- Until the seed completes, the read filter cannot be flipped for classic
  holdings without resurrecting ghosts. Native may go ahead of it.
- LP positions are not rendered on the account page at all today
  (`dto.rs:85-95`), and `lp_positions` is ordered `(pool_id, account_id)`, so
  account-side reads are a full scan. Write-path lifecycle for LP is in scope;
  **rendering is deliberately a separate task**.
- Soroban entries have a third state — archived-but-restorable — which the
  codebase never reads (`grep -rn evicted crates/` → nothing), so type-3
  holdings may over-report. Under investigation; does not block this decision.

### Rejected alternatives

| Alternative                      | Reason                                                                                                                                                                                                                                                                                               |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Read-time Soroban RPC            | Verified to work (131–139 ms, both halves of #377 in one round trip) but structurally cannot reach backward completeness — no enumeration primitive, so the gap (measured at 60%, not the ~7 % assumed here) stays invisible forever. Also puts an unverifiable third-party answer on a page render. |
| Read-time S3 XDR archive         | Correct but strictly dominated: 2.5 MB raw XDR per ledger, and the bucket is the public `aws-public-blockchain`, read unsigned — someone else's infrastructure exactly as much as RPC.                                                                                                               |
| Horizon                          | Third-party interpretation of chain data. Excluded by the standing rule. Measurement oracle only.                                                                                                                                                                                                    |
| `trustlines` entity table        | Entity boundary too narrow (classic-only), partially reverses task 0331's unification, forces repointing the MV that produces public `total_supply` / `holder_count`, and needs a second mechanism for Soroban holdings.                                                                             |
| Separate closures table          | Two sources of truth for one fact, plus a JOIN on the latency-critical per-account read.                                                                                                                                                                                                             |
| Numeric sentinel (`amount = -1`) | Poisons `sum(amount) AS total_supply`.                                                                                                                                                                                                                                                               |
| `Nullable(Int128)` sentinel      | Column rewrite on 76 M rows, worse compression, less legible meaning.                                                                                                                                                                                                                                |
| RMT `is_deleted`                 | Correctness would depend on merges reaching one part; task 0420 measured that they do not.                                                                                                                                                                                                           |
| Balance-history time series      | Would subsume this decision entirely — and remains the right long-term shape — but it is a separate project. Task 0464 is rewritten to carry it.                                                                                                                                                     |

---

## Notes

Task 0464 previously described the `trustlines`-entity shape and justified
deferring it on a backward-fill priced at ~168,000 RPC calls or a 13.3 M-ledger
re-parse. **Both figures were wrong**, and the re-parse could not have worked
at all. That task is rewritten as the balance-history effort, which was already
its own first trigger.
