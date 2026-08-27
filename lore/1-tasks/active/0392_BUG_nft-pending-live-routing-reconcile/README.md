---
id: '0392'
title: 'NFT completeness umbrella: no row silently missing (promote gap + parser/classifier gates)'
type: BUG
status: active
related_adr: []
related_tasks: ['0512', '0309', '0391', '0283', '0217', '0306', '0296', '0425']
tags: [priority-high, effort-medium, layer-indexer, layer-db, nft, clickhouse]
links: []
history:
  - date: 2026-08-21
    status: active
    who: karolkow
    note: >
      **Restarted as an umbrella; scope decided from the re-measurement.** The
      task keeps the outcome — no NFT row silently missing — and hands the
      classifier + parser execution to 0512 (renumbered from 0317, which
      collided with the archived `0317_BUG_contracts-events-ch-memory-limit`).
      Three gaps, in dependency order: the parser cannot shape bespoke ABIs
      (66 contracts / 692 events / zero rows anywhere), the classifier stamps
      bespoke NFTs `Other` (67 contracts / 278 quarantined rows), and nothing
      promotes when a verdict resolves (0 rows today — *created* by fixing the
      second). Gap 3 is this task's own code and is deliberately **gated behind
      0512**: its acceptance criterion is "observed promoting a real contract",
      and until 0512 flips verdicts no such contract exists. Building it early
      ships an untestable mechanism.
      Two further findings recorded while scoping. `extract_event_signature`
      (`persist/stage.rs:2137`) only reads a Symbol at topic 0, so
      namespace-first topics (`["BadgeNFT", sym:"init"]`) and String-typed event
      names (`["Mint"]`) yield a NULL signature — 470 events across 5 contracts,
      invisible to any monitoring keyed on event name. And 0309 defers tactical
      work to `0308`, **which does not exist**; 0512 is the real home, so that
      dangling link is repointed.
  - date: 2026-08-21
    status: active
    who: karolkow
    note: >
      **Re-verified a month on — every checkable claim in this task still holds;
      only the scale shrank.**
      Code, on `origin/develop` (`eb7e817c`): the promote half is still absent.
      `promote_or_count` exists solely inside
      `crates/backfill-runner/src/nft_reclassify.rs`, and that file is still
      present — the only drain is still a human. `route_for` is unchanged in
      behaviour (`persist/stage.rs:1648`; the body cites 1444, the line drifted,
      the semantics did not): `Token|Fungible -> Drop`, `Nft -> Hot`, everything
      else -> `Pending`.
      Prod, measured 2026-08-21 (chain tip 64,054,678): `nfts_pending` holds
      **278 rows across 67 contracts** (274 / 66 a month ago). A whole month's
      intake is **+4 rows, +1 contract** — against ~6,575 rows/day before PR #341.
      The last pending write was ledger 63,836,382, **~12.6 days ago**. Hot `nfts`
      is at 64,054,630, **48 ledgers off the tip (~4 min)**, holding 13,326 rows
      over those same 67 contracts. The frozen-surface failure is gone.
      The decisive split is unchanged from last month: across every row of
      `soroban_contracts` for those 67 contracts, duplicates included, the only
      `contract_type` ever stamped is `1` — `groupUniqArray` returns `[1]`.
      **0 carry a decisive verdict, 0 resolved-but-stranded.** Enum, for the
      record, since the ordering is not the obvious one:
      `Token = 0, Other = 1, Nft = 2, Fungible = 3`.
      So the defect is real and unfixed, but it is now a ~4-rows-a-month leak
      with an empty resolved queue — a different problem from the one this task
      was written for. The urgency numbers still quoted in the body (33 days
      frozen, 6,575 rows/day) remain stale. Restarted from scratch on
      `fix/0392_nft-pending-drain-gap`.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Taken over. Steps 2 and 3 re-verified as genuinely done; Step 1 confirmed
      necessary and is now the whole remaining task.**
      **Step 2 — done and live.** The `Option<i16>` fix (PR #341, `9cb3834e`) is on
      `origin/develop`, not on `origin/master` — but `master` last moved 2026-07-03
      and `develop` is **448 commits ahead**, so `master` is not the deploy source.
      Effectiveness proven from data instead of from branches: the last pending
      drain ran **2026-07-16 15:58:58** (`system.mutations`, `DELETE WHERE
      contract_id IN (… contract_type IN (0,2,3))`), and in the five days since,
      `nfts_pending` has received **nothing** — 274 rows total, newest at ledger
      63,386,630. With G9 still broken it would have taken ~6,575 rows/day, i.e.
      ~33,000 rows. So verdicts do resolve at write time now.
      **Step 3 — done and clean.** Of the distinct contracts in hot `nfts`, **all 66
      carry verdict `Nft` (2)** — zero `Fungible`/`Token` contamination survived the
      drain. The quarantine holds 66 contracts, all `Other`/NULL, which is a
      correctly-behaving quarantine.
      **Step 1 — still absent, and now proven so by code rather than inferred.**
      `persist/rows.rs:226-230` says promotion happens "via the post-backfill drain
      runbook — CH has no per-row UPDATE / `WHERE NOT EXISTS` equivalent to PG's
      in-tx `promote_pending_nfts_to_hot` step", and that function exists nowhere in
      the codebase (Postgres, retired in 0244). Nothing moves a contract out of
      quarantine once its verdict resolves except a human.
      Unrelated finding worth carrying out of this check: the API code reading
      `operation_asset_appearances.net_settled` is on `develop` but the column does
      not exist on prod. It is **not currently erroring** — 72h of `system.query_log`
      shows zero occurrences beyond my own probe — because the API has not been
      redeployed since that code landed. 0419 owns the `ALTER`; deploying the API
      before it runs gives `Code 47` on that endpoint.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Settled by code, not by the single measurement — the defect is dormant, not
      gone.** The re-measurement above shows an empty quarantine, which on its own
      cannot distinguish "problem fixed" from "problem currently idle". The code
      distinguishes it: `persist/rows.rs:226-230` states that pending rows are
      promoted "via the post-backfill drain runbook — **CH has no per-row UPDATE /
      `WHERE NOT EXISTS` equivalent to PG's in-tx `promote_pending_nfts_to_hot`
      step**". Grepped it: `promote_pending_nfts_to_hot` **exists nowhere in the
      codebase** — it was a Postgres function, and Postgres was retired in 0244.
      So the live path has **zero** post-hoc promotion, and the only mechanism that
      moves a resolved contract out of quarantine is a human running
      `nft-reclassify`.
      That makes the gap arithmetic rather than speculation. `route_for` deliberately
      defers a contract whose WASM has not been observed yet — this task's own §4f
      measured that as 61% of pending, "correct defer", not a leak. Nothing
      un-defers them. Every contract whose WASM is observed after its first NFT
      event strands its rows permanently until someone drains by hand. Today's
      "0 resolved-but-stranded" means 0306's drain cleared the backlog 11 days ago
      and none of the 66 residents has resolved since — not that resolution now
      promotes.
      **Step 1 stands.** What is stale is the urgency framing (hot frozen 33 days,
      6,575 rows/day), not the defect.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      Scope pinned: **`nft-reclassify` is deleted either way**, but only after the
      replacement is verified working — not before. Two acceptable end states, and
      leaving the subcommand standing is neither: (1) Step 1's continuous reconcile
      lands and is observed promoting a real contract, then the subcommand goes; or
      (2) a cheaper monitor lands — alert when a pending contract has carried a
      resolved verdict for more than N ledgers — and the subcommand goes with the
      alert as the safety net. Deleting it before either exists would remove the
      only working drain. Keeping it after either exists re-creates the ownerless
      mop this task was spawned to end (lore 0425 clause 4).
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Re-measured — the premise no longer holds.** Six days after PR #341 landed,
      the numbers this task was built on have inverted. Then (2026-07-15): hot `nfts`
      frozen at ledger 62,989,407 since 2026-06-12 (33 days), ~6,575 pending rows/day
      at 91% fungible false-positive, 401 fungible-verdict and 21 Nft-verdict
      contracts stranded in quarantine. Now (2026-07-21, chain tip 63,583,789):
      hot `nfts` at **63,569,710** — it moved 580,303 ledgers and tracks the tip to
      within ~19h; `nfts_pending` holds **274 rows across 66 contracts**, last
      written 63,386,630.
      The decisive number is the verdict split of those 66 pending contracts:
      **0 with an `Nft` verdict, 0 with `Fungible`, 66 `Other`/NULL.** Not one
      resolved-but-stranded row — which is precisely what Step 1's continuous
      reconcile exists to drain. There is currently nothing to reconcile: the
      quarantine holds only genuinely-unclassifiable contracts, which is the design
      working as intended.
      What changed: PR #341 fixed the G9 prefetch (it was a 100% mechanical no-op),
      so verdicts now resolve at write time and contracts route straight to hot;
      0306's drain cleared the accumulated backlog. Step 2 is done, Step 3 is done.
      **Step 1 needs re-justification before anyone starts it** — either the drain
      gap reopens under some condition worth naming, or this task closes and the
      residual 66 unclassifiable contracts belong to 0317 (launchpad-NFT
      discriminator + monitored-UNKNOWN), not here.
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Spawned from 0391 §"Why *_pending grows unbounded" + R §4. Two sub-bugs,
      one shared root (write-time verdict resolution). Measured: hot frozen 33
      days, live path writes ~6,575 pending rows/day @ 91% fungible false-pos.
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Corrected after devil's-advocate crux test (0391 §4f). The "write-time
      fail-open leak" framing was overclaimed — of fungible pending rows with
      known WASM timing, 61% are correct defer (WASM seen at/after event), not a
      leak. Reordered: continuous reconcile is now the PRIMARY fix (Step 1);
      write-time tightening demoted to SECONDARY, gated on measuring the prefetch
      miss-rate. Reconcile gap + 33-day drain-staleness remain proven.
  - date: 2026-07-15
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active. Step 2 gate resolved by direct measurement same day:
      G9 prefetch was a 100% mechanical no-op (Nullable(Int16)-as-i16, ch0.15
      wire-type check; 20,494 failures/7d on prod) — see
      notes/R-g9-prefetch-miss-rate-measured.md. Fix + red/green e2e in PR #341
      (also unbreaks the 0320 prior-row prefetch, stale `name` column).
      Consistent with the §4f correction: fix stops only the H1 slice; Step 1
      reconcile remains primary. Steps 1 + 3 remain.
---

# NFT completeness: no row silently missing

## Summary

**Restarted 2026-08-21.** This task was written as "build a continuous
promote/reconcile for `nfts_pending`". Re-measurement (see _Restart
measurements_ below) showed that mechanism would move **zero rows today**, while
a larger defect next door loses data silently. The task is now the umbrella for
one outcome:

> An NFT row that should exist must exist, and anything we cannot classify must
> be **visibly** unknown — never silently absent.

0392 owns that outcome and the sequencing. It does not hold all the code:
the classifier and parser work is executed in
[0512](../../active/0512_FEATURE_classifier-monitored-unknown-and-launchpad-nft.md)
(renumbered from 0317).

## The three gaps, in dependency order

| #   | Gap                                                                  | Population today                                 | Executed in |
| --- | -------------------------------------------------------------------- | ------------------------------------------------ | ----------- |
| 1   | Parser cannot shape bespoke ABIs, and drops without a trace          | 66 contracts, 692 events, **zero rows anywhere** | 0512        |
| 2   | Classifier stamps bespoke NFTs `Other`                               | 67 contracts, 278 quarantined rows               | 0512        |
| 3   | Nothing promotes a contract's pending rows when its verdict resolves | **0 today — created by gap 2 being fixed**       | 0392        |

Gap 3 is empty _because_ gaps 1 and 2 are open. Fix the classifier and ~66
contracts flip `Other → Nft`; at that moment 278 rows need promoting and the only
mechanism is `backfill-runner nft-reclassify`, a human-run subcommand that
[0425](../../archive/0425_REFACTOR_delete-spent-one-off-backfill-subcommands.md)
exists to delete. Landing 0512 without 0392 swaps a silent-miss defect for a
stale-data defect.

