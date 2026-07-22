---
id: '0053'
title: 'NFT membership decided at write time; the quarantine stops being fed'
status: accepted
deciders: [karolkow]
related_tasks: ['0392', '0217', '0283', '0309', '0317', '0325', '0391', '0415']
related_adrs: ['0046', '0032', '0044', '0047']
tags:
  [schema, nfts, contract-classification, indexer, persist-routing, classifier]
links: []
history:
  - date: '2026-07-22'
    status: accepted
    who: karolkow
    note: >
      Supersedes ADR 0046. Membership in `nfts` / `nft_ownership` is decided
      before the write: only a contract classified `Nft` produces rows, and
      what cannot be classified is dropped with a log line naming the
      contract. The quarantine tables stop being fed but are NOT dropped —
      they hold rows of ~28 contracts a better classifier will recognise, and
      deleting them now would destroy recoverable NFT data. The `DROP` belongs
      to the classifier follow-up (task 0309).
---

# ADR 0053: NFT membership is decided at write time; the quarantine stops being fed

**Supersedes:** [ADR 0046](./0046_classifier-quarantine-tables-nfts-pending.md) ·
**Implements:** [task 0392](../1-tasks/active/0392_BUG_nft-pending-live-routing-reconcile/README.md) ·
**Docs rule:** [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md)

## Context

ADR 0046 was right that NFT-shaped rows from an unclassified contract must not
reach `/v1/nfts*`. Its mechanism — park them in `nfts_pending` /
`nft_ownership_pending`, promote when the verdict resolves — was written for
Postgres, where promotion ran inside the `reclassify_contracts_from_wasm`
transaction. **Postgres was retired in task 0244.** ClickHouse has no per-row
`UPDATE`, promotion was never reimplemented, and the function 0046 names
(`promote_pending_nfts_to_hot`) exists nowhere in the codebase. The only drain
left was a human running `backfill-runner nft-reclassify` — last run 2026-07-16,
after the hot NFT surface had stood still for 33 days.

Two measurements reframed the problem before this decision was taken.

**The acute failure was already fixed.** PR #341 repaired the verdict prefetch
(a bare `i16` against a `Nullable(Int16)` column made it a 100% no-op for two
weeks). Since then the quarantine has received **nothing** — last write
2026-07-16, newest row 12 days old at the time of writing. What remained was a
dead mechanism, not an active leak.

**The quarantine only ever covered half the failure surface.** An NFT must pass
two independent gates: the parser (does this event _look_ like an NFT
operation?) and the classifier (is this _contract_ an NFT?). The quarantine sat
at the second gate only. Measured: **19 contracts carry an `Nft` verdict, emit
622 real events, and have no rows anywhere** — they fail at the parser, so
nothing was ever produced to quarantine. A safety net that catches half the
falls is not a safety net; it is a place where things wait forever.

## Decision

**Decide membership before the write.**

1. **`route_for` keeps only what is classified `Nft`.** `Fungible` / `Token`
   drop as they always did. A contract with no decisive verdict is **dropped** —
   it has no membership claim to make.
2. **The drop is never silent.** One `warn!` per contract per ledger names the
   contract: `0392 unclassified NFT-shaped emitter`. Dropping is the only
   outcome that can lose a real collection, so it is the one that gets a log
   line. That log is also the work queue for the classifier.
3. **The verdict lookup fails closed.** Since routing now discards what it
   cannot classify, an unavailable verdict must abort the ledger — the existing
   retry envelope plus S3 redelivery re-attempt it. The previous fail-open
   behaviour was safe only while a quarantine existed to fall through into.
4. **The verdict source is unchanged:** `soroban_contracts.contract_type`,
   stamped at deploy by the WASM classifier (G1/G9, task 0283).
5. **`nfts_pending` / `nft_ownership_pending` stop being fed but are NOT
   dropped.** No writer references them; both row structs, both `INSERT`
   streams, and `backfill-runner nft-reclassify` are deleted. The tables stay in
   `init.sql`, marked deprecated, holding their 274 + 492 rows.

## Rationale

