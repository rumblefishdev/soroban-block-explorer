---
id: '0463'
title: 'FEATURE: account detail — show zero-balance trustlines + signers/thresholds'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0464', '0321', '0331', '0295', '0214']
tags:
  [
    frontend,
    backend,
    account-detail,
    clickhouse,
    soroban-rpc,
    priority-medium,
    effort-medium,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Triaged from issue #377 (two asks in one report, both on the account
      detail page). Claims verified against prod ClickHouse and Horizon
      before filing.
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Converted to a directory after a 200-account measurement (notes/R-)
      and a source comparison (notes/S-). Three things changed: the feature
      matters more than assumed (97.8% of hidden rows on typical accounts
      are live trustlines), Horizon is excluded from the runtime path by
      decision, and an earlier claim of mine — that signers can never be
      indexed — was wrong and is corrected in notes/S-. Design is NOT final:
      the signers/balances question is being re-researched in a fresh
      session.
  - date: '2026-08-17'
    status: active
    who: karolkow
    note: >
      Activated. Opened with the design still unsettled on purpose — the
      solution space is being re-planned from scratch before any code, since
      option A was chosen for cost rather than fit and the signers half may
      belong to a different option entirely.
---

# FEATURE: account detail — zero-balance trustlines + signers/thresholds

## Summary

Two gaps on the account detail page, reported together in issue #377:

1. **A trustline that exists but holds 0 is invisible.** The read path drops
   it. An established trustline at zero is a real fact — the account can
   receive that asset.
2. **Signers and thresholds are not shown at all.** Nothing on the page says
   whether an account is multisig. We do not index this today.

## The trap that makes it non-trivial

A **removed** trustline is written as `amount = 0` too
(`persist/stage.rs:33-40`, write site `:1686`), so the two are byte-identical
in ClickHouse. Deleting the `amount != 0` filter resurrects closed trustlines
as ghosts — the inverse of the merged-account ghosts in task 0321.

Stellar's own docs confirm this is structural, not accidental: removing a
trustline **requires a zero balance** (`CHANGE_TRUST_INVALID_LIMIT` —
"attempting to remove a trustline with a non-zero asset balance"). Every
closed trustline passed through zero on its way out.

## Fixture and evidence

Reported account `GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN`:
five rows in `balances`, two rendered. AQUA / SHX / USDC sit at 0 and Horizon
confirms all three trustlines are live. The same account carries 5 ed25519
signers at weight 1 with thresholds 3/3/3 — genuinely multisig, presented by
us as ordinary.

Filter: `crates/api/src/accounts/queries.rs:422`.

Measurements and the full option comparison live in the notes:

- [`notes/R-zero-balance-probe.md`](notes/R-zero-balance-probe.md) —
  200-account probe, bimodal distribution, the 33.6 M ambiguous-pair count.
- [`notes/S-source-options.md`](notes/S-source-options.md) — every source
  considered, what is ruled out and why, and two corrections to earlier claims.

The earlier read-time-RPC design in `notes/S-` is **superseded** — it cannot
reach backward completeness, because `getLedgerEntries` has no enumeration
primitive. Keep the notes as the record of how the decision was reached; do
not implement from them.

Planning map (local, gitignored, not in the repo): `.wayfinder/0463/` — the
decision trail, five resolved research tickets with their measurements, and
the implementation tickets.

## Decided design — [ADR 0055](../../../2-adrs/0055_holding-lifecycle-column-on-balances.md)

The lifecycle becomes a **column on `balances`**; rows are never deleted.

```sql
ALTER TABLE balances ADD COLUMN closed_at_ledger Int64 DEFAULT 0;
```

The entity is the **holding relationship**, not the trustline: the same
ambiguity affects Soroban and LP holdings, and task 0331 already unified every
holding kind into this one table. The read path filters on
`closed_at_ledger = 0` instead of `amount != 0`.

Signers take a side table `account_entry_state` (single writer, RMT by ledger) —
not a column on `accounts`, whose whole-row replacement makes a bolt-on unsafe.

Backward completeness comes from a one-off seed of the history archive's
checkpoint bucket list (**4.54 GB gzipped, 21 files**, measured), which both
fills the ~7 % we hold no row for **and** derives the closures as
`{our zero rows} − {live in snapshot}`. Without that second step, flipping the
filter resurrects ghosts.

Full reasoning, rejected alternatives and the measured production facts are in
the ADR.

## Work breakdown

1. **Lifecycle column + writer** — `ALTER` with `DEFAULT`, then the writer,
   covering classic, Soroban and LP write paths together (deferring any kind
   costs a second full backward pass). Must also stamp the **account-removal**
   path (`state.rs:426-449`), so a merged account's native tombstone carries
   `closed_at_ledger` and native needs no special case downstream. Deployment
   order is load-bearing — task 0310.
2. **Signers extraction** — the writer half is parallel to the above, but the
   history is **not** free: forward-only indexing would leave ~94 % of accounts
   without a signers row after a month (830,014 of 14,509,686 accounts moved in
   the last ~30 days). An empty signers section reads as "not multisig", so the
   API and UI wait for the seed, or ship an explicit "not indexed" state.
3. **Seed from the checkpoint snapshot** — fills gaps and marks closures, and
   carries **`AccountEntry` too**, so signers get their history from the same
   artifact. Version on each entry's own `lastModifiedLedgerSeq`, never on a
   window boundary (task 0492). **Coverage must be measured afterwards and
   cross-checked against an independent source regardless of the result** — a
   standing requirement, not a nicety. (Originally the RPC route; that
   comparator was deleted 2026-08-21, so the check is now the 200-account
   probe against the chain plus the aggregate deltas.)
4. **Flip the read filter + production verification** — only after the seed
   verifies. This is where **native** becomes visible too (239,087 holders):
   under `closed_at_ledger = 0` it needs no exemption of its own. A standalone
   native read-filter patch was built and reverted on purpose — it would have
   been a temporary special case dissolved by this very step.

## Scope

**In:** classic, native and Soroban holdings; the LP **write path**; signers
and thresholds.

**Out:** rendering LP positions on the account page — task 0493, because the
page renders none today and `lp_positions` is ordered `(pool_id, account_id)`,
making the account-side read a full scan. Balance history over time — task 0464.

**Under investigation, non-blocking:** Soroban entries have an
archived-but-restorable state the codebase never reads, so type-3 holdings may
over-report.

## Status 2026-08-18 — seed built and dry-run verified; production write is the next move

Everything below is measured on production data and RPC-verified, not estimated.

### What exists in code (this branch, committed)

1. **Lifecycle writer** — the indexer now stamps `closed_at_ledger` (and zeroes
   the amount) when a trustline is removed and when an account is merged, for
   classic, native, Soroban and LP write paths. It also extracts signers +
   thresholds into `account_entry_state`. This is NOT deployed yet.