## Sequencing

1. **0512 first.** It supplies both the fix and — for the first time — real
   contracts whose verdict actually flips.
2. **0392's own code after it.** The promote mechanism is deliberately _not_
   built ahead of 0512: its acceptance criterion is "observed promoting a real
   contract", and until 0512 lands there is no such contract to observe. Building
   it early means shipping an untestable mechanism and calling it done.
3. **`nft-reclassify` deleted last**, once the replacement has been watched
   doing its job.

## What is already settled (do not redo)

- **Step 2 — done, shipped.** The G9 prefetch was a 100% mechanical no-op
  (`contract_type` read as bare `i16` against `Nullable(Int16)`, rejected by
  clickhouse 0.15). One-line `Option<i16>` fix in PR #341. Verdicts resolve at
  write time now; hot `nfts` tracks the chain tip to within ~4 minutes, and
  quarantine intake fell from ~6,575 rows/day to ~7 rows/month.
- **Step 3 — done.** 0306's drain cleared the accumulated backlog.
- **`route_for` semantics** (`persist/stage.rs:1648`): `Token|Fungible → Drop`,
  `Nft → Hot`, everything else → `Pending`. Unchanged and correct as far as it
  goes.

## Constraints on any design here

- **"If a row is in `nfts`, it should be an NFT."** A read-time visibility
  filter over a polluted table was tried and rejected on review. Membership is
  decided before the write.