**Why the tables survive their own retirement.** They hold rows from 66
contracts, and **28 of them are real NFT collections** — their own WASM
interfaces say so (`transfer(.., token_id: u32)`, `owner_of`); the current
name-only classifier simply cannot see it. Dropping the tables today would
delete **181 + 183 rows of genuine NFT data**, justified by a recovery path
(re-derivation from `soroban_events`) that **has not been written**. That is the
exact "later means never" pattern this task exists to end. Stopping the inflow is
a code change; deleting the data is a separate decision that belongs with the
classifier work (task 0309), when those 28 contracts can be named and their rows
reclaimed instead of discarded.

**Why the classifier is not touched here.** A signature-based rule (`transfer`'s
last parameter: `token_id` → NFT, `amount` → fungible) was written and validated
against the whole mainnet population — 127,221 contracts, **zero contradictions
with existing decisive verdicts**, 48 `Other` → `Nft`, 82 `Other` → `Fungible`.
It is deliberately **not** in this change: task 0415 established that the
classifier is refuted at the head of the traffic distribution (a price oracle
exporting `decimals` is stored as `Fungible`; 4,211 rows carry that type) and
that the real fix is typed signature sets from SEP-0048. Shipping a partial rule
here would bury that finding under a patch. The measurements are carried to 0415
so the work starts from data, not from scratch.

**Why the stamped verdict, not a live WASM classification.** Re-deriving the
verdict from `wasm_interface_metadata` on every lookup was built and measured; it
is one extra JOIN and it makes classifier changes take effect immediately. It was
rejected to keep one source of truth: `contract_type` is _meant_ to mirror the
contract's WASM, so the answer to it being stale is to refresh it, not to route
around it. Accepted consequences: a WASM upgrade carries the old verdict forward
(task 0325 — no mainnet occurrence to date), and a classifier change needs a
one-shot `backfill-runner contract-type-rebuild`. Measured 2026-07-22: **~73
contracts** currently carry a stale `Other` despite a decisive WASM, which one
rebuild clears.

**What this does not fix.** Ranked by measured harm, this is the fourth of four
findings; the first three have no scheduled work and belong to task 0415:

|     | Finding                                   | Scale                                |
| --- | ----------------------------------------- | ------------------------------------ |
| 1   | NFT ownership disagrees with the chain    | 4/4 sampled tokens of one collection |
| 2   | Parser blind to non-standard event names  | 19 collections, 622 events           |
| 3   | Classifier refuted at the head of traffic | 4,211 rows typed `Fungible`          |
| 4   | **Quarantine with no drain — this ADR**   | dormant for 12 days                  |

## Alternatives Considered

1. **Automatic promote/drop** (event-driven or scheduled). REJECTED — it is
   `nft-reclassify` with a trigger, the ownerless mop this task exists to end,
   and it could not have fired for the stuck population: on the live path a
   verdict is only written at deploy.
2. **Single table + read-time visibility filter.** Built, measured, removed. It
   works (`/v1/nfts` list 24 ms / 49k read rows → 42 ms / 239k; 5M invisible rows
   cost +13 ms because the predicate is a PK prefix) but it stores rows whose
   membership claim is false and asks every reader to remember a `WHERE`.
   Rejected on review: if a row is in `nfts`, it should be an NFT.
3. **ClickHouse view shadowing the table name.** REJECTED — renames a live table
   the indexer writes to, changes what unchanged code means, and freezes `FINAL`
   for every caller (worth up to 19× read amplification, task 0420).
4. **Read NFT ownership from ledger state instead of events.** REJECTED, and the
   reasons are protocol-level, verified against the sources on 2026-07-22:
   SEP-0050 is **Draft** and specifies no storage layout; CAP-0046-05 (**Final**)
   forbids keyspace iteration — _"no support for 'range queries', upper or lower
   bounds, or any sort of iteration over the keyspace"_; CAP-0046-12 (**Final**)
   archives persistent entries, so historical ownership is not readable from
   state at all. Probing mainnet confirmed the shape of the problem: the
   OpenZeppelin `Owner(u32)` key resolved for 6 of 7 sampled tokens of standard
   collections but for **0 of the 19** non-standard ones — contract storage keys
   are as author-defined as event names. Task 0415 owns this axis.