2. **Checkpoint-snapshot toolchain** — ONE `backfill-runner` subcommand,
   `snapshot-seed`, read-only except its explicit `--execute`. Its dry-run
   decodes the archive's full-state snapshot (4.44 GB, 21 buckets, ~6 min),
   folds it first-wins into DISTINCT entries, compares our `balances` against
   it and writes the artifacts (`manifest.json`, `summary.txt`, `ghosts.tsv`,
   `dumps/`); `--execute` additionally inserts.

   _Historical:_ this began as four commands. `snapshot-tally` and
   `snapshot-dedup` were research probes, deleted in the 2026-08-20 review
   (their numbers are recorded below); `snapshot-compare` was folded into the
   seed's dry-run on 2026-08-21 — it carried the same decode and the same
   verdict rule behind its own counting shell.

### The measured truth (checkpoint 64,010,495; RPC-verified 260/260 sampled)

| finding                                                         | count              | note                                                                                                                                                                                                                                            |
| --------------------------------------------------------------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| live trustlines the network has, we hold NO row                 | **19,290,231**     | the real gap is **60%**, not the ~7% the 200-account probe suggested — that probe sampled accounts WE hold; dormant-since-floor lines were invisible to it. 99.997% have `lastModifiedLedgerSeq` below our floor: dormancy, not a parser defect |
| our zero rows that are REAL closures                            | **24,474,026**     | the ghost set the read flip must not resurrect                                                                                                                                                                                                  |
| native ghosts (account deleted, we still show a native balance) | **1,035,265** rows | task 0321's class, 17× its own measurement — its method only saw merges we ingested; RPC check: 100/100 sampled accounts absent on chain                                                                                                        |
| assets we have never seen at all                                | **97,139**         | network has 391,092 live assets, we know 344,989, intersection ~294k (we also hold ~51k dead ones)                                                                                                                                              |
| signers rows ready to seed                                      | **10,865,408**     | every live account, versioned on each entry's own ledger                                                                                                                                                                                        |

Asymmetry explained: the RPC bootstrap (task 0492) seeded accounts + native
balances, never trustlines — so native measures complete while classic misses
19.3M.

| finding                                           | count                                              | note                                                                                                                                                                                          |
| ------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pool-share positions (NOT seeded, rides ADR 0056) | network **77,048** live vs our **40,738** positive | our `lp_positions` holds 108,579 pairs, 67,841 at zero — the SAME live-zero vs closed ambiguity as `balances`, measured 2026-08-18; the LP merge inherits both the gap and the disambiguation |

### Decisions taken (recorded in the map ticket + seed module docs)

- **Closure ledger = the run's checkpoint ledger** (both `closed_at_ledger`
  and the RMT version). Semantics for seeded rows: "closed at or before".
  Free cohort provenance until 0492 lands a real convention.
- **Ghosts are corrected AND reported** (option A applies): RPC proved them
  real removals, so `amount = 0` together with `closed_at_ledger`; the full
  list always lands in `ghosts.tsv`.
- **Pool shares NOT seeded** — `manifest.json` re-derives the identical
  snapshot for the ADR 0056 LP merge (archive is content-addressed).
