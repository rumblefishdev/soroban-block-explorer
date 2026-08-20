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

Signers take a side table `account_signers` (single writer, RMT by ledger) —
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
   cross-checked against the RPC route regardless of the result** — a standing
   requirement, not a nicety.
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
   thresholds into `account_signers`. This is NOT deployed yet.
2. **Checkpoint-snapshot toolchain** (`backfill-runner` subcommands, all
   read-only except the seed's explicit `--execute`):
   - `snapshot-tally` — decode the archive's full-state snapshot, count records
     (4.44 GB, 21 buckets, ~6 min, 13.5 MB peak RSS after the streaming fix).
   - `snapshot-dedup` — first-wins per key → DISTINCT entries. The bucket list
     is newest-first; the first record per key is the live one.
   - `snapshot-compare` — four-way diff of our `balances` against the network:
     missing / closure / ghost (ours >0, network gone) / divergent / stale,
     classic and native separately, with stride samples and a below/above-floor
     histogram of the missing bucket.
   - `snapshot-verify` — spot-check any sample against Soroban RPC raw XDR
     (`getLedgerEntries`); absence from the response = entry does not exist.
   - `snapshot-seed` — builds ALL corrections; dry-run writes artifacts only
     (`manifest.json`, `summary.txt`, `ghosts.tsv`), `--execute` inserts.

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
4. Coverage measurement + RPC cross-check (standing requirement), the
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

One defect blocked deploy and is fixed: **`account_signers` rows were not
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
`account_signers` had (two states in one ledger → two rows at the same RMT
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

### Open follow-ups spawned along the way

- 0502 (reusable snapshot decoder — extract `snapshot.rs` from
  backfill-runner into its own crate; the seed stays a backfill-runner
  consumer), 0503 (exhaustive audit), 0504 (five discarded entry types),
  0497 (retire repair-tier1), 0498/0499 (LP merge), 0492 (provenance).
- Lore id collisions present ON DEVELOP (pre-existing, not from this branch):
  two `0496_*` tasks, two `0054_*` ADRs.

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
      cross-checked against the RPC route regardless of the measured result
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
