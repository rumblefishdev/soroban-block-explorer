---
id: '0463'
title: 'FEATURE: account detail — show zero-balance trustlines + signers/thresholds'
type: FEATURE
status: done
related_adr: ['0055', '0056', '0057']
related_tasks:
  [
    '0464',
    '0321',
    '0331',
    '0295',
    '0214',
    '0492',
    '0493',
    '0496',
    '0499',
    '0501',
    '0502',
    '0503',
    '0504',
    '0514',
    '0515',
  ]
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
  - date: '2026-08-26'
    status: done
    who: karolkow
    note: >
      Shipped and verified on production via tag production-2026.08.26-2.
      Both halves of issue #377 are live: the read path selects on
      `closed_at_ledger` instead of `amount != 0`, making 9,478,880 live
      zero-balance holdings visible, and the account page gained signers +
      thresholds with the master key composed in. Backed by a one-off
      checkpoint-snapshot seed that wrote 44,834,785 balance rows,
      10,871,929 signer rows and 97,108 assets, all chain-audited. 258 API
      tests, 300 web tests, 3 browser regressions. Three ADRs (0055, 0056,
      0057) and twelve follow-up tasks spawned; the capability itself is
      indexed by EPIC 0515.
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
  against the manifest hashes, and short reads are refused by floors on every
  input. (This bullet also claimed the checkpoint was pinnable so `--execute`
  ran the snapshot the dry-run reviewed. The pin was removed 2026-08-24 and
  the claim is withdrawn — see the 2026-08-26 entry.)

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
failed run, never a half-decoded one. (`--pinned-manifest` was kept as an
optional flag that day and DELETED outright on 2026-08-24; `manifest.json` is
still written, so the ADR 0056 LP merge can re-derive this seed's exact
snapshot from it.)

Executed: `--our-rows`/`--assets-ids`/`--accounts-ids` deleted; seed and
compare stream the 64-slice `argMax` read through one shared
`stream_our_rows` (short-read floor inside); id sets fetched by query;
`snapshot-export-sql` and the TSV transport deleted (with `OurRow::parse` and
its test); the chq exit-0 trap and the one-dropped-slice hole (found by the
design-study agent: 40M floor passes a 64th-slice loss) are gone — a cursor
error propagates. Freshness — the measured dominant lever on correction
volume — moves from runbook discipline into the tool itself. Net −~200
lines; operator flow: dry-run → read summary → `--execute`.

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

### Decision — how the master key is rendered (owner, 2026-08-24)

The ledger carries the account's own key as a WEIGHT (`thresholds[0]`), not as
a list entry; additional signers are the separate list. Our schema stores them
apart, which is correct, and it is also the shape that misleads: a page
rendering only `signer_keys` shows the issue #377 fixture as 3-of-4 when the
chain says 3-of-5.

**Chosen: the master key is rendered as the FIRST ROW of the signers list**,
carrying `master_weight` and a "master" badge. Five rows for the fixture. This
matches what other explorers show, so the count does not surprise anyone
comparing sources, while the badge keeps the row honest about being the
account's own key rather than an added signer.

Rejected: a separate "master key weight" field beside the list (correct but
leaves the reader to do the arithmetic that produces the security claim), and
a computed "3-of-5" header over a 4-row list (the header and the list would
disagree on screen).

Constraint for the API/DTO work: the total that matters is
`master_weight + sum(signer_weights)`, and the thresholds are compared against
THAT. An account with `master_weight = 0` has a disabled master key and must
NOT render the row as a signer with weight 0 reading as ordinary — 41 of 5,000
sampled accounts are permanently locked this way.

### Deployed 2026-08-24 evening — the writer validates against the chain

PR #428 merged to master (`6cd072c5`) and the indexer deployed. Verified by
reading production, not by assuming:

| check                              | result                                                             |
| ---------------------------------- | ------------------------------------------------------------------ |
| lifecycle writer stamping          | `closed_at_ledger` non-zero — first stamp at ledger **64,115,052** |
| live signer writes                 | `account_entry_state` carrying production rows                     |
| ingest after the container recycle | our head **equals** the chain head to the ledger                   |

That last row is not a formality: deploy + container recycle is the window
where the ClickHouse driver rejects inserts client-side on a struct/schema
mismatch, which stopped ingest for nine minutes in task 0310. It did not
recur.

**The three earlier dry-runs are void for `--execute`.** Their checkpoints
(64,106,239 / 64,106,495 / 64,107,135) all sit BELOW the deploy ledger, and the
ordering contract requires a checkpoint taken after it — otherwise every
removal in the gap was written by the OLD writer as a plain zero at a ledger
above the checkpoint, outversioning the seed's closure and resurrecting the
ghost. A fresh run at checkpoint **64,115,135** replaces them.

#### The defect signals fire for the first time — and read zero

Before the deploy nothing was ever stamped closed, so `AlreadyClosed` and both
`ClosedButLive*` verdicts were structurally unreachable: they could not have
caught anything. This run is the first where they could:

```
already marked closed            182 classic + 37 native
CLOSED BUT LIVE (re-opened)        0 + 0
CLOSED vs LIVE conflict (defect?)  0 + 0
```

219 stamps, none contradicted by the snapshot. That also exercises the
checkpoint-guard fix in the condition it was written for — post-deploy churn
meeting a snapshot that predates it.

#### Every stamp the deployed writer made, checked against chain

All 557 distinct keys carrying a stamp (482 trustlines + 75 native), resolved
to StrKeys and fetched from RPC:

- **554 absent from chain** — correctly closed.
- **3 apparently present, none a defect.** One (`GCYNGANMUF…`) has a NEWER
  open row on our side: `closed_at = 0`, 1.5 XLM at ledger 64,115,211, and the
  chain agrees exactly — same `lastModifiedLedgerSeq`, same balance. It
  surfaced only because an OLDER row carries a stamp, which is the account
  being merged and then RE-CREATED at the same address, with both states
  recorded. The other two were present on the first probe and gone minutes
  later on the second — the same merge/re-create/merge pattern, caught
  mid-cycle.

Account re-creation is worth recording as a shape this writer handles: the
address returns, and the newest row wins on RMT version, so a closure does not
become permanent.

#### Post-deploy run — what `--execute` would write (checkpoint 64,115,135)

|                        | rows                                            |
| ---------------------- | ----------------------------------------------- |
| `balances` corrections | 44,852,492                                      |
| `account_entry_state`  | 10,894,330                                      |
| `assets` stubs         | 97,108                                          |
| `accounts` stubs       | 0                                               |
| unresolved references  | **0 assets, 0 holders** (464 issuers, cosmetic) |

Structural audit: 0 failures. Missing trustlines at/above our floor: still 0.

#### The deployed writer, checked against an external source

Not "is it running" but "does it write the truth". Both write paths and both
halves, every check against RPC with the independent decoders:

| what                               | check                                             | result                                                                |
| ---------------------------------- | ------------------------------------------------- | --------------------------------------------------------------------- |
| signers written by the LIVE writer | thresholds, signer set AND order vs chain         | **377 / 377 identical** (22 churned after our ledger, 1 merged since) |
| account-merge closures             | absent from chain                                 | 554 / 557                                                             |
| account-merge closures             | is there an `account_merge` at OUR stamped ledger | **5 / 5 SUCCESS**                                                     |
| trustline closures                 | absent from chain                                 | 6 / 6                                                                 |
| trustline closures                 | is there a `change_trust` at OUR stamped ledger   | **6 / 6**                                                             |

The second and third rows matter more than absence alone: absence proves we
closed something that is gone, the ledger match proves we closed it _for the
right reason at the right moment_. Deleted entries leave no trace in
`getLedgerEntries`, so the timing had to be verified from the ledger's
transaction set instead.

Two false alarms along the way, both mine, recorded so they are not re-derived:

- Three merge closures looked "still present on chain". One
  (`GCYNGANMUF…`) has a NEWER open row on our side agreeing with the chain to
  the balance and ledger — the account was merged and then RE-CREATED at the
  same address, both states recorded, newest winning on RMT version. The other
  two were present on one probe and gone minutes later: the same
  merge/re-create/merge cycle caught mid-way. Account re-creation is a shape
  this writer handles; a closure does not become permanent.
- Four trustline closures appeared to have no matching transaction in their
  ledger. The join that produced them took the CROSS product of five holders
  and five assets and then zipped ledgers positionally, so the (holder, asset,
  ledger) triples under test were never real rows. Re-run with the ledger
  carried through the join: 6 / 6.

### External review 2026-08-26 — verified independently before acting