- **No indexer stop needed for the seed**: every row versions on a real ledger
  (entry's own, or the checkpoint), so RMT ordering makes load order
  irrelevant — the live writer's newer rows win regardless.

### The load-bearing order (do not reorder)

1. **Deploy the lifecycle writer** (indexer from this branch).
2. Fresh `our_balances` + id exports (minutes before the seed; the skew
   window measurably grows divergents 1.5k→25k).
3. **`snapshot-seed --execute`** against a checkpoint taken AFTER the deploy.
   Reversed, every removal in the gap outversions its seed closure and
   resurrects the ghost.
4. Coverage measurement + independent cross-check (standing requirement), the
   200-account probe as a REPEATABLE check.
5. Only then: flip the read filter, ship signers API/DTO/UI (explicit
   "not indexed" state until then), regenerate API types, update docs.

### Findings 2026-08-18 (second sweep — the "omit nothing" pass)

- **The RPC bootstrap is NOT a one-off**: `bootstrap.rs` runs once per backfill
  window inside `run` (fills skeleton accounts via per-key RPC, task 0214).
  The seed covers its purpose strictly better, but retiring it is a live
  backfill-flow change — gated on the seed verifying on prod; recorded in 0502.
- **`assets` stubs need no RMT version** (question raised in review of the
  plan): the table has no version column, so CH keeps the last-inserted row
  per identity key — safe here because every `AssetRow` field including the
  `id` surrogate is a pure function of the identity tuple, so all rows for one
  key are byte-identical; stubs additionally only target absent ids. Argument
  recorded in the seed module docs.
- **Every snapshot entry type now has an explicit owner** — the full 10-type
  ledger (network counts, our side, compared-or-not, owner task) lives in
  0503; the window-discriminator verdict rule (before-floor = coverage gap,
  in-window = WE index wrong; post-seed it becomes a pure correctness
  monitor) is recorded in both 0502 and 0503.
- Snapshot bytes are never kept on disk (unnamed temp file per bucket); the
  durable artifacts are `manifest.json` (checkpoint + 21 bucket hashes —
  re-derives the exact snapshot), `summary.txt`, `ghosts.tsv`.

### Library audit 2026-08-18 — what upstream already does, and what it does not

Checked our hand-rolled pieces against the ecosystem (the standing
prefer-the-official-library rule) rather than assuming:

- **Dedup + ordering VALIDATED against the reference implementation.**
  `stellar/go`'s `ingest.NewCheckpointChangeReader` uses the same level order
  (0→10, `curr` before `snap`) and the same DEADENTRY-as-tombstone rule. On
  INITENTRY we are deliberately MORE conservative: Go skips recording those
  keys, leaning on the CAP-0020 invariant as a memory optimisation. Recorded in
  the module docs as "do not fix this to match Go".
- **No Rust equivalent exists** for bucket decoding or state reconstruction.
  SDF's `stellar-archivist` (rs-stellar-archivist, first release 2025-07) does
  cover manifest parsing, bucket-URL layout and checkpoint math — but it never
  decodes a bucket (`grep -c BucketEntry src/` = 0). Adoption would pull its
  default feature set (five cloud storage backends) for three small functions
  that are byte-for-byte equivalent to its `history_format` module. Left as-is,
  documented, revisit in 0502.
- **`next` correctly ignored** — Go's `BucketList.Hash()` folds only `curr` and
  `snap`, so `next` (an in-flight merge) is by definition not committed state.
  Caveat recorded: today's manifest shows `state: 0` everywhere, but historical
  checkpoints do not — do not infer the field is always idle.
- **Shadow buckets are a non-issue** — CAP-0025 removed them in protocol 12 and
  they never appeared in a committed `curr`/`snap`.
- **`hotArchiveBuckets` (CAP-0062, protocol 23) resolved T8 in our favour** —
  eviction DELETES the entry from the live bucket list, so `currentBuckets` is
  the authority on liveness and classifying an evicted holding as gone is
  correct. The archived-but-restorable distinction is a display question, not a
  correctness one. Future constraint: post-23 the header bucket-list hash is
  `SHA256(live, hotArchive)` — hash verification must fold both.
- Hardening applied: METAENTRY is now required at record 1 (Go errors on this
  too) instead of being silently skipped anywhere.

### Decision 2026-08-18 — ONE seed run, and the direction is the reusable tool

A five-agent audit recommended splitting the seed in two (closures only first,
the 19.3M backward fill later) on the grounds that no acceptance criterion
needs the fill and it carries all the production risk.

**Rejected.** The point of this work is not to close issue #377 at minimum
cost; it is to make the index complete and to grow the snapshot reader into
the reusable capability task 0502 describes. Splitting optimises for shipping
the issue and moves AWAY from that. One run, done properly.

What that decision obliges, in place of the split:

- The aggregates WILL move (`balance_aggregates_mv` recomputes every two
  minutes). Capture `total_supply` / `holder_count` before the run and verify
  the delta against RPC — the acceptance criterion is rewritten accordingly.
- Up to ~97k previously-unknown assets enter the public assets list and search
  (`assets` list query filters nothing). That is correct — they are real
  network assets — but it is a visible product change, not a silent one.
- The seed's own inputs must be trustworthy: buckets are SHA-256 verified
  against the manifest hashes, the checkpoint is pinnable so `--execute` runs
  the snapshot the dry-run reviewed, and truncated input files are refused.

### Audit findings 2026-08-18 — five agents over the full branch diff

One defect blocked deploy and is fixed: **`account_entry_state` rows were not
deduplicated per account.** States arrive one per transaction and every state
in a ledger carries that ledger as its RMT version, so two `SetOptions` on one
account in one ledger wrote two rows at the SAME version — the merge picks
arbitrarily, and a REMOVED signer could win. The balances path beside it
already guarded exactly this. Fixed with a per-account fold plus a regression
test; a keyless signer now warns instead of silently shrinking the set.

Surviving objections worth carrying (each now has a fix in this branch or a
named owner): the missing-bucket verdicts are the least verifiable ones (the
sample files carry surrogates, and reversing them runs through our own
incomplete tables); 260 RPC samples cannot bound the error on 44.9M
corrections; `--execute` re-fetched the manifest, so it would not have run the
snapshot the dry-run reviewed; rows the writer had already closed were counted
as missing because the early return skipped the matched flag.

### Export freshness is the biggest lever on correction volume — measured

Re-running the comparison with a stale `--our-rows` export against a much later
checkpoint moved the amount-disagreement bucket from **25,346 to 809,418** for
classic trustlines (native: 16,813 divergent + 41,815 heal). The population is
the same; only the gap between export and checkpoint grew. Earlier readings on
narrower gaps gave ~1.5k.

That is not corruption — it is ordinary network churn the comparison correctly
attributes to whichever side is newer — but it means **the seed's correction
count is dominated by export staleness, not by real defects.** Export minutes
before the run, not hours. The unified verdict makes this legible: `heal`
(snapshot strictly newer, seed adopts) is now reported apart from
`divergent ours-newer` (kept, live parser knows better), where the old single
"divergent" number hid the direction entirely.

### Pre-existing defect found while auditing same-ledger ties — and the seed repairs it

Audited EVERY ledger-versioned ReplacingMergeTree table for the hazard that
`account_entry_state` had (two states in one ledger → two rows at the same RMT
version → the merge picks arbitrarily). Measured on production:

| table                   | keys with >1 content at the same version |
| ----------------------- | ---------------------------------------- |
| `accounts`              | 0                                        |
| `soroban_contracts`     | 0                                        |
| `liquidity_pools`       | 0                                        |
| `lp_positions`          | 0                                        |
| `nfts` / `nfts_pending` | 0                                        |
| **`balances`**          | **1,238,583**                            |

Every other writer already folds per key before emitting. `balances` has
`upsert_balance`, but it only dedups WITHIN one `prepare()` call — it cannot see
rows written by a different pass.

The shape is specific: **100% native XLM, and 100% of them are `0` versus a real
amount** at the same ledger. By ledger band: 54M→658, 56M→3,338, 58M→6,970,
60M→6,913, 62M→1,460 per 1/64 slice, and **nothing above 63.04M** — the
generating process stopped ~2 months ago, so the live indexer is not the source.
Consistent with the same ledger being parsed twice by different code (a
`--reindex` pass): first run wrote 0, second wrote the real amount, both at the
same version.

**Why it matters here:** our export uses `argMax(amount, last_updated_ledger)`,
and `argMax` on a tie is also arbitrary — so those 1.24M keys enter the
comparison as a coin flip, landing in `closure` or `ghost` depending on which
row the merge happened to keep.

**Root cause PROVEN (2026-08-19).** Commit `80552009`, deployed 2026-06-23
("fix(lore-0295): zero native balance on account merge (removed tombstone)"),
is the dividing line — and ledger 63.04M IS 2026-06-23. Before it, a merge
left the account's last balance X in `balances`; after it, a merge writes a
native 0 tombstone at the merge ledger. A re-parse of 54M–63.04M with the
post-fix code then wrote the 0 rows at the SAME versions where the pre-fix
live writer had written X. Evidence: all 5 sampled tie accounts have
`last_seen == tie ledger` (the merge was their last act), amounts are
pre-merge dust (1.5 XLM, 5 XLM, 1 stroop), 5/5 RPC-verified ABSENT from
chain, 3/5 sit in `ghosts.tsv` and 2/5 in the closure bucket — the argMax
coin flip observed directly. Not the RPC bootstrap (0 of 19,339 tie ledgers
on a 64k boundary) and not the legacy-table migration
(`account_balances_current` holds NO rows for these accounts).

**Therefore the seed repairs ALL harmful ties, not just most.** A tombstone-0
is only ever written at a merge, so every tie key is a merged account — gone
from the snapshot — and gets a `Closure`/`Ghost` correction at the checkpoint
version, which outversions BOTH tie rows deterministically. Keys with
activity after the tie ledger were never harmed (argMax picks the newer row).
No separate repair task needed; the standing tie-audit query moves to 0503 as
a recurring check, because the mechanism (semantic writer change + re-parse
of old windows at equal versions) can recur.

**Burn-account verification (2026-08-19), closing the supply question:**
`GALAXYVOID…LUTO` holds **55,442,115,247 XLM** in our data — our 105.41B
total minus the burn account = 49.97B, exactly the public circulating figure.
Our sum is correct; burned XLM sit on a signerless account and never left
`total_coins`.

### Decision 2026-08-19 — reconciliation-after-re-parse is the fundamental fix

Question raised: how do we make BOTH tie sources impossible? Answer recorded:

- **Within one ledger** — already impossible today: full-schema audit in 0503
  (state writers fold last-wins in application order; fact tables carry order
  columns or aggregate per tx; presence tables collapse by design). Gap: some
  folds lack regression tests.
- **Between runs** — impossible to prevent in-schema without a second version
  dimension. The first argument recorded here (an insert-time tiebreaker
  rejected because "a regressed re-parse would clobber good data") was
  CHALLENGED and the challenge stands: a backfill is run precisely because the
  new writer is declared better, so refusing to let it win defeats the
  backfill's purpose — the June re-parse's own tombstones losing half their
  coin flips is the proof. Determinism is the goal. The revised position: the
  MECHANISM for determinism should be the network's value, not insert time —
  reconciliation (tie query + compare + seed) supersedes both sides with the
  chain's own truth at the checkpoint version, needs no fleet-wide engine
  migration, and catches a regressed writer instead of trusting whoever ran
  last. It is now a MANDATORY post-re-parse step in `docs/backfills.md`.
  Dead-entity ties are auto-repaired outright. Live-entity same-ledger
  divergences are currently quarantined as `DivergentSameLedger`; a
  `--heal-same-ledger` mode (adopt the NETWORK amount at the checkpoint
  version, full list to an artifact) is the agreed direction if the quarantine
  bucket proves noisy — it delivers the "new data wins" outcome from a source
  better than either run.

### Steps 1-2 of the hardening list — done 2026-08-19

- Fold regression tests added for the four state writers that lacked them
  (accounts, lp_positions, liquidity_pools, nfts): two states in one ledger
  collapse to one row carrying the LAST state; first_deposit/minted survive
  via min-preservation.
- **The bucket list is now verified against the ledger header** — the value
  validators signed — using stellar-core's fold (per-level SHA256(curr||snap),
  zeros included, SHA256 over level hashes, and the CAP-0062 post-protocol-23
  composition SHA256(live||hotArchive)). Confirmed live on the first run.
  Wired into all four snapshot commands; `manifest.json` now carries raw
  levels so pinned re-runs re-verify instead of skipping (a level-less pin
  skips LOUDLY). The chain of trust no longer ends at TLS.

