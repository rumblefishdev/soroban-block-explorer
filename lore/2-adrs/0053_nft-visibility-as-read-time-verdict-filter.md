---
id: '0053'
title: 'NFT visibility as a read-time verdict filter (replaces the nfts_pending quarantine)'
status: accepted
deciders: [karolkow]
related_tasks: ['0392', '0217', '0283', '0306', '0391']
related_adrs: ['0046', '0032', '0044', '0047']
tags:
  [schema, nfts, contract-classification, indexer, persist-routing, read-path]
links: []
history:
  - date: '2026-07-22'
    status: accepted
    who: karolkow
    note: >
      Supersedes ADR 0046. The quarantine tables are removed; visibility
      becomes a read-time predicate on the contract's current verdict.
      Decided after measuring that the design 0046 chose has no live
      promotion path at all — the only drain was a human running
      `backfill-runner nft-reclassify`, last run 2026-07-16.
---

# ADR 0053: NFT visibility is a read-time verdict filter, not a physical table split

**Supersedes:** [ADR 0046](./0046_classifier-quarantine-tables-nfts-pending.md) ·
**Implements:** [task 0392](../1-tasks/active/0392_BUG_nft-pending-live-routing-reconcile/README.md) ·
**Docs rule:** [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md)

## Context

ADR 0046 was right that NFT-shaped rows from an unclassified contract must not
reach `/v1/nfts*`. Its mechanism — park them in `nfts_pending` /
`nft_ownership_pending`, promote when the verdict resolves — was specified
against Postgres, where promotion ran in the same transaction as
`reclassify_contracts_from_wasm`. **Postgres was retired in task 0244.**
ClickHouse has no per-row `UPDATE`, and promotion was never reimplemented; the
function 0046 names (`promote_pending_nfts_to_hot`) exists nowhere in the
codebase.

So the quarantine had no exit. Measured on prod:

| Fact                                       | Value                                                                                                                                   |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| Only mechanism that ever emptied it        | a human running `nft-reclassify` (last run 2026-07-16)                                                                                  |
| Hot `nfts` frozen at ledger 62,989,407 for | 33 days (2026-07-15, [0391 §4a](../1-tasks/backlog/0391_RESEARCH_nft-token-flow-coverage-audit/notes/R-nft-coverage-measured-state.md)) |
| Intake while the verdict prefetch was dead | ~6,575 rows/day, 91% fungible                                                                                                           |
| `nfts` / quarantine, 2026-07-21            | 13,051 rows (66 contracts) / 274 rows (66 contracts)                                                                                    |
| Contracts with an `Nft` verdict            | 122 of 131,094                                                                                                                          |

A contract whose WASM is classified _after_ its first NFT event is permanent by
design — an NFT and a fungible `transfer` are byte-identical on the wire
(`from,to,token_id` vs `from,to,i128`), so only the WASM decides, and it may not
be observable yet. Those rows quarantined correctly and then stayed forever.

The root cause is not a missing job: **a mutable judgement was encoded in
immutable physical storage** — which table a row lives in. Without `UPDATE`, that
forces rows to physically move whenever the judgement changes, and something must
own the moving.

## Decision

Write every NFT-shaped row that is not _proven_ fungible into `nfts` /
`nft_ownership`; decide visibility at read time:

```sql
contract_id IN (SELECT id FROM soroban_contracts FINAL WHERE contract_type = 2)
```

Both quarantine tables are dropped, along with the `nft-reclassify` subcommand,
its runbooks, and the routing branch that fed them.

| Verdict at write time        | Action                                          |
| ---------------------------- | ----------------------------------------------- |
| `Fungible` (3) / `Token` (0) | drop — never written (**unchanged** from 0046)  |
| `Nft` (2)                    | write; visible immediately                      |
| `Other` (1) / no verdict     | **write**; invisible until the verdict resolves |

The discard rule is byte-identical to the old one, so no candidate that used to
be stored is now lost — only the destination of the undecided bucket changed.

The predicate is defined once, in `api::nfts::queries::NFT_VISIBLE`;
`crates/api/tests/nft_visibility_guard.rs` fails the build if any query reads
either table without it (or without an explicit, reasoned waiver). `FINAL` is
load-bearing: `soroban_contracts` is a `ReplacingMergeTree`, and a non-FINAL read
can serve a pre-upgrade verdict.

Nothing promotes, because nothing moves: a verdict resolving to `Nft` — live, or
via `backfill-runner contract-type-rebuild` after a classifier improvement —
makes that contract's existing rows visible on the next read.

## Rationale

**It removes the failure mode instead of automating around it.** The alternatives
all keep the split and add a moving part that must run, be monitored, and not
race the ingest inserts. Here the count of things that can silently stop working
goes from one to zero.

**The read cost was measured before deciding, not after.** Prod, 2026-07-21:

| Query                                      | Duration | Read rows |
| ------------------------------------------ | -------- | --------- |
| `/v1/nfts` list page, no filter (baseline) | 24 ms    | 49,024    |
| Same, with the visibility predicate        | 42 ms    | 238,795   |
| Predicate alone, `FINAL`                   | 23 ms    | 211,031   |
| Predicate alone, no `FINAL`                | 9 ms     | 168,404   |

`soroban_contracts` holds 131k contracts, not the millions the task 0355 note
implies — that figure described a different join. 18 ms on a page that is not on
any hot path buys the deletion of a bug class. `FINAL` is kept despite being 14 ms
of it: without it, visibility means "some version read as `Nft`", correct only
while nothing downgrades a verdict — true today, not worth making load-bearing.

**It is a net deletion:** −710 / +257 lines across 15 files.