## Consequences

**Positive.** No promotion path to maintain, monitor, or run — the failure mode
that produced a 33-day stale NFT surface has no step left to skip. `nfts` now
carries a true membership claim, so no reader can get it wrong and no read-time
filter is needed. `nft-reclassify` and three drain runbooks are gone. What we
cannot classify is now _visible_ in the logs instead of silently accumulating.

**Negative — an unclassifiable contract's rows are dropped, not parked.** With
the classifier unchanged that includes the 28 contracts a better classifier would
recognise; their **existing** rows survive in the retained quarantine tables, but
new events from them are discarded. Recovery for anything dropped after this
change is a re-derivation from `soroban_events`, which retains every event with
full XDR (measured: 2,301 events for these contracts, 100% complete) — DB-to-DB,
no S3, but **the job does not exist yet**. This is the deliberate cost of not
shipping a partial classifier.

**Negative — the verdict lookup is now load-bearing.** A ClickHouse failure
aborts the ledger instead of degrading. That is the correct direction, but it
means NFT-bearing ledgers depend on that read succeeding.

**Negative — a stale stamp misroutes.** ~73 contracts carry `Other` despite a
decisive WASM and will have their rows dropped until a `contract-type-rebuild`
runs. Bounded, measured, clearable in one pass.

## Operational Impact

No API deploy is needed — the API is unchanged. No `DROP TABLE`, no data
migration, no point of no return:

1. **Deploy the indexer.** From here only classified NFTs are written and
   unclassifiable emitters are logged.
2. **Watch the tripwire** — `0392 unclassified NFT-shaped emitter` in the indexer
   logs is the list of contracts the classifier still cannot name.
3. **Optionally run `backfill-runner contract-type-rebuild` once** (indexer
   stopped) to clear the ~73 stale stamps. Independent and non-blocking.
4. **Deferred to the classifier follow-up (0309):** reclaim the retained
   quarantine rows for the contracts it can newly name, then `DROP TABLE` both.

Retires `docs/runbooks/0217_nfts_pending_migration_and_drain.md`,
`0221_ch_drain_sac_from_nfts_pending.md`,
`0294_ch_drain_orphans_from_pending.md` — the operations they describe no longer
exist, though the tables they mention do until step 4.

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md):

- [x] `database-schema-overview.md` — quarantine section replaced by the write-time membership contract
- [x] `clickhouse-pilot.md` — CH-side counterpart
- [x] `indexing-pipeline-overview.md` — routing description
- [x] `xdr-parsing-overview.md` — parser/classifier division of labour
- [x] `docs/backfills.md` + `crates/backfill-runner/README.md` — `nft-reclassify` removed, `contract-type-rebuild` role restated
- [x] Each updated doc links back here
- [ ] `technical-design-general-overview.md` — N/A (no topology change)
- [ ] `backend-overview.md` — N/A (no endpoint added, removed, or reshaped)
- [ ] `frontend-overview.md` — N/A (transparent to the FE)
- [ ] `infrastructure-overview.md` — N/A (no infrastructure change)

## References

- [ADR 0046](./0046_classifier-quarantine-tables-nfts-pending.md) — replaced. Its Context stays accurate on why unclassified rows must not reach the API, and why the parser cannot make that call.
- [Task 0415](../1-tasks/backlog/0415_AUDIT_authoritative-facts-ledger-not-logs.md) — owns the three larger findings and the protocol-level reasoning behind alternative 4.
- [Task 0391 §4](../1-tasks/backlog/0391_RESEARCH_nft-token-flow-coverage-audit/notes/R-nft-coverage-measured-state.md) — the measurements that opened this.
- [PR #341](https://github.com/rumblefishdev/soroban-block-explorer/pull/341) — the wire-type fix that stopped the intake before this work started.
- CAP-0046-05 (Final), CAP-0046-12 (Final), SEP-0050 (Draft) — verified 2026-07-22 for alternative 4.
