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

| finding                                            | count                                 | note                                                                                                                                                                                                                                            |
| -------------------------------------------------- | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| live trustlines the network has, we hold NO row    | **19,290,231**                        | the real gap is **60%**, not the ~7% the 200-account probe suggested — that probe sampled accounts WE hold; dormant-since-floor lines were invisible to it. 99.997% have `lastModifiedLedgerSeq` below our floor: dormancy, not a parser defect |
| our zero rows that are REAL closures               | **24,474,026**                        | the ghost set the read flip must not resurrect                                                                                                                                                                                                  |
| native ghosts (account deleted, we still show XLM) | **1,035,265** carrying **45.51M XLM** | task 0321's class, 17× its own measurement — its method only saw merges we ingested; RPC check: 100/100 sampled accounts absent on chain                                                                                                        |
| assets we have never seen at all                   | **97,139**                            | network has 391,092 live assets, we know 344,989, intersection ~294k (we also hold ~51k dead ones)                                                                                                                                              |
| signers rows ready to seed                         | **10,865,408**                        | every live account, versioned on each entry's own ledger                                                                                                                                                                                        |

Asymmetry explained: the RPC bootstrap (task 0492) seeded accounts + native
balances, never trustlines — so native measures complete while classic misses
19.3M.

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

### Findings 2026-08-18 (second sweep — Karol's "omit nothing" pass)

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
- [ ] `total_supply` and `holder_count` unchanged for a spot-checked asset
- [ ] The 200-account probe from `notes/R-` returns zero accounts where the
      chain holds more live zero trustlines than we do
- [ ] **Verified on production**, not at merge — the destination is a shipped,
      checked change
- [ ] **Docs updated** — `docs/architecture/**` read path and frontend data
      contract; `docs/backfills.md` gains the seed pass
- [ ] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)