### Review 2026-08-20 — over-engineering cuts before merge

A fresh-eyes senior audit over the full branch (requested precisely to
counter this branch's own bias) classified ~900 lines (~25% of the new Rust)
as research scaffolding or false-precision instrumentation. Decisions taken
and EXECUTED:

- **`snapshot-tally` and `snapshot-dedup` removed** (6 subcommands → 4).
  Research probes whose numbers are recorded above; `snapshot-compare` prints
  the same distinct-entry report before folding our rows in.
- **The statistics apparatus removed**: `--sample-cap`, the population
  estimates, derived strides, and the "statistical power" printout. The
  rule-of-three arithmetic assumed independent per-row errors, but decoder
  defects are SYSTEMATIC — a wrong derivation hits every row of a class, so
  1,000 fixed samples per bucket detect anything 27,000 would. Kept: the
  deterministic key-hash sampling for `missing` (HashMap order is per-process),
  `Agree` as positive control, PROVEN-AT-CHECKPOINT vs CHANGED-SINCE labels.
- **The ledger-header bucket-list-hash verification removed** (with its
  `levels`/`hot_levels` plumbing; manifest.json back to
  checkpoint+archive+buckets). Honest threat model: it defended against a
  forged or stale MANIFEST from the network's own reference publisher — not a
  realistic failure for a one-off seed whose output is independently
  RPC-spot-checked anyway. Per-bucket SHA-256 (covers truncation/corruption,
  the realistic class) stays. Was live-verified once before removal;
  resurrect from git history if a third-party mirror is ever added.
  ADR 0057 decision 4 amended accordingly.
- **`--execute` now HARD-ERRORS without `--pinned-manifest`** (was a
  warning) — closes the audit finding that execute could decode a different
  snapshot than the dry-run reviewed. **SUPERSEDED 2026-08-21 → deleted
  2026-08-24**: the pin's job was keeping a FROZEN TSV export consistent with
  the snapshot; with our side read live every run is self-consistent at its
  own checkpoint, and an unpinned run decodes the fresher one. Demoted to
  optional that day, then removed entirely — `manifest.json` is still written,
  so the ADR 0056 LP merge can identify which checkpoint this seed used.
- **RPC verification (`snapshot-verify`) DELETED 2026-08-21** — recorded in 0502. The snapshot outranks RPC as a source (content-hash verified +
  enumerable, vs per-key JSON on trust), so a permanent RPC comparator earns
  nothing. It had done its one job already: 260/260 samples and the 100/100
  ghost check that flipped ghosts from "report only" to "zero and report".
  `rpc_snapshot.rs` stays — `bootstrap` and `balance-seed` use it. This
  supersedes the standing "cross-check RPC regardless of result" AC; post-seed
  verification now rests on coverage measurement + the 200-account probe.
- Fold tests for the four sibling state writers STAY in this branch (owner's
  call — they answer the audited hazard even though 0503 is their home).
- Small trims EXECUTED same day: `Stale` verdict moved to the no-op arm
  (verdict rule guarantees equal amounts, the write-arm equality guard was
  dead), ONE dump file per bucket (verify key first, surrogates after;
  the dump is self-describing), one
  shared `MIN_OUR_ROWS` 40M floor, seed reuses `build_state` instead of its
  own decode loop.
- **Seed disk cost MEASURED, a non-issue**: `balances` today is 78.2M rows =
  1.02 GiB on disk (2.91 GiB uncompressed), ~14 B/row compressed. The seed's
  ~44.9M balance rows ≈ 0.6 GiB; 10.9M signer rows at accounts-like density
  (~77 B/row) ≈ 0.8 GiB; stubs negligible. Total ~1–2 GiB against 428 GiB
  free on the prod disk.

### Decision 2026-08-21 — the toolchain reads its own inputs (manual exports abolished)

A crate-convention analysis plus an independent design study settled how the
compare/seed obtain "our side". Finding: every other corrective command in
backfill-runner (`repair-tier1`, `contract-type-rebuild`, `balance-seed`,
`nft-reclassify`, `bootstrap`) self-serves its inputs from `sink.client()`
with `--dry-run` recomputing at the real run — the snapshot trio was the
crate's ONLY consumer of hand-fed files, and its stated justification (don't
hand mTLS creds to the binary) was void: `--execute` inserts through that
same connection anyway.

Options weighed: manual TSV (status quo), live self-read (sibling style),
self-export-as-artifact, CH staging tables (rejected: verdict logic
duplicated in SQL + dry-run would write to prod + same memory ceiling), and
terraform-style plan/apply. **Chosen: live self-read (sibling style)** —
owner's call, explicitly preferring the crate's dry-run-as-sanity-check
model over frozen-input machinery; the dry-run/execute drift is absorbed by
the `>= checkpoint` guard (churned rows → NewerThanCheckpoint → left alone),
so the drift is only rows newly skipped, never a different correction — and
the post-seed verification (coverage, 200-account probe, aggregate deltas)
measures the OUTCOME against the network anyway, which is the real net.