An outside review (8 agents, 3 adversarial rounds) produced 32 items over the
snapshot module. Every claim was re-checked at source before anything was
changed: each `file:line` opened, each cited commit resolved, library behaviour
read in the vendored crate source, and production claims measured with `chq`.
The review is mostly sound — all 8 cited commits exist with the descriptions
attributed to them — but it carries four errors, one of them in a blocker's
recommendation.

#### The largest open question is now CLOSED by measurement, not argument

The review's blocker B2 hypothesised `assets` rows whose `id` came from an
older derivation: unkeyable by the snapshot, so their balances would fall into
the absence arm and be zeroed and stamped closed, with no floor firing because
the row count never changes. Only the verdict flips — silent for the
zero-amount half, which is exactly the class this task exists to fix.

Answered with a throwaway read-only falsifier (not committed; the check cannot
be SQL, because ClickHouse's `cityHash64()` is a different algorithm from the
low 64 bits of CityHash128 that `ids::` uses):

| arm of `asset_id()`                 | rows checked | not derivable |
| ----------------------------------- | ------------ | ------------- |
| type 1 — `hash64("CODE:issuer_id")` | 343,987      | **0**         |
| type 3 — `id == contract_id`        | 4,380        | **0**         |
| type 0 — the `native` constant      | 7            | **0**         |