## Alternatives Considered

1. **Continuous promote/drop triggered by classification.** Keeps both tables;
   promotes a contract's rows when its verdict resolves. REJECTED — on the live
   path a verdict is written only at deploy (`stage.rs:776`) and a WASM upgrade
   carries the old one forward (`stage.rs:255`), so for the population that is
   actually stuck (66 contracts, all `Other`) the trigger would never fire. It
   could not be demonstrated working, which task 0392 requires before deleting
   `nft-reclassify`.
2. **Scheduled reconcile sweep.** Same promote/drop on a timer. REJECTED — that
   is `nft-reclassify` with a cron, i.e. the ownerless mop this task exists to
   end, plus a cadence to tune.
3. **ClickHouse view shadowing the table name.** Enforcement at the DB level.
   REJECTED — renames a live table the indexer writes to, silently changes what
   `FROM nfts` means for unchanged code, and freezes `FINAL` for every caller (a
   knob worth up to 19× read amplification, task 0420). Replaced by an explicit
   constant plus a build-failing guard test.
4. **Keep the quarantine, add an alert** when a resolved verdict sits unpromoted.
   REJECTED — its only remediation is running `nft-reclassify`, making the human
   mop permanent and documented instead of accidental.

## Consequences

**Positive.** No promotion path to maintain, monitor, or run — the failure mode
that produced a 33-day stale NFT surface has no step left to skip. Classifier
improvements (task 0317) land instantly: flipped verdicts surface their existing
rows on the next read. Three drain runbooks and one backfill subcommand go away.
Fail-open is genuinely safe now — a failed verdict prefetch keeps the rows and
merely delays visibility, instead of parking them where nothing would drain them.

**`nfts` accumulates rows that may never become visible.** Same volume as before
(they were already written, just elsewhere), but now in the table the API reads. A
full-history backfill left 45 M rows in the quarantine before the 2026-06 drain,
about half fungible that now drops at write time — so a re-parse would leave on
the order of 20 M invisible rows against ~13 k visible. **Measured, and it does not
cost the read path:** 5,063,125 invisible rows loaded next to the real prod slice
on CH 26.3, production list query run with the junk present, then deleted:

| `nfts` contents               | Duration | Read rows |
| ----------------------------- | -------- | --------- |
| 13,325 rows (real prod slice) | 7 ms     | 179,383   |
| + 5,063,125 invisible rows    | 20 ms    | 192,473   |

380× the junk costs +13 k read rows and +13 ms — granule-boundary overhead, not a
scan: `nfts` is `ORDER BY (contract_id, token_id)` and the predicate is a prefix,
so unclassified contracts' granules are pruned before `FINAL` touches them. If
that stops holding, the answer is a skip index or partitioning, not a second
table.

**The predicate is the only barrier.** A query that omits it shows fungible tokens
as NFTs — plausible-looking and silent. The guard test was verified red before
green, on each of the four filtered call sites independently.

**One-time migration, and an irreversible `DROP TABLE`.** After the merge step the
API filter is load-bearing: rolling the API back to a pre-filter build would
expose the merged rows. Fix forward.

## Operational Impact

Deploy order is not merge order. Step 2 is the point of no return:

1. **Deploy the API** with the filter — a proven no-op on current data (0 rows in
   `nfts` carry a non-`Nft` verdict, 0 quarantined rows carry an `Nft` verdict),
   so it is verifiable in production before anything moves.
2. **Merge the quarantine** — `INSERT INTO nfts SELECT * FROM nfts_pending` and
   the ownership twin. Idempotent; column layouts verified identical.
3. **Deploy the indexer** without the quarantine branch.
4. **Repeat step 2** — rows written between 2 and 3 are still in the quarantine.
   Verify with `uniqExact`, not bare `count()` (unmerged RMT duplicates, task 0420).
5. **`DROP TABLE`** both. Reclaims 39 KiB — this change is about removing a
   failure mode, not about space.

Retires the runbooks `0217_nfts_pending_migration_and_drain.md`,
`0221_ch_drain_sac_from_nfts_pending.md`, `0294_ch_drain_orphans_from_pending.md`.

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md):

- [x] `database-schema-overview.md` §4.13.1 — quarantine replaced by the read-time filter contract
- [x] `clickhouse-pilot.md` §4c-bis — CH-side counterpart
- [x] `indexing-pipeline-overview.md` — table inventory + routing
- [x] `xdr-parsing-overview.md` — the "parser cannot discriminate, only WASM can" note
- [x] `docs/backfills.md` + `crates/backfill-runner/README.md` — `nft-reclassify` removed
- [x] Each updated doc links back here
- [ ] `technical-design-general-overview.md` — N/A (no topology change)
- [ ] `backend-overview.md` — N/A (no endpoint added, removed, or reshaped)
- [ ] `frontend-overview.md` — N/A (transparent to the FE)
- [ ] `infrastructure-overview.md` — N/A (no infrastructure change)

## References

- [ADR 0046](./0046_classifier-quarantine-tables-nfts-pending.md) — replaced. Its Context and Alternatives stay accurate on _why_ unclassified rows must not reach the API, and why the parser cannot make that call.
- [Task 0391 §4](../1-tasks/backlog/0391_RESEARCH_nft-token-flow-coverage-audit/notes/R-nft-coverage-measured-state.md) — the measurements that opened this.
- [PR #341](https://github.com/rumblefishdev/soroban-block-explorer/pull/341) — the verdict-prefetch wire-type fix that stopped the intake.
- SEP-0041 (`amount: i128`) / SEP-0050 (`token_id`) — why the two event shapes are indistinguishable.