**Follow-up same day: the `--execute` pin demoted to optional.** The hard
requirement was flagged as the same class of frozen-input machinery — and the
challenge held: the pin's original job was keeping the FROZEN TSV export and
the snapshot mutually consistent (the measured 1.5k→809k skew blowup); with
our side read live, every run is self-consistent at its own checkpoint, and
an unpinned execute uses the FRESHER snapshot — strictly better input. The
completeness worry ("can the newest checkpoint be half-published?") was
verified three ways: stellar-core `docs/history.md` states the `.well-known`
manifest is written LAST as an atomic commit point (failed publications are
discarded, not half-visible); a live probe of a minutes-old manifest found
all 22 referenced files present; and the runner fails loud regardless (404
via `error_for_status`, truncation via per-bucket SHA-256) — worst case is a
failed run, never a half-decoded one. `--pinned-manifest` stays as an
optional flag with one real consumer: the ADR 0056 LP merge re-derives this
seed's exact snapshot from `artifacts/manifest.json`.

Executed: `--our-rows`/`--assets-ids`/`--accounts-ids` deleted; seed and
compare stream the 64-slice `argMax` read through one shared
`stream_our_rows` (short-read floor inside); id sets fetched by query;
`snapshot-export-sql` and the TSV transport deleted (with `OurRow::parse` and
its test); the chq exit-0 trap and the one-dropped-slice hole (found by the
design-study agent: 40M floor passes a 64th-slice loss) are gone — a cursor
error propagates. Freshness — the measured dominant lever on correction
volume — moves from runbook discipline into the tool itself. Net −~200
lines; operator flow: dry-run → read summary → `--execute --pinned-manifest`.

### CI red 2026-08-20 — not this branch: Rust 1.98.0 vs zig's linker

The PR's `Rust (lambda build)` check went red on a **docs-only commit**
(`a8a98bbd`, three markdown files) while clippy, fmt, tests and API-types all
stayed green. Cause is external toolchain drift, proven from the run log:

```
stable-aarch64-unknown-linux-gnu updated - rustc 1.98.0 (2026-08-18)
                                           (from rustc 1.97.1 (2026-07-14))
error: unsupported linker arg: --fix-cortex-a53-843419
error: could not compile `crc-fast` (lib)
```

Rust 1.98.0 shipped 2026-08-20 and passes `--fix-cortex-a53-843419` to the
linker for `aarch64-unknown-linux-gnu`. `cargo lambda build --arm64` links
through zig, which rejects the argument. `cargo-zigbuild` filters it from
0.23.0 (PR #452, 2026-06-10), but cargo-lambda 1.9.1 — the newest release —
hard-depends on `cargo-zigbuild = "0.20.1"`, so upgrading cargo-lambda does
not help and installing a newer standalone zigbuild does nothing (it is a
vendored library, not a binary on PATH).

Fix applied: pin the toolchain to 1.97.1 in the two jobs that build lambdas —
`ci.yml`'s `rust-lambda` and `deploy-production.yml` (which has the identical
setup, so **the next production deploy would have failed the same way**). The
other Rust jobs stay on `@stable`; they build natively, never through zig, and
are green on 1.98.0. Unpin both together when cargo-lambda ships a release
depending on cargo-zigbuild >= 0.23.0.

Worth noting the vector: `dtolnay/rust-toolchain@stable` plus an unpinned
`pip3 install cargo-lambda` means CI's Rust build is not reproducible — the
same commit builds differently on different days. This fix pins one half of
that; pinning cargo-lambda itself is the obvious follow-up. The fix belongs on
develop rather than this feature branch, since it blocks every Rust PR in the
repo.

### Decision 2026-08-21 — `snapshot-compare` folded into the seed's dry-run

The owner's requirement, clarified: the completeness comparison is wanted
**on demand only** — automatic runs are of no interest. That removed the one
argument for a standalone read-only command (a binary with no write path,
safe to schedule). What remained was the crate's own convention: one command
with a dry-run, not two commands where one is the other's dry-run.

The merge was gated on losing no analysis. A fresh-eyes agent enumerated
every output of `compare_command` and traced where each is computed: the
verdict rule, first-wins dedup and `report_state` already live in
`snapshot.rs` and the seed already called them; compare-only code was ~340
lines of counting and sampling around that engine, all computable from data
the seed's dry-run holds at the same loop position.

**Zero analytic outputs lost. The seed's dry-run GAINED four** it did not
print before: the `report_state` distinct-entry table, the `agree` /
`stale` / `already_closed` / `divergent_ours_newer` buckets, the
classic-vs-native split on every counter, and the excluded-population
counts. `summary.txt` is now the full report; the sample dumps land in
`<artifacts>/dumps/`.

Done in four reviewable steps, each compiling and testing on its own so a
bisect cannot land on a broken commit:

1. `snapshot::open_snapshot` — one opened pass, both consumers.
2. `snapshot_report.rs` — the analysis gets its own module.
3. `Report::observe` RETURNS the verdict it counted, so the seed classifies
   each row exactly once and builds its correction from that same verdict.
   The report and the write can no longer describe different populations.
4. `snapshot-compare` retired to `.trash/`; `stream_our_rows` moved to its
   only remaining consumer.

Cost accepted: the report run is always the heavy one (~4.5 GB RSS, rows
materialised). A `--report-only` flag was designed and dropped as YAGNI —
it would save ~2 GB on a box that has it, at the price of a third mode.

Net: 2 snapshot subcommands -> 1, and the mandatory post-re-parse
reconciliation drops from three snapshot decodes to two.

### Open follow-ups spawned along the way

- 0502 (reusable snapshot decoder — extract `snapshot.rs` from
  backfill-runner into its own crate; the seed stays a backfill-runner
  consumer), 0503 (exhaustive audit), 0504 (five discarded entry types),
  0497 (retire repair-tier1), 0498/0499 (LP merge), 0492 (provenance).
- Lore id collisions present ON DEVELOP (pre-existing, not from this branch):
  two `0496_*` tasks, two `0054_*` ADRs.

### Dry-run on production 2026-08-24 — checkpoint 64,106,239, verified against chain

Method, and the two dead ends it ran into, in
[`notes/V-chain-audit-method.md`](notes/V-chain-audit-method.md).

First full dry-run of `snapshot-seed` against production. 909 s, peak RSS
4.37 GB, exit 0. Every number below is measured, and every chain check uses an
INDEPENDENT implementation (SEP-23 StrKey + LedgerKey XDR + AccountEntry /
TrustLineEntry decoders written from the spec in Python), so a shared
misreading in our Rust cannot pass it.

| what the seed would write                    | rows                                           |
| -------------------------------------------- | ---------------------------------------------- |
| `balances` corrections                       | 44,846,161                                     |
| `account_entry_state` (signers + thresholds) | 10,891,935                                     |
| `assets` stubs                               | 97,109                                         |
| `accounts` stubs                             | **0** — every referenced holder already exists |

**Unresolved references: 0 assets, 0 holders** (464 issuers, cosmetic). A new
counter, added for this run, turns three silent `continue`s in the stub pass
into a reported number and makes `--execute` refuse rather than write a balance
whose JOIN finds nothing.