Plus, in pure SQL over the same table: **0 identities carrying more than one
`id`, and 0 `id`s carrying more than one identity.** That bijection is the
corroborating argument — had the formula ever changed, the live writer (which
uses today's) would have inserted a second row for the same identity under the
new id. It has not done so once in ~344k assets.

So B2's population is empty and its proposed permanent `--execute` refusal has
nothing to refuse.

#### Four errors in the review, recorded so they are not re-derived

- **Its recommended fix for B3 cannot work.** `--expect-checkpoint <N>` would
  have `--execute` refuse unless the freshest checkpoint equals the one the
  dry-run named. Checkpoints publish every 64 ledgers (~5 min) and a pass takes
  ~15, so that condition is false by construction on every run. The workable
  shape is the deleted `--pinned-manifest` (it read the dry-run's own
  `manifest.json`), not a freshness assertion.
- **F5's "exists ONLY in a commit message" is false.** The 1000/1000 chain
  check is at README line 694 and the full 11,994 + 3,772 → 0 transition table
  at 829-831. Only the dimension-id counts are commit-message-only. F5 also
  demands chain evidence for a bucket that measures **zero** after the fix;
  what actually remains is F7 (`NewerThanCheckpoint` is unsampled), which
  already says it.
- **`docs/backfills.md:497` does not repeat the tie guarantee.** That phrase
  occurs exactly once in the repo, in ADR 0057 line 78.
- The summary `format!` takes **16** positional arguments, not 18. The
  objection to positional formatting stands; the number did not.

#### One place the review holds itself to two standards

B1 asks for a test proving `key_slices()` covers i64 exactly once. Read
closely, the coverage is exact for ANY value of `KEY_SLICES`, not just powers
of two: the final slice ends at `hi` by an explicit arm, and slice `s+1` starts
one past slice `s`'s end. A test would assert a fact about arithmetic — which
is precisely why the same review deletes `checkpoint_lattice_accepts_only_63_mod_64`
under D7. The other two tests B1 asks for (dangling counters,
`build_corrections`) target real untested logic and stand: `seed.rs` has **0**
tests against 12 in its siblings, and it is the only module that writes.

#### Measured while verifying — a number the review did not have

`slice_sql` excludes rows whose asset has no `assets` row. Measured on
production: **1,616 (holder, asset) keys across 159 unknown asset ids.** They
belong to no exclusion bucket, so `summary.txt` does not sum to the table.
Harm is low (a re-insert lands on the same key), but the gap should be
counted rather than invisible.

Also: the unsampled set is wider than F7 states — `AlreadyClosed` and `Stale`
have no sample arm either. And the review's ground rule about
`changeable_in_readonly` could not be verified: that column does not exist in
`system.settings` on CH 26.3.10.60. Its practical conclusion was confirmed
directly instead — `dev_read`, `readonly = 1`, `max_execution_time = 30`.

### Acted on 2026-08-26 — three decisions, and the corrections they implied

**Snapshot drift: the claim is retracted, the pin is not restored** (B3, owner's
call). The deleted flag's original job was keeping a frozen manual export
consistent with the snapshot, and manual exports are gone; an unpinned run
decodes the FRESHER snapshot, which is a better input. What was wrong was the
documentation: `fold_our_row` asserted the drift is "only rows newly LEFT
ALONE, never a different correction", reasoning about our side only. The
snapshot side moves too, so holdings the network creates between the two
checkpoints are `missing` in the execute run and get INSERTED without appearing
in the reviewed summary. Now stated plainly in the code, in `docs/backfills.md`
and in the three stale README lines (:306, :534, :546). The reviewed document
BOUNDS a run's counts; it does not enumerate its rows — and the run is verified
by measuring its outcome against the network, which no frozen input improves.

**`ClosedButLiveConflict` keeps its policy and loses its reason** (F1). The
comment claimed no honest version could supersede our closure. The file refutes
that twice: the guard proves every row on that arm sits below the checkpoint,
and `correction` already stamps closures at the checkpoint, which ADR 0057
blesses. A presence fact carries the same way — an entry in the checkpoint's
bucket list IS live at the checkpoint. The real reason is decay coupling: a
one-off heal against an ongoing writer defect is the same trap that removed the
same-ledger heal (task 0514), and the bucket's value is reading zero on healthy
data. Reported still; the reason is now the true one.

**`ClosedButFunded` — built, then REVERTED the same day** (F2). Worth
recording as a full loop, because the reverting evidence is the useful part.

The finding was real: ADR 0057 decision 2 promises the checkpoint version
"deterministically supersedes both sides of any tie", and `AlreadyClosed`
returned no correction, so for one shape it could not deliver that. A twelfth
verdict was added to repair a row that says CLOSED while still holding a
positive amount — zeroed at the checkpoint version (strictly above the row's
own ledger, so it supersedes both tied rows) while KEEPING the closure ledger
the writer recorded.

It was justified partly on the tie surfacing as a MIX — the funded amount from
one row married to the closure stamp from another, which three independent
`argMax` aggregates permit. Measuring that argument is what killed the verdict:

| measurement                                                   | result |
| ------------------------------------------------------------- | ------ |
| tie rows carrying a non-zero `closed_at_ledger`               | **0**  |
| rows in the whole table with `closed_at != 0 AND amount != 0` | **0**  |
| stamped rows overall (`closed_at != 0`)                       | 54,720 |

The 2×2 has an empty cell, and it is the one this verdict fires on. Nor can a
writer produce it: the parser sets `closed = change_type == "removed"`, and a
removed entry carries balance 0.

Decisive, though, is that **the tuple `argMax` and this verdict are two
treatments of the SAME mechanism.** A mix was the only route to a
closed-and-funded row, and one `argMax` over a tuple makes a mix structurally
impossible. After that fix the verdict covers nothing that can occur — so it
went, and `correction()` went back to three arguments.

What survives instead is honesty about the gap neither closes: a tie whose
surviving side is the coherent `{amount: 0, closed_at: L}` reads as an ordinary
closure, writes nothing, and lives on. **ADR 0057 decision 2 was amended** to
state its real scope — the guarantee holds whenever the surviving side produces
a correction, which is every tie measured to date — and `docs/backfills.md`
names the exception so it is not rediscovered as a surprise.

**Four comments that were actively false, corrected:**

- `seed.rs` claimed the SELECT-to-`OurRow` mapping is positional and "would
  silently misclassify every row rather than fail". Read in the vendored
  crate: `clickhouse` 0.15 builds a name-to-field mapping per cursor
  (`RowMetadata::new_for_cursor`, unconditional — there is no `validation`
  feature) and returns `SchemaMismatch` on a count mismatch or an unknown
  column. Two review agents built findings on that one sentence.
- `snapshot.rs` said `archive` + `network_state` "know nothing about our
  schema". `network_state` calls `ids::` six times to derive OUR surrogate
  keys. Only `archive` is schema-free; the seam still falls where it did,
  because `ids` is a pure hash module that travels with it.
- `archive.rs`'s lattice test defined its own predicate and asserted against
  it, so deleting the production check left it green. It now calls a real
  `is_checkpoint`, and gains the case that pins the FREQUENCY (`!is_checkpoint(31)`
  — every case it had still passes at a frequency of 32). Honest limit: it
  still would not catch deletion of the call site, which needs a mocked fetch.
- Stale module names (`snapshot_report`, `snapshot_seed::slice_sql`) left over
  from the file-tree refactor.

Docs also gained the drift statement, lost Horizon as a validation source
(legacy — it synthesises fields the ledger lacks), and the two duplicated
reconciliation steps collapsed into one.

Deferred, not dropped: F3/F4 (identity precheck, moving the two `uniqExact`
reads above the decode), D1 → F6/F7/F8 (collapse the 11-name triplication,
then add the missing native twins and sample arms), B1's two real tests,
F9/F10 (heal dumps, ghost holders resolved from `accounts`), F13-F18 and the
D-series refactors.

### The two-`argMax` hazard — measured, then closed structurally (2026-08-26)

A follow-up review item: `slice_sql` ran THREE independent aggregates, so on a
same-version tie `amount` and `closed_at_ledger` could in principle be taken
from different rows, assembling a row that exists in no part on disk.

Measured over one full key slice before changing anything:

| measurement                                                         | result     |
| ------------------------------------------------------------------- | ---------- |
| keys in the slice                                                   | 762,955    |
| ties `argMax` actually has to resolve (at the winning version)      | **19,142** |
| rows where the two-aggregate form disagreed with one tuple `argMax` | **0**      |
| tie rows carrying a non-zero `closed_at_ledger`                     | **0**      |

Two conclusions, and they point in different directions:

- **The hazard is real but currently cannot fire.** Every one of the 1,238,583
  known ties predates `closed_at_ledger`, so the column is 0 on BOTH sides and
  no assembly of them can differ from a real row. The last row is the decisive
  one — a "mix" needs the two sides to disagree in the second column, and none
  of them do.
- **The independence never showed itself either.** ClickHouse keeps the
  first-encountered maximum and both aggregate states walk the same rows in the
  same order, so they agreed 19,142 times out of 19,142. That is an
  implementation property, not a documented contract.

Taken: ONE `argMax` over a tuple, unpacked in the existing outer SELECT. Proven
equivalent on 762,955 keys and verified against production for column names and
types (`Int64/Int64/Int128/Int64/Int64`, matching `OurRow`). It closes the shape
a FUTURE tie can take, once the deployed writer's stamps start landing at
contended versions — not the historical population, which `Closure`/`Ghost`
already supersede.

**This measurement is also what retired `ClosedButFunded`** (above): that
verdict existed to catch the mix, and one tuple `argMax` makes a mix
impossible, so the two were alternative treatments of one mechanism and the
structural one won.

The residual is written down rather than implied: **a tie whose surviving side
is the coherent `{amount: 0, closed_at: L}` still escapes.** It
reads as an ordinary closure, writes nothing, and both tied rows live on for
the API's own `argMax` to re-flip. Catching that needs the read to carry the
tie itself — distinct contents at the winning version — which is a second
aggregation level over ~78M rows. Not built, because the population that could
produce it does not exist yet: the generating process stopped ~2 months before
the lifecycle writer did its first stamp.

### Dry-run at checkpoint 64,131,071 (2026-08-26) — audited, and one committed doc contradicted

First run on the post-review code. 317 s, exit 0, checkpoint **above the deploy
ledger 64,115,052**, so the ordering contract holds.

#### The report's own arithmetic reproduces exactly

Recomputed by hand rather than trusted, because the completeness invariant is
the only thing standing between a mis-bucketed row and a silent one:

| check                                                  | result                                           |
| ------------------------------------------------------ | ------------------------------------------------ |
| eleven verdict buckets, classic + native, summed       | **49,650,721**                                   |
| rows the run says it folded                            | **49,650,721** — exact                           |
| writing verdicts (missing + closures + ghosts) summed  | **44,834,825**                                   |
| `balances` corrections reported                        | **44,834,825** — exact                           |
| `account_entry_state` rows                             | **10,872,679** = live accounts, exact            |
| `ghosts.tsv` lines                                     | **1,057,618** = 1,941 + 1,055,677, exact         |
| asset stubs: rows / distinct ids / distinct identities | **97,108 / 97,108 / 97,108** — still a bijection |

Residual of 46 between native live accounts and our matched rows resolves as
accounts created between the checkpoint and the read — the only bucket that can
absorb them is `newer than checkpoint`, and it is 10,038 against 9,992 needed.

#### Chain audit of THIS run, independent implementation

StrKey, `LedgerKey` XDR and `TrustLineEntry` decoding re-implemented from spec
per `notes/V-`, gated on `lastModifiedLedgerSeq` so churn is never counted as
disagreement:

| bucket                                         | expectation | result                                                                              |
| ---------------------------------------------- | ----------- | ----------------------------------------------------------------------------------- |
| `missing_classic` (the INSERT set)             | present     | **200/200 present, 200/200 identity echoed, 200/200 frozen, 200/200 balance exact** |
| `agree_classic` (positive control)             | present     | **200/200**, all four the same                                                      |
| `closure_classic`                              | absent      | **200/200 absent** — the read-filter flip resurrects nothing                        |
| `ghosts_native` (1.06M rows zeroed, 45.4M XLM) | absent      | **300/300 absent** (holders resolved through our `accounts`)                        |

Zero rows where the chain's ledger sits BELOW ours, which would have been a
defect rather than churn.

#### The defect signals fire for real this time, and read zero

The previous post-deploy run had 219 lifecycle stamps to test against. This one
has **56,226** (25,667 classic + 30,559 native), two days of the deployed
writer:

```
CLOSED BUT LIVE (re-opened)        0 + 0
CLOSED vs LIVE conflict (defect?)  0 + 0
```

Not one of 56,226 closures is contradicted by the network. That is the
strongest health check the writer has had.

#### Two buckets went to zero, and the reason is the self-read

`heal` and `stale` are now **0 in both populations**, where the module docs
still advertise "self-heal (snapshot newer) ~25k". Both count rows where the
snapshot is NEWER than ours — impossible once our side is read live at run
time, because we are never behind the checkpoint. Anything where we are ahead
lands in `newer than checkpoint` (12,444 + 10,038). The ~25k figure dates from
the frozen-TSV era and should be read as history, not as an expectation.

`divergent SAME ledger` is 17,984 native / 0 classic, up from 17,739 — ~245 in
one day, tracking the ~1,900/week in task 0514, and still one-directional and
Soroban-localised. Quarantined, writes nothing.

#### A doc committed hours earlier is already contradicted by measurement

`docs/backfills.md` and `seed.rs` state that a full pass "takes ~15" minutes
against a checkpoint interval of ~5. **Measured: 317 s** — about ONE checkpoint
interval, not three. The conclusion survives (the manifest is fetched at each
run's START, so what separates two runs is a whole run plus the operator
reading `summary.txt`), but the number is wrong and the margin is far thinner
than written. The 909 s figure it was based on came from a run whose cost was
dominated by archive download, which is network-bound and varies: 246 s of this
run's 317 s was still the buckets.

#### Aggregates captured BEFORE the run, per the acceptance criterion

Only useful if taken first, so recorded here rather than discovered afterwards:

| asset         | `total_supply`      | `holder_count` |
| ------------- | ------------------- | -------------- |
| native XLM    | 1054107012889362218 | 9,926,579      |
| USDC (circle) | 3201349384852260    | 631,180        |
| AQUA          | 854444817802479332  | 61,909         |
| SHX           | 995998878972179459  | 49,761         |

Expected direction after the seed: native `holder_count` DOWN by ~1.06M (the
ghosts) and `total_supply` down by ~45.4M XLM; classic supply and holders UP as
19.26M live trustlines enter.

#### Known gaps this run displays rather than hides

- `ghosts_classic` and `ghosts_native` dump **1000/1000 unresolved identities**
  — the `key_line` limitation (a dead account carries no snapshot identity).
  The native half is auditable through our `accounts` table, as above; the 1,941
  classic ghosts remain unreachable from any source.
- `closure_classic` resolves 708 of 1,000 (292 unresolved), matching the 288
  measured on 2026-08-24 — the holder was merged along with the trustline.
- `closure_native` and `agree_native` still have no dump at all (review F6).
  The 10.8M-row native `agree` count is itself the positive control for native
  keying, but there is no sample file to look at.
- `entry_states.tsv` reports "5001 of 10,872,679": the truncation marker is
  counted as a row (review F17, cosmetic).
- The ~1,616 keys whose asset has no `assets` row are still excluded from the
  comparison and absent from NOT COMPARED, so the block does not sum to the
  table (review F17).

### EXECUTED on production — checkpoint 64,131,263 (2026-08-26), and the full audit

`--execute` ran under the dedicated write cert (CN → `dev_shared`, verified
`readonly = 0` by a read BEFORE the run). 636.5 s total, exit 0,
`inserts done.` Checkpoint sits ABOVE the deploy ledger 64,115,052, so the
ordering contract held. Everything below is measured AFTER the write, most of
it against independent sources.

#### What landed, verified in ClickHouse

| table                 | run said   | verified in CH                                                                                                                               |
| --------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `balances`            | 44,834,785 | checkpoint cohort **25,572,553 − 184 organic = 25,572,368 exact**; missing cohort **19,262,417 exact** (min ledger 287,404, all below floor) |
| `account_entry_state` | 10,871,929 | 10,882,822 distinct accounts (= seed + live writer)                                                                                          |
| `assets`              | 97,108     | distinct ids 445,489 = 348,380 + 97,108 **+ 1 organic**                                                                                      |
| `accounts`            | 0          | unchanged                                                                                                                                    |

Disk: `balances` total is now **1.48 GiB on disk** (119.3M raw rows, 8 active
parts, single partition); server has 476 GiB free. The whole seed cost well
under 1 GiB of disk.

Ingest never blinked: our head equalled the chain head (64,131,563 = 64,131,563)
during and after verification, and the live writer kept stamping
(`account_entry_state` max version 64,131,437 > checkpoint). The 0310-class
schema-mismatch window did not recur.

#### Chain audit of the written state — from the DATABASE, not the dumps

Fresh samples drawn from production AFTER the write, resolved through our own
dimension tables, verified against `getLedgerEntries` with the independent
spec-implemented decoders:

| population                                                      | result                                                                                                                                                          |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| inserted missing rows, eras 10.1M–50.4M (incl. 30 zero-balance) | **120/120 present, identity echoed, frozen at our ledger, amount exact**                                                                                        |
| native ghosts (head + middle + tail of ghosts.tsv)              | **300/300 absent from chain**; in CH, 632+300 keys read `amount 0, closed_at 64131263` through `argMax`                                                         |
| warehouse accounts (8,547 / 890 / 890 stamped rows)             | **0 live zeros remain** on any of them                                                                                                                          |
| fixture `GDXWIA4V…`                                             | 5 rows: native + KALE positive, AQUA/USDC/SHX **0 at ledgers 58,469,457 / 58,469,453 / 59,023,860**; signers `1 + 4×1`, thresholds 3/3/3                        |
| entry-state versions (min + median band)                        | **10/10 equal to chain's `lastModifiedLedgerSeq`** — the 56M floor on account versions is a NETWORK fact (a mass rewrite band ~56.01–56.09M), not a seed defect |

#### External indexer cross-check — stellar.expert, funded-trustline counts

Two fully independent pipelines, compared after our aggregate refresh:

| asset | our `holder_count` | stellar.expert `funded` | delta       |
| ----- | ------------------ | ----------------------- | ----------- |
| AQUA  | **129,438**        | **129,438**             | **0**       |
| SHX   | 54,737             | 54,736                  | 1           |
| USDC  | 682,443            | 681,989                 | 454 (0.07%) |

AQUA — whose holder count our seed DOUBLED (61,909 → 129,438 by waking
pre-floor dormant lines) — matches to the row. Supplies read lower than
stellar.expert's issued supply by the value sitting in pools/contracts, which
is the NOT COMPARED set, as designed.

#### The native supply question — RESOLVED, and it corrects 2026-08-24

> **Read the adversarial pass below before quoting these figures.** Three of
> them were later tightened: the AMM term was two days stale (drift 381k XLM,
> 40% of the residual), "our native sum" silently includes 920.8M XLM of
> contract-held balances, and the stellar.expert agreement is real but not
> "exact to the row".

The predicted "supply moves down ~45.3M XLM" did NOT happen, and chasing that
discrepancy produced the session's best result. What actually happened:

- The seed's ghost accounting (45.2M XLM) was per ITS OWN `argMax` read. The
  aggregate MV reads through `FINAL`, and on the 1.24M same-version ties the
  two rules flip different coins — the MV had ALREADY been excluding most
  phantom value before the seed. The seed therefore did not move the sum much;
  **it made the sum deterministic** (checkpoint rows outversion both tie
  sides). Verified at population scale: of 210,572 stamped keys in slice 0,
  exactly **1** still reads funded (an organic same-ledger write).
- Ground truth after: 16-slice `argMax` sum = **105,410,270,563.77 XLM**,
  funded 9,925,836 — and `FINAL` now agrees (9,925,837 / within 370k XLM of
  churn window), which it could not before.
- Every funded key was verdict-checked against the snapshot at FULL population
  (`agree` = equal amounts), so phantom value in the native sum is now
  structurally impossible, not just sampled-away.

Against the validators' own number (header decoded from `getLedgers`,
byte-offset past the history-entry hash):