- **ADR 0046** (`classifier-quarantine-tables-nfts-pending`) is live and
  **unsuperseded** on develop. It is the standing decision behind the quarantine;
  honour it or supersede it deliberately.
- **ADR id 0053 is taken** on develop by `fast-change-offchain-compute-at-read`.
  Any ADR spawned here needs a free number.
- Do **not** mirror `nft_reclassify`'s `ALTER … DELETE` on the live path — it
  treats the symptom and races the ingest inserts.
- A genuinely-unknown contract must still be able to quarantine. `Fungible` and
  NFT `transfer` events are byte-identical in shape (`from,to,i128` vs
  `from,to,token_id`); only WASM classification separates them. The goal is not
  a zero quarantine, it is a quarantine that holds only the genuinely unresolved
  — and says so out loud.

## Acceptance Criteria

- [ ] (gap 3, this task) When a contract's verdict resolves to `Nft`, its pending
      rows promote to hot without a manual `nft-reclassify` run; `Fungible|Token`
      rows drop. Scoped to the contract, never a full-table sweep.
- [ ] (gap 3) **Observed promoting a real contract** — not inferred from an empty
      quarantine. 0512 supplies the contract.
- [ ] No contract carrying a decisive `Nft` verdict holds zero rows while emitting
      NFT events. _(Replaces the old "hot max ledger tracks the tip" check, which
      PR #341 already satisfies and which never covered the parser gate.)_
- [ ] Anything the parser or classifier cannot resolve is **counted and visible**,
      not dropped silently. A NULL `signature` must not be indistinguishable from
      "no event".
- [ ] **`nft-reclassify` is deleted** — together with its `docs/backfills.md` row
      and its `crates/backfill-runner/README.md` entry — _after_ the replacement
      has been observed working. Per 0425 clause 4.
- [ ] **Docs updated** — `docs/architecture/**` ingestion-pipeline + XDR-parsing
      sections describe the routing and the promote step (ADR 0032).
- [ ] **API types regenerated** — N/A unless the work touches `crates/api/**`;
      this is `crates/db-clickhouse` + `crates/xdr-parser` + `crates/indexer`.

### Dropped acceptance criteria

- ~~"Genuinely-unresolved contracts (WASM never observed) still quarantine
  correctly"~~ — **describes an empty set.** All 67 quarantined contracts have an
  observed WASM. See F1/F2.

## Notes

- 0217 (archived) built the quarantine; 0306 was the one-shot prod drain.
- 0512 (was 0317) is the classifier + parser execution task.
- 0309 is the strategic redesign (total function, monitored UNKNOWN, SEP-46/47/48).
  It defers tactical work to `0308`, **which does not exist** — that link dangles,
  and 0512 is the real home.
- **Reference only, nothing extracted:** branch
  `fix/0392_nft-pending-live-routing-reconcile` (`c57bd7e8`) and PR #358, closed
  unmerged 2026-08-21. Its measurements are ours, a month old, and were
  re-derived from scratch rather than copied.

## Restart measurements — 2026-08-21

Re-derived from scratch against prod ClickHouse and, where it matters, against
the chain via the official `stellar` CLI 26.0.0. Nothing here is carried over
from earlier sessions; figures that disagree with the body above supersede it.

### F1 — the quarantine holds nobody this task can drain

Of the 67 contracts in `nfts_pending`, split by when their WASM was uploaded
relative to their first pending row:

| Bucket                                                      | Contracts | Rows |
| ----------------------------------------------------------- | --------- | ---- |
| A — WASM never observed                                     | **0**     | 0    |
| B — WASM known _before_ the first event                     | **66**    | 274  |
| C — WASM arrived _after_ the first event (late, strandable) | **1**     | 4    |

Bucket B is the whole quarantine. For those 66 the verdict _was_ available at
write time — `route_for` asked, and the answer was `Other`. They are not
deferred-for-timing; they are classifier misses. A reconcile cannot move them:
their verdict is already computed and it is not decisive.

Bucket C — one contract, 4 rows — is the entire population Step 1 was designed
for. Even it would not promote, because its verdict is also `Other`.

**So Step 1, implemented exactly as written, would move zero rows today.**

### F2 — an acceptance criterion describes an empty set

The AC _"genuinely-unresolved contracts (WASM never observed) still quarantine
correctly"_ has no members: bucket A is 0. The stated design intent of the
quarantine — hold contracts whose WASM has not been seen — is not what the
quarantine actually contains.

### F3 — the larger defect is next door, and it is silent

Contracts carrying a decisive `Nft` verdict, measured against every NFT table:

|                                       | Contracts | Events  |
| ------------------------------------- | --------- | ------- |
| have rows in `nfts` + `nft_ownership` | 67        | 25,110  |
| **have rows nowhere at all**          | **66**    | **692** |

Those 66 are not quarantined. They are absent — no hot row, no ownership row,
no pending row. The quarantine sits at the _classifier_ gate only; nothing
guards the _parser_ gate, so a known-NFT contract whose events the parser cannot
shape simply produces nothing, with no record that anything was dropped.

### F4 — cause of F3, confirmed on-chain not from our tables

`stellar contract info interface --network mainnet` on two of the 66, reading
the deployed WASM:

- `CB2SIYGHFGQMKEYQUWCTF3HCWBCPFUSRGVWXOPV3LIJR7K5LRPFXZEYK` —
  `transfer(env, domain: String, from: Address, to: Address)` and
  `owner_of(domain: String)`. Token identity is a **String**, and it is the
  **first** argument. Canonical is `(from, to, token_id)`. The shape cannot
  match, so every transfer is dropped. This contract emits a perfectly
  canonical `transfer` event name — name matching alone would not have saved it.
- `CBT5JMDOUAU3BJF7YZR42LVODLMZSQE4LIJUJNUBKEC2VZOXIF4JFBRU` —
  `owner_of(token_id: u32)`, `transfer`, `mint_badge`, `total_supply`.
  Unambiguously an NFT; mints via `mint_badge`, emits `init` / `minted`.

Event signatures across the 66, by volume: `(null)` 470 (5 contracts),
`minted` 63, `uri_upd` 58, `mint` 46, plus `identity_minted`, `mint_event`,
`transfer_event`, `set_uri_event`, `approval_for_transfer_event`. The 470
undecoded signatures are the single largest slice and are not yet explained.

### F5 — current intake

Two contracts wrote pending rows in the last month:
`CAHDANGTQY4TOXV7LYYXRTFIUKJKJUXWNRUMKRPBJG2QOGSATYTRX6HP` (5 rows, ledgers
63,636,969–63,836,382) and
`CBGMSY35IZMHNVBFQQY22PA62VWJVXIKC4TU2CTAKRGIJOACZE4EEIWW` (2 rows). The net
count moved 274 → 278; the prior 274/66 figure comes from an earlier session and
was not re-verified, so treat the delta as approximate. What is solid: two
contracts, single-digit rows, one month.

### F6 — measurement trap for whoever works this next

`soroban_contracts` is RMT and unmerged. `CAHDANG…` carries two disagreeing
rows — a pre-WASM placeholder (`contract_type` NULL, `wasm_uploaded_at_ledger` 0) and a later stamp (`63,207,509`, `contract_type` 1). Any join to
`soroban_contracts` without deduping fans out and double-counts; it inflated one
of my own intermediate counts 5 → 10 before I caught it. Dedup with
`argMax(contract_type, wasm_uploaded_at_ledger)` grouped by `id`.

### What this implies for the restart

The drain gap is real as a mechanism and unfixed in code, but its addressable
population is one contract and four rows, none of which would move. The defect
that is actually costing data is the parser's ABI coverage (F3/F4), which is
0309 / 0317 territory, not this task's. That is the input to the design
decision, not the decision itself.