#### The highest-stakes population is clean

The class that cannot be caught by inspection is an INVENTED entity: a wrong
amount on a real asset is visible to anyone who looks the asset up, an asset
that does not exist on chain is not, because nobody knows to look for it.

Structural audit over **every** row of the stub dump (no sampling):

- 97,109 stubs, 97,109 distinct ids, 97,109 distinct identities — a bijection,
  so no surrogate collision merged two assets into one row.
- Every asset code is alphanumeric (width histogram peaks at 3-4 and 10-12,
  the shape real Stellar codes have) or the documented `0x` hex fallback — 3 of
  97,109 take that path.
- Every issuer StrKey passes an independently computed CRC16.

Chain audit of the rows the seed INSERTS as new live holdings — 1,000 sampled
from `missing_classic`:

|                                                |                 |
| ---------------------------------------------- | --------------- |
| present on chain                               | **1000 / 1000** |
| identity echoed back unchanged (code + issuer) | **1000 / 1000** |
| unchanged since our recorded ledger            | **1000 / 1000** |
| balance matches to the stroop                  | **1000 / 1000** |

Positive control (`agree_classic`) 200/200 present and frozen; `closure_classic`
200/200 absent, so flipping the read filter resurrects nothing; native ghosts
300/300 absent from chain (holder StrKeys resolved through `accounts`).

17.8% of the missing rows carry amount 0 — live zero-balance trustlines, the
exact class issue #377 reports.

#### Defect found and fixed: the checkpoint guard sat in the wrong branch

`NewerThanCheckpoint` was only reachable when the network held NO entry for the
key. A row our writer touched after the snapshot was taken, but which the
snapshot still lists as live, fell through to a comparison that cannot mean
anything.

Measured consequence: **1000 of 1000 sampled rows in BOTH `divergent
ours-newer` buckets (11,994 classic + 3,772 native) were simply newer than the
checkpoint.** The bucket an operator reads as "our parser and the network
disagree" held no disagreement at all.

The serious half is latent rather than visible: once the lifecycle writer is
deployed — step 1 of the deployment order, before this seed runs by design — a
trustline our writer CLOSES between the checkpoint and the run meets a snapshot
that still calls it live, and is reported as `ClosedButLiveConflict`. That is
one of the two defect signals, whose whole value is never firing on healthy
data. It reads 0 today only because the writer is not deployed yet.