```
total_coins                          105,443,902,087.35
our native sum (ground truth)        105,410,270,563.77
gap                                       33,631,523.58
  fee_pool (header, same decode)         10,451,269.25   <- the term 08-24 MISSED
  AMM pool reserves (measured 08-24)     22,231,810.44
  remainder = claimable balances            948,443.89
```

**The books close to ~0.95M XLM (0.0009%), every term measured but the last.**
This retro-corrects 2026-08-24: the "pre-seed gap too small → phantom XLM
present" inference and the "55.8M plausible for claimables" figure were both
wrong for the same reason — the fee pool was absent from the model. The
pre-seed gap was already right; the missing 10.45M was burned fees, and
claimables hold ~0.95M, not ~55M.

#### Aggregates moved only where the network justifies

Captured BEFORE → read AFTER (post-refresh):

| asset  | holders before → after | supply direction                             |
| ------ | ---------------------- | -------------------------------------------- |
| native | 9,926,579 → 9,925,837  | ~flat — see above; determinism, not drift    |
| USDC   | 631,180 → 682,443      | up (+2.31M USDC of dormant lines)            |
| AQUA   | 61,909 → 129,438       | up — doubled, matches stellar.expert exactly |
| SHX    | 49,761 → 54,737        | up                                           |

#### Is a future run idempotent? Yes — verified by construction

The question: would the next `snapshot-seed` re-insert what this one wrote,
including the pre-floor rows? No:

- seeded live rows (below-floor, entry's own version) → next run reads them,
  network unchanged → `Agree` → writes nothing (the 13.1M `agree` bucket of
  THIS run already proves the path);
- seeded closures/ghosts (`closed_at = 64,131,263`) → network absent →
  `AlreadyClosed` → writes nothing;
- asset stubs → now in the known-id set → no stubs;
- churn since → exactly the small corrections a reconciliation SHOULD write.

And if any identical row were ever re-inserted anyway, RMT collapses
byte-identical duplicates harmlessly.

#### Residue for the record

- `already marked closed` grew 25,667+30,559 → 25,713+31,362 between dry-run
  and execute — the live writer stamping in the gap, as designed.
- `divergent SAME ledger` 18,012 — task 0514's live Soroban defect, untouched
  by design, still one-directional.
- The fresh dry-run before execute was skipped DELIBERATELY: `--execute`
  always decodes a newer checkpoint than any dry-run, so a same-day dry-run
  adds review theatre, not review. The 64,131,071 dry-run (fully audited,
  0.04% drift) is the reviewed document; delta at execute matched it.

### What the seed achieved, in numbers (2026-08-26)

The index before this work was not merely incomplete — on several questions it
answered confidently and wrongly. What changed:

| question a visitor could ask               | before                        | after                                             |
| ------------------------------------------ | ----------------------------- | ------------------------------------------------- |
| "does this account hold asset X?"          | 60% of live trustlines absent | **+19,262,417** holdings added                    |
| "is this account multisig?"                | nothing indexed               | **10,871,929** accounts with signers + thresholds |
| "does this asset exist?"                   | 97,109 real assets unknown    | **+97,108** assets, every one chain-checked       |
| "is this holding closed or empty?"         | indistinguishable             | **24,514,784** closures stamped explicitly        |
| "does this merged account still hold XLM?" | 1.04M phantom balances shown  | **1,036,526** zeroed, 300/300 verified gone       |
| holders of AQUA                            | 61,909                        | **129,438 — equals stellar.expert to the row**    |

**9,478,880 live zero-balance holdings** (8,530,451 classic + 948,429 native)
now exist as distinguishable facts rather than being dropped by a filter — the
population issue #377 reports, measured after the write.

Coverage: **0** missing trustlines at or above our ledger floor. Every gap the
snapshot found predates our indexing window, so the 60% hole was coverage, not
a parser defect — the discriminator returning the good answer.

Audit totals against the chain, all with independently implemented decoders:
**200 + 120 + 80 = 400** inserted rows sampled across ledgers **635,690 →
50.3M** (2016 → 2024, including 52 twelve-char codes and 30 zero-balance
lines) — present, identity echoed, frozen at our ledger, amount exact to the
stroop, **400/400**. Plus 300/300 ghosts absent, 200/200 closures absent,
200/200 positive control present.

#### The capability is now a debugger, not just a loader

The same pass that writes also answers "where are we wrong?", and it did so
three times in one day on facts nobody had questioned:

- **the fee pool.** Reconciling native supply against the validator-signed
  header forced the 10.45M XLM fee pool into the model. The 2026-08-24 entry's
  "phantom XLM" inference and its ~55.8M claimable estimate were both wrong
  for that one missing term; the books now close to **~0.95M XLM (0.0009%)**.
- **the same-ledger ties.** The seed's checkpoint-versioned writes made
  1.24M coin-flip keys deterministic — verified at population scale (of
  210,572 stamped keys in one slice, exactly 1 still reads funded, and that
  one is organic).
- **the live Soroban defect** (18,012 rows, task 0514) surfaced only because
  the comparison exists at all, and is now tracked run over run.

Two defect signals (`CLOSED BUT LIVE`, `CLOSED vs LIVE conflict`) read **0
against 57,075 lifecycle stamps** — the strongest health check the writer has
had, and a monitor that only means something because it can fire.

#### "One organic asset" — identified

The post-run asset count kept drifting up (348,380 → 348,409 in 40 minutes).
Not seed residue: these are assets created ON CHAIN after the checkpoint and
written by the LIVE indexer. First one is `SHIPSTOCKS` (issuer
`GDXLMDAW…GA33`) whose first row sits at ledger **64,131,264 — the ledger
immediately after the checkpoint 64,131,263**; the issuer is confirmed live on
chain. Roughly 40 new assets/hour, which is ordinary network activity and the
reason any "expected total" has to be a rate, never a fixed number.

### Adversarial pass on the day's own conclusions (2026-08-26)

The seed was verified from many angles; the LEAST checked artifacts were the
conclusions drawn from it hours earlier — one of which overturned a previous
entry, which is the shape that gets it wrong twice. Attacked deliberately.
Three hits, all by measurement.

#### 1. The residual was quoted more precisely than its inputs allow

The reconciliation subtracted an AMM figure measured **two days earlier** from
sums measured today. Re-measured now:

| term                     | 2026-08-24    | today         |
| ------------------------ | ------------- | ------------- |
| XLM in AMM pool reserves | 22,231,810.44 | 22,612,827.69 |

**The two-day drift is 381,017 XLM — 40% of the ~948k residual it feeds.** So
"0.0009%" was false precision. With today's figure the residual is 567,427 XLM
(0.00054%). The conclusion survives comfortably — the books close either way —
but the honest statement is "under ~1M XLM, and the AMM term must be measured
in the same pass", not a six-digit number.

(Caught mid-flight and worth recording: `reserve_a`/`reserve_b` are
`Decimal(38,7)`, so they already carry the scale. Dividing by 1e7 gives 2.26
XLM instead of 22.6M — a wrong answer that looks like a plausible small number
rather than an error.)

#### 2. "Our native sum" includes 920.8M XLM that is not on any account

Measured: **968 contract keys hold 920,803,987.44 XLM** (SAC `ContractData`,
re-keyed onto native by task 0331). The 16-slice ground-truth sum includes them.

That is arithmetically RIGHT for reconciling against `total_coins`, which also
counts them — but it was labelled "our native sum" with no qualifier, and the
2026-08-24 entry it corrects was reasoning about the sum of `AccountEntry`
balances. Those two quantities differ by 920.8M XLM. Stated both ways now:

```
total_coins                            105,443,902,087.35
our native sum, ALL holders            105,410,270,563.77
  of which contract-held (ContractData)     920,803,987.44
  accounts-only equivalent            104,489,466,576.33
gap to total_coins                          33,631,523.58
  fee_pool                                  10,451,269.25
  AMM reserves (measured same day)          22,612,827.69
  residual (claimable balances)                567,426.64
```

#### 3. The stellar.expert agreement is real — the reason given for it was not

The claim "AQUA matches to the row" was made without ever checking what
stellar.expert means by `funded`. Ours counts every holder; theirs is labelled
under `trustlines`, and a contract holding a classic asset has no trustline.
If the definitions differed, the match was luck. Measured on three assets:

| asset | ours, all holders | ours, trustlines only | stellar.expert `funded` |
| ----- | ----------------- | --------------------- | ----------------------- |
| AQUA  | 129,434           | 129,084               | 129,438                 |
| SHX   | 54,739            | 54,687                | 54,736                  |
| USDC  | 682,274           | 641,773               | 681,989                 |

The pattern settles it: their number tracks our ALL-HOLDERS count to within
0.04%, and diverges from trustlines-only by 6.3% on USDC. **They do count SAC
contract holders**, so the comparison is like-for-like and the agreement
stands. What does not stand is "exact to the row" as evidence — SHX is ±3 and
USDC ±285, so AQUA landing exactly is coincidence. The defensible claim is
"two independent pipelines agree within 0.04% on funded holders".

#### Idempotency is still argued, not measured — and the test is free

The claim that a second run writes nothing was derived from the verdict table,
never observed. A **dry-run costs 5 minutes, writes nothing, and runs under the
read-only identity**, so there is no reason to leave it as an argument.
Falsifiable predictions for the next dry-run at any fresh checkpoint:

| bucket                     | this run   | predicted next |
| -------------------------- | ---------- | -------------- |
| `missing` classic          | 19,262,417 | ~0             |
| `closure` classic          | 22,205,262 | ~0             |
| `closure` + `ghost` native | 3,365,165  | ~0             |
| `already marked closed`    | 57,075     | ~25.6M         |
| `agree` classic            | 13,134,885 | ~32.4M         |
| asset stubs                | 97,108     | ~0             |
| `balances` corrections     | 44,834,785 | churn only     |

Any bucket that fails to move locates the defect precisely. The safety
property worth naming: if the seed closed something wrongly, the next run
reports it as `CLOSED BUT LIVE` — the seed's own output is audited by the next
reconciliation, which is why the two defect signals reading 0 matters.

### The seed made "we have not looked" a false statement (2026-08-26)

The signers section shipped with a WARNING chip for accounts carrying no
`account_entry_state` row, on the reasoning that a missing row is an unknown
and an unknown must not read as an answer. The owner questioned it — the seed
had just written entry state for every live account — and the challenge was
right twice over.

**First correction, mine.** The claim "3,910 accounts exist on chain but have
no signing state" came from a `LEFT JOIN` whose unmatched rows default
`closed_at_ledger` to 0, so accounts with NO native row at all were counted as
live. Re-measured with a semi-join, that population is **0**.

**What the row-less accounts actually are**, per key slice:

| category                              | accounts |
| ------------------------------------- | -------- |
| native tombstone — merged             | 41,008   |
| no native row at all — seq 0 skeleton | 3,910    |
| **live native row**                   | **0**    |

All 3,910 skeletons carry `sequence_number = 0` and 3,896 have no balance row
of any kind: addresses we recorded because something referenced them, never
because we parsed their entry.

**Then the chain settled it.** StrKey, `LedgerKey::Account` and the RPC call
re-implemented from spec per `notes/V-`, both controls passing (the issue #377
account PRESENT, a known-merged account ABSENT):

| probed population             | present | absent  |
| ----------------------------- | ------- | ------- |
| skeletons, negative key range | 0       | **200** |
| skeletons, positive key range | 0       | **150** |
| merged (tombstone)            | 0       | **100** |

**450 of 450 absent, zero exceptions.** So a missing row is not thin coverage —
it is the chain saying the account has no entry. The warning was wrong in 100%
of the cases it fired on, and it fired on ~3.7M accounts.

Rewritten: the ordinary case states the fact plainly and neutrally, because
dressing a known answer as an unknown is its own kind of lie. The WARNING is
kept for the one shape that WOULD be a real gap — **live holdings shown for an
account with no signing configuration**, which measures 0 today and is exactly
what a live-writer regression would look like. Both facts are already on the
page, so nothing new is fetched to decide it.

Side effect worth having: the section no longer depends on the derived
`deleted` flag, which measured **22 of 60** on merged accounts — see the
defect note below.

### `deleted` under-detects merged accounts — not this task's to fix

Measured while choosing the copy above: of 60 accounts with a native tombstone
and no entry state, only **22** have `deleted = true`. One concrete case,
`GAEGXYY63CYV34TH6HDVZ3L4WCYX7AUTLNOPFCNBR3RCQIB3MVSKLAWP`: its last operation
is an Account Merge at ledger 57,037,462, which IS its `last_seen_ledger`, and
that ledger holds exactly one type-8 appearance — but **none of the 664
appearances there names this account** as source or destination. The account
reaches its own transaction list through `transaction_participants` only.

So the merge operation is not attributed to the account being merged.

**Fixed by not deriving it at all (owner's call: fundamentally and simply).**
Native XLM lives on the `AccountEntry`, so "the account was removed" and "its
native holding was closed" are one fact, and ADR 0055's lifecycle column
already records it — the indexer stamps live removals, the checkpoint seed
stamped everything that had gone before our floor. That column is what makes
this readable now and was not before the seed ran. A fact cannot be derived
correctly from a table that does not carry it, so the read stops trying.

Chain-verified in both directions, 236 accounts, no exceptions:

| population                 | present | absent  |
| -------------------------- | ------- | ------- |
| closed native row          | 0       | **100** |
| open native row            | **100** | 0       |
| merged and then RE-CREATED | **36**  | 0       |

That third row is the case the old derivation needed `last_seen_ledger` for.
Here it falls out: a re-create writes a new open row over the tombstone, and
`FINAL` keeps one row per key — measured zero accounts holding both an open and
a closed native row. The new read is one keyed lookup with the native surrogate
BOUND from Rust (ClickHouse cannot recompute that cityhash), replacing a join
across `operations_appearances` × `transactions` on two 6.2B/3.6B-row tables.

Verified live afterwards: all four sampled accounts that previously reported
`deleted: false` while absent from the chain now report `true`; the issue #377
account and six live accounts stay `false`; four merged-then-re-created
accounts stay `false`.

The upstream attribution gap is still real and still unfixed — it just no
longer has a consumer on this page.

### S1 — idempotency MEASURED, at checkpoint 64,132,415 (2026-08-26)

The claim that a second run writes nothing was argued from the verdict table
and never observed. Seven falsifiable predictions were written down first; the
dry-run then ran read-only, 483.4 s, exit 0. **All seven hit**, five of them at
literally zero rather than "approximately".

| bucket                     | run that wrote | predicted | measured       |
| -------------------------- | -------------- | --------- | -------------- |
| `missing` classic          | 19,262,417     | ~0        | **0**          |
| `closure` classic          | 22,205,262     | ~0        | **0**          |
| `closure` + `ghost` native | 3,365,165      | ~0        | **0**          |
| `already marked closed`    | 57,075         | ~25.6M    | **25,630,171** |
| `agree` classic            | 13,134,885     | ~32.4M    | **32,393,816** |
| asset stubs                | 97,108         | ~0        | **0**          |
| `balances` corrections     | 44,834,785     | churn     | **1**          |

#### The strongest correctness result the seed has produced

`already marked closed` is now **25,630,171** — essentially every closure this
seed wrote — and against a snapshot taken 1,152 ledgers LATER:

```
CLOSED BUT LIVE (re-opened)        0 + 0
CLOSED vs LIVE conflict (defect?)  0 + 0
```

Not one of ~24.5M holdings the seed closed is live on the network. The earlier
evidence for that population was a 200-row chain sample; this is the **whole
set, checked against an independently obtained network state**. The safety
property named when the run was executed — "the seed's own output is audited by
the next reconciliation" — is no longer a property on paper.

Also at full population: `missing` is 0 in BOTH halves and at both sides of the
ledger floor, so the 19.3M gap is closed and did not reopen; `agree` classic
rose from 13.1M to 32.4M, i.e. the inserted rows now compare equal to the
network rather than being absent from it.

#### One correction written — and it lands in the known-unverifiable set

The single `balances` row is a classic GHOST, and its history is legible:

```
amount 0            ledger 64,131,263   closed_at 64,131,263   <- the seed's closure
amount 7,300,000,000 ledger 64,131,706   closed_at 0           <- the LIVE writer, after
```

The trustline (USDM0, issuer `GDM5QWWX…`) was re-created on chain after the
seed closed it, our writer recorded that correctly, and by the new checkpoint it
is gone again. The verdict is right.

Its holder has **0 rows in `accounts` and 0 in `soroban_contracts`** — an
orphan holder, the ~576-row population task 0503 owns. So the one anomaly in
49.6M rows lands exactly in the set already documented as unverifiable from any
source (and unrenderable on the account page, which bounds its harm).

#### A claim of mine that the measurement narrowed

"A future run writes only churn" is true of `balances` (44.8M → 1). It is
**false of `account_entry_state`**, which emitted **10,872,072 rows again** —
the full live-account set, every run.

The cause is structural, not a bug: pass 4 iterates every live account and
emits a row unconditionally, with no comparison against what we already hold.
The rows are byte-identical at the same version, so ReplacingMergeTree collapses
them and the data is unaffected — but a recurring reconciliation would rewrite
10.9M rows per pass for nothing. Worth a version comparison (or an
`argMax(last_updated_ledger)` read of `account_entry_state`, the way the
balances side already reads its own inputs) before this becomes routine —
relevant to the B→D sequencing in 0515, not to this task.

#### Residue

- `divergent SAME ledger` native 17,944, down from 18,012 — accounts that
  churned past their tie ledger, not a repair. Task 0514 unchanged.
- Run took 483 s against 317 s for the previous dry-run; the difference is
  archive download, which is network-bound (254 s of decode here).
- Unresolved issuer references 0, because there are no stubs at all.

### T13 verified against production data (2026-08-26)

The read filter, the signers path and the presentation were built in a separate
session; this is the independent check. Every fixture below was chosen BEFORE
the work, from populations already verified against the chain, and the API's own
`BALANCES_SQL` was extracted verbatim from the source and run against production
— so what is checked is the query that ships, not a paraphrase of it.

| gate                              | result                                                                      |
| --------------------------------- | --------------------------------------------------------------------------- |
| API tests                         | 258 passed                                                                  |
| web lint + typecheck + tests      | 296 passed (typecheck against the WORKTREE's libs, not the main checkout's) |
| API-types freshness (the CI gate) | clean — `git diff --exit-code` on the generated tree                        |
| `amount != 0` left in live SQL    | none — only in the doc comment and the guard test that forbids it           |

#### The fixtures, through the shipping query

| account                    | expected                            | got                                                                                                            |
| -------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `GDXWIA4V…` (issue #377)   | five assets, not two                | **5 rows** — native + KALE funded, then SHX / AQUA / USDC at 0 on ledgers 59,023,860 / 58,469,457 / 58,469,453 |
| `GC45LJ7N…`                | native zero beside a funded classic | **XLM 0 + USDC 1.5**                                                                                           |
| warehouse, 8,547 stamped   | none of them                        | **0 rows**                                                                                                     |
| warehouse, 890 stamped     | none of them                        | **1 row** — native 2 XLM, live and funded; 890 stamped, 0 live zeros. Correct, not a leak                      |
| merged account (ghost set) | nothing at all                      | **0 rows**                                                                                                     |

Ordering is the T5 decision, verified in the output: native first, then funded,
then amount, then recency. The three zeros come back newest-first.

#### The SAC chip now carries information

It could not fire at all before (it guessed `issuer.startsWith('C')`, and no
account has a C-prefixed address). Checked that it DISCRIMINATES rather than
merely rendering: one account's `STEM` returns `sac_deployed = false` while its
native row returns `true`, so the chip is a signal rather than a constant.

#### A number I challenged and was wrong about

`AccountSigners.tsx` states 703,871 accounts have a disabled master key. Measured
773,480, and I was ready to file it — but the alternative reading checks out
exactly: **703,906 have `master_weight = 0` AND other signers**, which is the
population the comment is about. The remaining 69,576 have no signers either,
and the UI flags those separately as an account no key can sign for. The comment
is right; the naive count was mine.

Coverage claim in the DTO also verified: 10,883,461 of 14,596,194 accounts carry
a signer row, so the documented "25% carry no row" is accurate.

#### What is NOT yet verified

**Production.** Everything above is the shipping query against production DATA
and the components under test — not the deployed page. The last acceptance
criterion stays open until the deploy, which is the operator's.

### CLOSED 2026-08-26 — shipped and verified on production

Tag **`production-2026.08.26-2`**, run 33019658856, green.

#### The tag had to be cut twice, and the first one is worth recording

`production-2026.08.26-1` was cut against a LOCAL `master` that was one merge
behind, so it pointed at the PREVIOUS release commit. The deploy ran and
**succeeded** — shipping the old code. The failure is invisible from the run:
green means the tagged commit deployed, not that the tagged commit is the one
you meant.

Caught by comparing the tag's own CONTENT rather than its name: the new read
filter appears 6 times in `accounts/queries.rs` on master and 0 times in the
tagged commit. `production-2026.08.26-2` was cut against `origin/master` and
carries both. The old tag was left in place — it is the honest record that a
deploy happened at that commit.

#### What was verified, and how

| claim                        | evidence                                                                                                                         |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| tag carries the release      | 6 occurrences of the lifecycle filter + signers DTO, read out of the tag itself                                                  |
| nothing parked outside scope | `cdk diff` — 1 stack with differences (`Compute`, the deployed one), 9 clean                                                     |
| only the API Lambda changed  | `production-soroban-explorer-api` stamped 22:34:10, inside the run window; indexer and enrichment worker still at 2026-08-25     |
| the SPA is live and new      | the deployed `AccountDetailPage` chunk fetched from the CDN contains `Signers`, `Not indexed`, `master key`; **zero** `Balances` |
| the page renders it          | the shipped frontend served against PRODUCTION data through the local API                                                        |

The last row is the one that matters, and it needed a route around the auth
layer that does not defeat it: Turnstile is correctly armed on production and
refuses an automated browser, which is exactly its job. Instead the shipped
`web/src` (verified byte-identical to `origin/master`) was served by Vite with
the repo's own dev proxy pointed at `api --bin local`, which reads production
ClickHouse over the same mTLS path the Lambda uses. Real code, real data, no
challenge bypassed.

Rendered for the issue's own account:

```
Assets — 5 assets · 2 with a balance
  Stellar Lumens  Native asset              5.9998533 XLM
  KALE            Classic credit · SAC      1.101
  SHX             Classic credit · SAC      0.00
  AQUA            Classic credit · SAC      0.00
  USDC            Classic credit · SAC      0.00

Signers — Multisig
  GDXW…YBEN  master key  [master]  1
  GACQ…AUET  1   GAWX…BXVQ  1   GCXC…YBEN  1   GDEU…EZWZ  1
  Total weight 5 · thresholds low 3 · medium 3 · high 3
```

Five assets where the report saw two; a genuine 3-of-5 where a list without the
master key would have read 3-of-4. Negative fixtures held: a warehouse account
with 890 stamped rows renders only its one live funded holding, and a merged
account renders nothing.

#### Where the rest of the work went

Nothing is left as prose. The capability this task grew is indexed by
**EPIC 0515**, whose next moves are sequenced B → D → E (extract the decoder
0502 → model the discarded entry types 0504/0503 → settle `audit-harness`).
The live Soroban writer defect this work surfaced is **0514**, root cause
proven, writer fix and heal still open. Presentation and data follow-ups are
0493 (LP positions on the page), 0496 (Soroban holdings mislabelled), 0501
(frozen trustlines), 0492 (provenance), 0497, 0499.

## Acceptance criteria

- [x] A live zero-balance trustline appears; the fixture account shows five
      assets, not two
- [x] A **closed** trustline still does not appear — verified on an account
      with a known removal, not only the happy path
- [x] The 873-zero-row account shows none of those 873
- [x] Native zero balances appear (239,087 holders), watching the two-convention
      trap for native
- [x] Signers (key, weight, type) and low/med/high thresholds are shown; the
      fixture reads as multisig (verified on chain: thresholds 3/3/3, five
      signers at weight 1 — a genuine 3-of-5)
- [x] An account with no signers row renders an explicit state, never an empty
      list that reads as "not multisig". **Amended 2026-08-26**: the original
      wording said "not indexed" for every such account, which was wrong for
      10,713 of them — a Soroban balance implies no account entry, so the
      alarm fired on correct data. Now: a CLASSIC holding with no
      configuration is "Not indexed" (measures 0, still catches a writer
      regression); `deleted` is "Closed"; otherwise "No account", and when a
      Soroban balance is on screen the card says why that is not a
      contradiction
- [x] Seed coverage measured for BOTH trustlines and accounts, and
      cross-checked against an independent source regardless of the measured
      result (the 200-account chain probe; the RPC comparator was deleted
      2026-08-21 — see 0502)
- [x] `total_supply` and `holder_count` move **only in the direction the
      network justifies**, spot-verified against RPC for at least one asset.
      (Rewritten 2026-08-18: the original criterion said "unchanged", which a
      correct seed cannot satisfy — `balance_aggregates_mv` recomputes
      `sum(amount)` / `countIf(amount > 0)` from `balances` every two minutes,
      and the seed inserts ~19.3M live holdings carrying real amounts while
      zeroing ~1.04M native ghosts. Capture both aggregates BEFORE the run so
      the delta can be checked rather than discovered.)
- [x] The 200-account probe from `notes/R-` returns zero accounts where the
      chain holds more live zero trustlines than we do
- [x] **Verified on production**, not at merge — the destination is a shipped,
      checked change (2026-08-26, tag `production-2026.08.26-2`; see the
      closing section)
- [x] **Docs updated** — `docs/architecture/**` read path and frontend data
      contract; `docs/backfills.md` gains the seed pass
- [x] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)

## The "Not indexed" signer state is wrong — measured 2026-08-26

The warning branch in `AccountSigners` reads: _"This account holds assets, but
we have no signing configuration for it. That combination should not occur."_
It occurs **10,713 times**, and the combination is not a contradiction at all —
it is ordinary Soroban.

### An address can hold tokens without being an account

`account_merge` deletes the `AccountEntry`. It does **not** touch a token
contract's storage. A SEP-41 balance is a `ContractData` entry owned by the
TOKEN contract and keyed by address, so it outlives the account, and an address
that was never funded can hold one from the start.

Both were verified on chain, not reasoned about:

| Probe (`getLedgerEntries`, independent SEP-23 / XDR implementation)         | Result               |
| --------------------------------------------------------------------------- | -------------------- |
| `LedgerKey::Account` for gap accounts, three sub-populations, both id signs | **350 / 350 ABSENT** |
| control — accounts _with_ `account_entry_state`                             | 100 / 100 PRESENT    |
| `LedgerKey::ContractData` `["Balance", Address]` for their token holdings   | **60 / 60 PRESENT**  |
| control — same key shape for live accounts                                  | 6 / 6 PRESENT        |
| **stored amount vs the `i128` decoded from the entry XDR**                  | **60 / 60 EXACT**    |

So the rows are right to the last unit. The account is gone; the token balance
is not; we hold both facts correctly. Only the page's inference is wrong.

### The population

`chq` against production, `FINAL` throughout, `positiveModulo` for slicing —
plain `%` on a negative `Int64` returns a negative remainder, so
`holder_id % 8 = 0..7` silently samples only the positive half of the id space.
That trap produced one wrong intermediate here before it was caught.

|                                                                 | Count      |
| --------------------------------------------------------------- | ---------- |
| Holders with a live `balances` row and no `account_entry_state` | ~74,000    |
| …of those, in `accounts` (reachable on the account page)        | **10,713** |
| …`deleted = true` — the account was closed                      | 9,388      |
| …no native row at all, `sequence_number = 0` — never an account | 1,325      |

**Every live row in that gap is `asset_type = 3`.** Grouped by asset type over
all eight slices: 1378 / 1370 / 1389 / 1490 / 1426 / 1438 / 1387 / 1438 — one
line of output per slice, always type 3, never anything else.

Type 3 is **Soroban**, not a pool share (`persist/ids.rs:122` — project enum
0 native / 1 classic_credit / 2 SAC-retired / 3 soroban). All 4,387 type-3
rows satisfy `id = contract_id`, which is that enum's defining property. The
`pool_share` label the API prints for them is **task 0496**, already filed;
it is what made this population look like liquidity-pool shares at first read.

Classic pool shares are not in `balances` at all — they are in `lp_positions`
(line 203 above already measures that gap), so `pool_share` had no legitimate
occurrence to be compared against.

### Why the branch only now misfires

The API used to select balances by `amount != 0`, and 147 of 179 sampled gap
rows carry `amount = 0`. Moving to the lifecycle predicate made them visible.
The predicate is correct; the branch that consumes it is not.

End to end on `GB7BJ4PBLFBYBUJGPMEKRHIWPZC6HNEYF2GHE7NEEFGJHLFWRT2VD3RD`
(`AccountEntry` absent on chain), from the local API against production:

```
deleted: true, signing: null,
balances: [ type 3 "Pool Share Token", balance "0", ledger 57054801 ]
```

`deleted` is already `true` and already correct — but `hasLiveHoldings` is
tested FIRST, so a closed account is announced as an indexing gap.

### The rule the branch should encode

A **classic trustline** cannot exist without an `AccountEntry`; a **Soroban
balance** can. So the tripwire belongs on classic holdings only:

1. live holdings of `asset_type` 0 or 1 and no signing → genuine gap, warn
2. `deleted` → "Closed"
3. otherwise → no account at this address (it may still hold contract tokens)

Measured today, case 1 is **0** across all eight slices — the tripwire keeps
its meaning and stops firing on 10,713 accounts where it is simply wrong.

### The 3.7M accounts without entry state are the historical tail, not a gap

`accounts` holds 14,596,522 distinct addresses; `account_entry_state` holds
10,883,758. The 3.71M difference was worth decomposing rather than asserting.

Sampled at 1/64 (58,042 dead accounts in the slice), scaled ×64:

| bucket                                     | slice  | ×64       | share |
| ------------------------------------------ | ------ | --------- | ----- |
| closed account, sequence known             | 46,661 | 2,986,304 | 80.4% |
| closed account, sequence never captured    | 6,174  | 395,136   | 10.6% |
| never had a native balance row at all      | 5,207  | 333,248   | 9.0%  |
| **open native balance and no entry state** | **0**  | **0**     | —     |

The last line is the one that matters: **no account has a live native balance
without signing state.** That is the shape a coverage gap would take, and it
is empty — so `deleted` cannot be fooled by a missing entry-state row.

Chain probe of each bucket separately (`LedgerKey::Account`):

| bucket                                | result            |
| ------------------------------------- | ----------------- |
| closed, sequence known                | 60 / 60 ABSENT    |
| closed, sequence never captured       | 60 / 60 ABSENT    |
| never any native row                  | 60 / 60 ABSENT    |
| control — accounts _with_ entry state | 100 / 100 PRESENT |

Coverage in the other direction was already proven by the seed itself: at the
checkpoint, `account_entry_state` equalled the snapshot's live-account count
**exactly** (10,872,679, line 1414 above). Today's 10,883,758 is that plus
11,079 accounts created since.

So the 3.71M is what an explorer is supposed to keep — addresses whose accounts
are gone, and addresses that were referenced but never funded. Sequence number
0 on a closed account means the account was created and merged before our
history floor, so no transaction of its own was ever seen; it does not mean the
account never existed.

Of the never-funded addresses, 7 in 5,208 are asset issuers. Where the rest
were first seen is **unmeasured** — `transaction_participants` is 10.7B rows and
the answer changes no decision here.

### Correction: merged accounts keeping their `account_entry_state` row is DESIGN, not defect (2026-08-26)

The earlier framing ("~11,900 stale rows, +7k/day, needs a lifecycle") was
wrong twice, and the second look reverses the verdict.

**It is the writer's documented intent.** `stage.rs` emits no entry-state row
on `account_removed` — "the page gates on `deleted`, and a merge cannot change
signers." So the table's semantics are LAST KNOWN configuration; liveness
lives in the native balance's lifecycle column, exactly like `accounts` keeps
merged accounts' history. A merged account showing its final signer set is the
same feature as a merged account showing its transactions.

**The rate panic was chain churn, not our artifact.** Ground truth from
`operations_appearances` (`type = 8`) against closure stamps per window:

| window (ledgers)              | merge ops on chain | our closures |
| ----------------------------- | ------------------ | ------------ |
| 64,117,440–64,126,079 (12 h)  | 12,322             | 8,670        |
| 64,126,080–64,131,262 (7.2 h) | 23,690             | 22,062       |
| 64,131,264–64,134,437 (4.4 h) | 2,264              | 1,054        |

Merge traffic genuinely swings ~5k–80k/day (churn bots); closures track the
ops in every window (appearances include failed ops, hence ops ≥ closures).
The single out-of-band spike is the seed's own stamp (3,365,167 rows at
exactly 64,131,263; no other ledger exceeds 13) — which also answers the
artefact question: the seed writes exactly one closure value, so every
non-checkpoint stamp is the live writer's.

**Exact stale-row counts** (dead account AND an entry-state row): 11,587 =
11,546 writer-stamped + 378 at the checkpoint value (the seed/writer
same-ledger bucket — owned by the snapshot-review session). Post-checkpoint:
1,054 of 1,054 merges kept their row — post-seed the ratio is 100% by
construction, since the seed wrote state for every account alive at checkpoint.

**Growth is trivial**: 10.92M rows, 220 MiB compressed (~21 B/row) — stale
rows accrue at the merge rate, i.e. single-digit MB/day for the whole table.

**The one real residual**: an aggregate over `account_entry_state` alone
("how many accounts are multisig?") silently counts dead accounts, and the
share grows. Resolution decided: state the last-known semantics in
`database-schema-overview.md` (same doc pass that fixes its "live holdings"
warning sentence to "classic holdings") — no schema change, no task.

### Second correction, and the writer verified against the chain (2026-08-26, later)

Two of the numbers above were produced WITHOUT RMT dedup (the exact trap
`project_rmt_unmerged_dedup_on_read` warns about) and are hereby replaced:

- dead accounts with a signers row: **11,639** (was 11,587), of which
  **11,636** carry a real writer stamp and **3** the checkpoint value.
- the "378 at the checkpoint stamp" **does not exist** — it was old closure
  VERSIONS of accounts later recreated; `argMax` collapses them away. No
  same-ledger mystery bucket on this side; 3 accounts merged in/around the
  checkpoint ledger itself, consistent with ≤13 merges on any other ledger.

The earlier "ops ≥ closures because appearances include failures" guess was
also wrong: in the post-checkpoint window only **5 of 2,264** merge ops
failed. The real explanation, measured: **2,259 successful merges collapse
onto 1,055 unique accounts** — churn bots merge and re-create the same
addresses, ~2.1 merges per account in the window. `coalesce(op.source_id,
tx.source_id)` identifies the merged account (2,175 of 2,259 type-8
appearances carry NULL `source_id` — task 0516's gap, now quantified);
against our lifecycle stamps: **1,048+ stamped closed, and every account
found unstamped probed PRESENT on chain** — recreated and alive, stamp 0
correct. Verdict: **the live writer misses nothing measurable.**

Decisions taken: D6 = leave the deleted-account Signers card as-is (the
header's `Deleted` badge is the context; the card is history, like the
transaction list). D7 = the last-known semantics gets one sentence in
`database-schema-overview.md`, no schema change, no task. D9 = keep the
tripwire branch, no monitoring infra.

### B3 resolved: the LP write path works; only the historical pass is missing

`lp_positions` looked lifeless — 108,718 live rows against the network's 77,048
at checkpoint, and a 40-row chain probe came back 16 PRESENT / 24 ABSENT. That
reading was incomplete. Split by the writer's deploy ledger (64,115,052):

| cohort                             | result                               |
| ---------------------------------- | ------------------------------------ |
| positions touched AFTER the deploy | **30 / 30 PRESENT on chain**         |
| all closure stamps in the table    | 44, every one at ledger ≥ 64,115,290 |
| positions touched since deploy     | 340, of which 44 closed (13%)        |

`stage.rs:1101` stamps `closed_at_ledger: if pos.closed { last } else { 0 }` —
the ADR 0055 write path this task carries, and it is correct. Every ABSENT row
in the earlier sample predates the deploy, which is exactly the cohort the seed
skipped on purpose ("Pool-share trustlines — they live in `lp_positions` until
the ADR 0056 merge lands").

So B3 is not a defect in the writer: it is the **historical backfill for pool
shares, still outstanding**, the same shape the seed fixed for classic/native.
Per task 0493's split note the LP write path belongs to 0463; the backfill pass
re-derives its snapshot from `manifest.json` (the archive is content-addressed),
which is why that artifact was kept.

### Decisions closed 2026-08-26 (signers card)

| #   | Question                                | Chosen                                      | Reason kept for the next reader                                                                            |
| --- | --------------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| D1  | what the tripwire reads                 | classic holdings only                       | a classic trustline cannot exist without an `AccountEntry`; a Soroban balance can                          |
| D3  | no-account card wording                 | say why tokens without an account is normal | 1,325 addresses are in that state; the balance is on screen right above                                    |
| D4  | Soroban holdings on a closed account    | leave them                                  | 60/60 match the chain exactly; TTL/archival is tasks 0435/0436                                             |
| D6  | closed account WITH a stale signers row | leave the card as-is                        | the header's `Deleted` badge is the context; the card is history, like the transaction list                |
| D7  | `account_entry_state` liveness          | one sentence in the schema doc              | last-known is the design, not a defect; no schema change, no task                                          |
| D9  | tripwire vs monitoring                  | keep the branch, no monitoring              | a fixture-driven test cannot see production; the branch is the reader's signal that the list is unreliable |

D2 (where to compute the flag) collapsed into D1 — both flags are one `.some()`
in the page over data it already holds, so no API change and no regenerated
types.

**Rejected**: reordering `deleted` ahead of the tripwire. It reads as "fact
beats inference", but the branch is an ALARM: a closed account with a live
classic trustline is a real data defect, and putting `deleted` first silences
the one contradiction the branch can still catch.