Fixed by hoisting the `>= checkpoint` guard above both the closure branch and
the live-entry match, with a test pinning all six combinations. **No correction
changes**: every verdict the hoist can absorb was already report-only
(`ClosedButLive` and `HealFromSnapshot` both require a snapshot ledger above the
row's, which a post-checkpoint row makes impossible), so the measured correction
counts stand.

#### The same-ledger quarantine proved noisy — and the direction is one-way

`DivergentSameLedger` (17,840 native) is the bucket held back pending evidence.
ADR 0057 recorded `--heal-same-ledger` as the agreed direction "if the
quarantine bucket proves noisy". Measured against chain, 1,000 sampled:

- 997 comparable (3 churned, 0 gone, 0 equal)
- **997 of 997: our amount is LOWER than the chain's. Zero exceptions.**
- Differences cluster hard — 9,614 stroops on 415 accounts, 9,624 on 102,
  77,268 on 73 — across only 249 distinct values.

One-directional plus clustered is systematic, not random corruption: we are
capturing an earlier point in a ledger the account changed more than once. The
seed writes nothing for these, so it neither causes nor worsens them, but it
also does not fix them — they stay wrong after the run.

#### Classic ghosts belong to holders we hold no identity for

Every other verdict bucket resolves 100% of its holders through `accounts`.
`ghosts_classic` resolves **0 of 440**. Measured on one 1/64 key slice: 9 orphan
holders in 111,879, so roughly **576 network-wide (0.008%)** — rows in
`balances` whose holder has no `accounts` row and is not a contract either.

Consequence for this run: the 1,941 classic ghosts cannot be chain-verified by
us at all, because no source carries their StrKey — not `accounts`, and not the
snapshot, which holds no `AccountEntry` for them live or dead. Their risk is
bounded by the same fact: with no StrKey the account page cannot render them
today, so zeroing changes nothing user-visible. Root cause is a separate
question, and belongs with the orphan-holder population rather than with the
seed.

#### The fixture account, end to end

`GDXWIA4V…` — five rows in `balances`, two rendered today:

| asset | ours           | chain   | `lastModifiedLedgerSeq` |
| ----- | -------------- | ------- | ----------------------- |
| AQUA  | 0 @ 58,469,457 | PRESENT | 58,469,457              |
| USDC  | 0 @ 58,469,453 | PRESENT | 58,469,453              |
| SHX   | 0 @ 59,023,860 | PRESENT | 59,023,860              |

Three live trustlines at zero, agreeing to the ledger.

#### Correction to this task's own description: four signers, not five

The chain's `AccountEntry` for the fixture carries **four** signers at weight 1
and `masterWeight = 1`, thresholds 3/3/3. Total signing power 5, threshold 3 —
so "a genuine 3-of-5" is right, but the fifth key is the account's OWN master
key, which the ledger does not put in the signers list. Horizon synthesises it
in; that is where "five ed25519 signers" came from.

This is a UI constraint, not a wording nit. `signer_keys` holds four entries and
`master_weight` is a separate column, which is correct per XDR — but a page that
renders only the list shows **3-of-4** and reads as a real threshold rather than
as missing data. `stage.rs` already carries that exact warning for the keyless
case; the master key is the same failure through a different door.

#### Thresholds above total weight are normal, not corrupt

An audit assertion that every account must be able to sign flagged 50 of 5,000
sampled states. Three verified against chain, byte-identical to ours
(`1/1/255/255` with no signers, `0/1/1/1` with no signers, `1/10/10/10` with one
weight-1 signer). These are the standard idioms for locking an account forever —
a fixed-supply issuer proving it cannot issue more. The assertion was wrong;
counted as an observation now.

#### Native supply: resolved against the ledger header, not against a remembered figure

Native `total_supply` moves DOWN by the ghosts' value, 45,345,267.65 XLM, and
`holder_count` by 1,045,836. This task previously judged that number against a
recollection ("our total minus the burn account matched the public circulating
figure exactly"). Measured properly instead, against the value the validators
sign — `total_coins` in the ledger header, read from RPC `getLedgers`:

```
total_coins                     105,443,902,087.35 XLM
  minus the burn account         50,001,786,840.35 XLM   <- the published ~50B supply
```

The decomposition settles it:

|                                                                                | XLM                |
| ------------------------------------------------------------------------------ | ------------------ |
| our native sum, pre-seed                                                       | 105,411,180,657.36 |
| our native sum, post-seed                                                      | 105,365,835,389.71 |
| gap to `total_coins`, pre-seed                                                 | 32,721,429.99      |
| **gap to `total_coins`, post-seed**                                            | **78,066,697.64**  |
| of which XLM in AMM pools (measured, `liquidity_pool_snapshots`, 52,619 pools) | 22,231,810.44      |
| remainder — claimable balances + contract-held                                 | 55,834,887.20      |

The gap is REQUIRED, not a defect: XLM sitting in AMM pool reserves and in
claimable balances belongs to no `AccountEntry`, so the sum of account balances
must fall below `total_coins` by exactly that much. The pre-seed gap of 32.7M is
in fact too SMALL — it leaves only 10.5M for every claimable balance and
contract-held XLM on the network, after 22.2M of AMM reserves are accounted for.
The post-seed gap leaves 55.8M, which is the plausible figure for a network with
years of airdrop claimable balances outstanding.

So the seed moves native supply TOWARD the chain, and the earlier "exact match"
was two errors cancelling: phantom XLM on merged accounts filling in for XLM
correctly held outside accounts. Acceptance criterion satisfied — direction
justified, magnitude decomposed, anchor independent.

Left genuinely open: the remainder is inferred, not enumerated. The snapshot
decoder already reads `ClaimableBalanceEntry` and `LiquidityPoolEntry` and
discards them (77.9M unmodelled records); tallying the XLM in those two entry
types would turn 55.8M from a plausible residual into a measured one. That
belongs with 0503/0504, which own the discarded entry types.

#### Confirming run — checkpoint 64,106,495, with the verdict fix

Re-run four minutes later on a fresh checkpoint. The fix moved exactly what it
should and nothing else:

| bucket                                           | before    | after              |
| ------------------------------------------------ | --------- | ------------------ |
| `divergent ours-newer`, classic                  | 11,994    | **0**              |
| `divergent ours-newer`, native                   | 3,772     | **0**              |
| `newer than checkpoint`                          | 324 + 151 | 9,062 + 10,744     |
| `divergent SAME ledger` (the real defect signal) | 17,840    | 17,739 — untouched |

The bucket an operator reads as disagreement is now empty, because it never held
disagreement; the genuine signal is unaffected, as it must be — those rows sit
at a ledger below the checkpoint, where comparison is meaningful.

Reproduced: 0 structural failures, 0 unresolved asset/holder references, 97,109
asset stubs (identical count — the decode is deterministic), and a SECOND
independent 1,000-row chain sample of `missing_classic` at 1000/1000 present,
identity-matched, frozen and amount-exact. 2,000 samples across two runs, no
defect.

Also worth recording as a clean bill of health for the live writer: **0 of the
19,278,325 missing trustlines have a `lastModifiedLedgerSeq` at or above our
floor.** Every one predates our indexing window, so the gap is coverage, not a
parser that drops trustlines — the discriminator rule from 0502/0503, returning
the good answer.

#### The audit's own coverage was uneven — closed 2026-08-24

Asked which populations got the same scrutiny, the honest answer was no. By
rows written versus chain probes run:

| set               | rows it writes | chain probes, before | after                                      |
| ----------------- | -------------- | -------------------- | ------------------------------------------ |
| `missing_classic` | 19.3M          | 2,000                | 2,000                                      |
| `closure_classic` | **22.2M**      | 200                  | **712** (every resolvable row of the dump) |
| `entry_states`    | **10.9M**      | **3**                | **5,000**                                  |
| `ghosts_native`   | 1.04M          | 300                  | 300                                        |
| `ghosts_classic`  | 1,941          | 0                    | 0 — unreachable, no StrKey exists anywhere |

Signers were the real gap: 10.9M rows written on the strength of three
samples. Closed by fetching all 5,000 dumped accounts from chain and comparing
the decoded `AccountEntry` field by field —

**5,000 of 5,000 identical: thresholds, signer set, AND signer order.** No
account absent, none divergent.

`closure_classic` re-probed at 712 of 712 gone from chain.

#### The same-ledger defect is LIVE, and it is in the Soroban path

The heal decision made the root cause urgent rather than academic, so the
quarantine bucket was characterised rather than just counted.

**It is not historical.** Ledgers run to 64,106,462 against a checkpoint of
64,106,495 — the newest possible. By band across the 1,000-row sample: 8 at
58-59M, 131 at 59-60M, 25, 434, 30, 265, and **107 in the current 64M band**,
spread over **96 distinct ledgers**, so it is a continuous process and not one
bad event.

**It is one-directional in the current band too**: 101 of 101 comparable rows
have OUR value lower, none higher, differences of roughly 0.001-0.007 XLM.

**It localises to Soroban.** For the 107 (account, ledger) pairs in the newest
band, our `transactions` table holds:

|                                                       |         |
| ----------------------------------------------------- | ------- |
| Soroban transactions from that account in that ledger | **106** |
| classic-only transactions                             | **0**   |

106 of 107 pairs, zero classic. That is a subsystem, not a coincidence.

**One hypothesis raised and REFUTED, recorded so it is not re-tried:** that
`extract_ledger_entry_changes` misses a fourth meta container carrying the
unused-resource-fee refund. `TransactionMetaV4` in stellar-xdr 26.0.1 carries
only `tx_changes_before`, `operations` and `tx_changes_after`, and the parser
reads all three. The refund does surface as a `TransactionEvent` with
`stage = AfterAllTxs` (our own `tx_event_stage_real_meta.rs` pins that against
mainnet), and `AfterAllTxs` appears nowhere in `db-clickhouse/src/persist/` —
but the balance path never reasons about stages at all, so that is a lead, not
a cause.

**What this means for the heal**: `--heal-same-ledger` repairs the ~17.7k
accumulated rows, and the writer keeps producing new ones. It is a correction,
not a fix, and the run does not close the underlying defect.

#### The third production run FAILED — a load-dependent ceiling, found the hard way

The `--heal-same-ledger` run died at a step the two previous runs had passed:

```
Code: 159. DB::Exception: Timeout exceeded: elapsed 31857 ms, maximum: 30000 ms
```

Not a balance slice — it never reached one. The unsliced dimension read,
`SELECT id FROM accounts GROUP BY id`, has to ship **14.58M ids** (15.82M raw
rows collapsed), and `max_execution_time` counts the time spent SENDING rows,
not only aggregating them. The aggregation alone measures **0.4s**; the
transfer is the whole cost.

That is the worst shape a limit can have: it depends on how busy the server is,
so it passes until it does not, and it would have passed again on a retry.

Two things went right, and are worth keeping rather than assuming:

- **It failed loudly and early**, before a single row was classified. A cursor
  error propagates rather than yielding a short set — which matters more here
  than almost anywhere else in the run, because fewer known ids means more ids
  judged absent, which means more dimension stubs. A silently truncated id read
  would have manufactured `assets` rows for assets that already exist: the
  invented-entity failure, arriving through the back door.
- The `MIN_ASSET_IDS` / `MIN_ACCOUNT_IDS` floors would have caught a short read
  too. Two independent guards on the same hazard, and the cheaper one fired.

Fixed by slicing the id read on `id`, exactly as `stream_our_rows` already
slices the balances read — the tool now has one policy for reading production
rather than one policy and one exception.

#### `--heal-same-ledger` built and dry-run verified (checkpoint 64,107,135)

The mode ADR 0057 named as the direction if the quarantine proved noisy. It
adopts the NETWORK's amount at the CHECKPOINT version — not at the tied ledger,
which would merely add a third candidate at the same RMT version and leave the
survivor a coin flip. The checkpoint is always strictly above the tie, because a
post-checkpoint row returns from the guard before ever reaching this verdict.

**17,798 rows healed**, full list with BOTH values in `same_ledger_healed.tsv`.

The one-directional finding now covers the WHOLE population, not a sample:

|                                    | rows       |
| ---------------------------------- | ---------- |
| network higher (our value too low) | **17,798** |
| network lower                      | **0**      |
| equal                              | **0**      |

And the healed value is the chain's: 200 sampled healed rows fetched from RPC,
**200 of 200 equal to the network balance** at the recorded ledger, none
churned, none gone.

Correction totals move as expected — 44,867,274 balance rows against 44,847,266
without the flag; the ~20k delta is the 17,798 heals plus ordinary churn between
two checkpoints taken ~11 minutes apart. Unresolved references still 0 and 0.

This is a correction of accumulated state, NOT a fix of the writer. At roughly
1,900 new divergences a week (extrapolated from the band distribution), the
benefit decays until the Soroban path is repaired, and the heal has to be
re-run.

**Decision 2026-08-24 (owner): the flag is REMOVED from the seed.** Healing the
symptom from the seed while the writer keeps producing new ties couples an
ongoing repair to a one-off tool. The design and its verification survive in
task 0514 (the Soroban writer bug), which owns root cause, writer fix, and the
one-shot heal AFTER the fix — in that order. The full-population measurement
(17,798 / 0 / 0) and the 200/200 RPC check of healed values are recorded there
and in the verdict's docs.

#### `--execute` needs an identity this dry-run does not have

Established by reading the server's own settings, never by attempting a write:
the laptop mTLS cert maps to ClickHouse user **`dev_read`**, whose profile sets
`readonly = 1` (explicitly changed) and `max_execution_time = 30`. An INSERT is
refused on the readonly setting before grants are consulted at all, and a
44.8M-row insert would exceed the execution ceiling regardless.

The dry-run is unaffected — it only reads, in 64 bounded slices. But the
deployment order has an unwritten step between "review summary.txt" and
"`--execute`": the seed must run under a write-capable identity, which is the
same class of infrastructure action as the indexer deploy that precedes it.
Worth settling before the run rather than discovering at the prompt.

### Round of 2026-08-24 evening — owner decisions executed

- **`--heal-same-ledger` removed** (decision above). `verdict::correction` is
  back to three arguments; the quarantine stands; repair lives in 0514.
- **Task 0514 filed on develop** (`45ce3f10`): the same-ledger bucket is a live
  Soroban writer bug — full-population one-directionality, ~1,900 rows/week,
  106/107 Soroban-only, the refuted fourth-container hypothesis, and the heal
  design to resurrect after the writer fix.
- **0503 appended**: the ~576 orphan holders (balances rows whose holder has no
  `accounts` row) as an enumerable parity defect; the 1,941 unverifiable
  classic ghosts are that same population.
- **Timeout fix kept as slicing** (owner decision): raising
  `max_execution_time` per query is refused under `readonly = 1` (verified by
  attempting `SETTINGS max_execution_time = 120` — `Code: 164`), so a profile
  change would be the only alternative, and the slicing works under any
  profile.
- **Write path exercised for the first time**: full `--execute` against a
  LOCAL ClickHouse (docker compose + `db-clickhouse-init` schema), using a
  scratch build with the three read floors zeroed (empty local tables would
  trip them by design; the binary is not committed). Results below.

### The write path, exercised end to end (2026-08-24, local ClickHouse)

The one thing no dry-run could test: `--execute` had never run, anywhere. Done
now against a local ClickHouse (docker compose + `db-clickhouse-init`), with a
scratch build whose three read floors are zeroed (an empty local DB trips them
by design — that build is not committed).

An empty "our side" makes the whole network missing, so the test is BIGGER
than the production run will be, and it exercises the account-stub insert path
that production (0 stubs) never will:

| table                 | inserted   | counted in CH after | match |
| --------------------- | ---------- | ------------------- | ----- |
| `balances`            | 43,307,561 | 43,307,561          | exact |
| `account_entry_state` | 10,892,282 | 10,892,282          | exact |
| `assets`              | 394,033    | 394,033             | exact |
| `accounts`            | 10,892,282 | 10,892,282          | exact |

**~65.5M rows, exit 0, row-exact on all four tables.** 937s total, 3.19 GB
peak RSS. Unresolved references 0 and 0 even from an empty database. This
closes the 0310-class risk (client-side insert rejection on struct/schema
mismatch) for all four row types against the real schema.

Content check, not just counts: the issue #377 fixture account rebuilt from
NOTHING but the snapshot shows all five holdings (native + AQUA/KALE/SHX/USDC,
the three zeros at their exact ledgers) and thresholds 1/3/3/3 with 4 signer
keys — acceptance criterion 1 demonstrated through the write path itself.

## Acceptance criteria

- [ ] A live zero-balance trustline appears; the fixture account shows five
      assets, not two
- [ ] A **closed** trustline still does not appear — verified on an account
      with a known removal, not only the happy path
- [ ] The 873-zero-row account shows none of those 873
- [ ] Native zero balances appear (239,087 holders), watching the two-convention
      trap for native
- [ ] Signers (key, weight, type) and low/med/high thresholds are shown; the
      fixture reads as multisig (verified on chain: thresholds 3/3/3, five
      signers at weight 1 — a genuine 3-of-5)
- [ ] An account with no signers row renders an explicit "not indexed", never
      an empty list that reads as "not multisig"
- [ ] Seed coverage measured for BOTH trustlines and accounts, and
      cross-checked against an independent source regardless of the measured
      result (the 200-account chain probe; the RPC comparator was deleted
      2026-08-21 — see 0502)
- [ ] `total_supply` and `holder_count` move **only in the direction the
      network justifies**, spot-verified against RPC for at least one asset.
      (Rewritten 2026-08-18: the original criterion said "unchanged", which a
      correct seed cannot satisfy — `balance_aggregates_mv` recomputes
      `sum(amount)` / `countIf(amount > 0)` from `balances` every two minutes,
      and the seed inserts ~19.3M live holdings carrying real amounts while
      zeroing ~1.04M native ghosts. Capture both aggregates BEFORE the run so
      the delta can be checked rather than discovered.)
- [ ] The 200-account probe from `notes/R-` returns zero accounts where the
      chain holds more live zero trustlines than we do
- [ ] **Verified on production**, not at merge — the destination is a shipped,
      checked change
- [ ] **Docs updated** — `docs/architecture/**` read path and frontend data
      contract; `docs/backfills.md` gains the seed pass
- [ ] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)
