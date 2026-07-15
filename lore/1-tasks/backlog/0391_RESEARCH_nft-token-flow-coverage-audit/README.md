---
id: '0391'
title: 'NFT token-flow coverage audit (0383 follow-up) — measure NFT parity, close gaps'
type: RESEARCH
status: backlog
related_adr: []
related_tasks:
  [
    '0383',
    '0392',
    '0309',
    '0217',
    '0306',
    '0296',
    '0283',
    '0320',
    '0316',
    '0359',
  ]
tags:
  ['phase-future', 'effort-small', 'priority-medium', 'nft', 'coverage-audit']
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Spawned from 0383 (Soroban event token-flow decode) future work. Audit
      whether NFT movements get the same account-page + collection-page coverage
      0383 gave fungibles. Measured prod CH; findings in notes/R + notes/S.
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Follow-up measurement (R §4). Hot frozen 33 days at ledger 62,989,407
      (2026-06-12, last nft-reclassify); live path writes ~6,575 pending
      rows/day, 91% Fungible-verdict false-positives (401/401 confirmed real
      fungible assets); 21 Nft-verdict collections / 559 tokens stranded (all
      already in hot → post-June mints missing). Spawned 0392 (live reconcile).
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Devil's-advocate crux test (R §4f) corrected the mechanism claim: the
      "write-time fail-open leak" is a minority (~11% overall) and unproven — of
      fungible pending rows with known WASM timing, 61% are correct defer. Fix
      reframed to continuous-reconcile primary; write-time tightening secondary,
      gated on prefetch miss-rate. Drain-staleness + reconcile gap stay proven.
---

# NFT token-flow coverage audit (0383 follow-up)

## Summary

Task **0383** (Soroban-event token-flow decode) surfaced _fungible_ token moves
(transfer / mint / burn / clawback) onto account + asset pages by writing
presence rows into `transaction_participants` (account index) and
`operation_asset_appearances` (asset index). This task audits whether the
**analogous coverage holds for NFTs** — and it does, for the account side.
The account-page participant registration is **classification-independent** and
already covers NFT movers (including the rare `consecutive_mint` batch mints).
The real gap is entirely on the **NFT collection/detail pages**, which read only
the _hot_ `nfts` / `nft_ownership` tables — so NFTs whose contract is not yet
`Nft`-classified sit invisible in the `*_pending` quarantine. That gap is
already owned by existing tasks (0217 drain, 0309 classifier, 0320/0316 WASM
observation); this audit quantifies it and confirms **no new NFT decode work is
needed** — 0383 did not leave an NFT hole on the account side.

**Update 2026-07-15:** the audit did surface **new live-flow work** (distinct
from decode). Re-measuring a day later proved the pending quarantine is not a
drained-once relic — hot has been frozen at ledger `62,989,407`
(**2026-06-12**, last manual `nft-reclassify`) for **33 days** (confirmed: new
`Nft`-verdict rows accrue continuously right up to ~6h before the chain tip yet
none reach hot), while the live path keeps writing to pending, ~91% of it
fungible-verdict false-positives. That is a real live **reconcile/drain** gap,
spawned as **[0392](../0392_BUG_nft-pending-live-routing-reconcile/README.md)**.
Note: a devil's-advocate crux test (R §4f) showed the _mechanism_ is NOT a
proven write-time fail-open leak — among rows with known WASM-observation timing
the majority are _correct defer_ (WASM seen at/after the event), so the fix is a
continuous reconcile, not a write-time routing change. Details in
[R §4](notes/R-nft-coverage-measured-state.md).

## Context

0383's devil's-advocate pass flagged four NFT questions (Q1–Q4). This task
answers each with prod measurements + code trace. Full data in
[R-nft-coverage-measured-state](notes/R-nft-coverage-measured-state.md);
gap analysis + decisions in
[S-nft-coverage-gaps-and-decisions](notes/S-nft-coverage-gaps-and-decisions.md).

## Findings (one line each — detail in notes)

- **Q1 — `consecutive_mint` gap: NOT a gap.** Only 23 events / 8 contracts
  chain-wide (negligible). 0383's `parse_token_event` does not match it, but the
  **pre-existing** NFT-owner participant path (`stage.rs:599`) registers the
  recipient anyway. Verified on a real tx: recipient IS in
  `transaction_participants`.
- **Q2 — classification undercount: REAL, but 91% is fungible false-positives.**
  `nfts_pending` holds 794 contracts / 176,604 tokens, but broken down by
  verdict: **Fungible 350c/161,559t (false positives → drop), Other 423c/14,632t
  (genuine unknowns), Nft 21c/429t (classified but unpromoted).** So the true
  NFT-page shortfall is ~21 promoted-late collections + 423 unclassifiable ones,
  not "176k tokens missing".
- **Q3 — collection page completeness:** NFT pages read _hot_ tables only →
  the 21 `Nft`-classified-but-pending collections (and their `/transfers`
  histories) are missing until promoted. Unlike 0383 (which had to _write a new
  index_), here the data already EXISTS in `*_pending`; it just needs the
  promotion drain to run.
- **Q4 — account-page NFT coverage: SOLID.** Participant registration ignores
  classification. Standard verbs get both sides (Path A = 0383 once deployed;
  Path B = owner path); `consecutive_mint` recipient gets Path B.

## Why `*_pending` grows unbounded (and why the drain must go live)

Measured surprise during this audit: `nfts_pending` / `nft_ownership_pending`
are **not** a drained-once relic — they fill continuously, right up to the
chain tip (pending max ledger `63,474,129` vs tip `63,474,130`, 2026-07-14).
The last 500k ledgers alone added **213,083 rows across 183 contracts**, and the
top recent contributors are **all `Fungible`-classified** false-positives
(`CCSNFZ5R…` 56,706 rows, `CBIJBDNZ…` 43,960, …). Root cause chain:

1. **Pending is a by-design "don't-know-yet" sink.** `route_for`
   (`stage.rs:1411-1423`): `Nft→Hot`, `Fungible|Token→Drop`,
   **`Other|NULL|uncached→Pending`**. Any contract whose WASM interface is not
   yet classified at the moment its (NFT-shaped) event is ingested MUST land in
   pending. This cannot be zero — you don't know a contract's type before you
   see its WASM.
2. **The live pipeline only ever INSERTs into pending** (`writer.rs:279-280,341`
   — no DELETE/promote on the ingest path). So pending only grows on the hot
   path.
3. **The ingest-time classification prefetch (0283) only narrows inflow, never
   stops it.** G1/G9 (`persist.rs:225,394`) resolve cross-ledger verdicts but
   return only `Nft`/`Fungible`; `Other`/NULL/unobserved are skipped and
   re-resolve to Pending every time. 0283's own comments defer the rest to "a
   batch backstop drain later."
4. **0296 _increased_ inflow.** Recovering the packed/map/`consecutive_mint`
   NFT event shapes the old parser silently dropped moves those events from
   _dropped_ → _`nfts_pending`_ — and because the shape parser can't tell an
   NFT `token_id` from a fungible `i128` amount, high-volume fungible/DeFi
   contracts flood pending with amount-as-token_id rows (seen as negative
   `token_id` e.g. `-10000000`).
5. **The only drainer is a manual one-shot.** `backfill-runner nft-reclassify`
   (`nft_reclassify.rs`) promotes `Nft` pending→hot and DELETEs
   `Fungible|Token` stale rows — but it is a manual CLI subcommand with **no
   cron / EventBridge / Lambda / schedule wiring anywhere**. Runbook 0217 is
   explicitly "once per environment." The consolidating prod run that would
   drain (**task 0306** `0306_OPS_nft-surfacing-enrichment-prod-pipeline`,
   reparse → rebuild → **reclassify** → enrich) is **still backlog, never
   executed on prod**.
6. **A `Fungible`-in-pending row is expected staleness, not a routing bug:**
   ingest-time verdict was `Other`/NULL → pending; WASM later observed → contract
   flips to `Fungible`; the `Fungible→Drop` rule only applies to _new_ ingests,
   never retroactively — and the drain that would delete the old rows hasn't run.
7. **One genuine residual bug:** un-deployed SACs mislabeled `is_sac=false` land
   at `contract_type = NULL`; `nft-reclassify` drains only `Token|Nft|Fungible`,
   so NULL-verdict pollution is **neither promoted nor dropped, ever** (tasks
   0221 / 0294 / 0323).

**Design conclusion (this task's recommendation):** the promote+drop that
`nft-reclassify` does as a one-shot **should run continuously on the live path**,
not only as an operator backfill. **Primary fix — continuous reconcile:** when a
contract's verdict resolves to `Nft`/`Fungible`/`Token`, promote/drop its pending
rows (event-driven per newly-classified contract, not a full-table sweep). This
covers the dominant case, because the crux test (R §4f) shows most fungible
pending arrived as _correct defer_ (WASM not yet observed at ingest), which no
write-time change can prevent. **Secondary / optional — write-time tightening:** a
verdict-authoritative lookup would `Drop` the ~11–39% (H1) whose verdict was
already computable but the prefetch missed; gate this on first measuring the 0283
prefetch miss-rate. A live mirror of the backfill `ALTER … DELETE` would only
treat the symptom. **Spawned as [0392](../0392_BUG_nft-pending-live-routing-reconcile/README.md).**
See [R §4](notes/R-nft-coverage-measured-state.md) for the 2026-07-15 numbers
(drain 33 days stale, proven; leak-vs-defer mechanism unresolved).

## Acceptance Criteria

- [x] Measured hot vs pending NFT table sizes + classification breakdown (prod CH)
- [x] Traced both `transaction_participants` NFT write paths (Path A / Path B)
- [x] Confirmed which tables the NFT read endpoints seek (hot only)
- [x] Verified `consecutive_mint` recipient reaches `transaction_participants`
- [x] Concrete gap list mapped to owning tasks (below)
- [ ] **Docs updated** — N/A (audit only; no system-shape change)
- [ ] **API types regenerated** — N/A (no `crates/api/**` change)

## Future Work (gaps → owning tasks; no new decode work)

1. **Promotion-lag drain + make it LIVE (primary action).** Run the
   `nfts_pending` → `nfts` drain (`nft_reclassify`, per **task 0217** runbook
   `docs/runbooks/0217_nfts_pending_migration_and_drain.md`; the consolidating
   prod run is **task 0306**): promotes the 21 `Nft`-classified collections (429
   tokens) onto the pages and evicts the 350 fungible false-positives (161,559
   rows). **But a one-shot run is not enough** — pending refills to the chain
   tip continuously (see "Why `*_pending` grows unbounded" above). The drain
   must become recurring/live. Live-flow fix (stop leak + reconcile residual)
   spawned as **[0392](../0392_BUG_nft-pending-live-routing-reconcile/README.md)**;
   the one-shot prod drain remains **0306**.
2. **Unclassifiable residual (423 Other collections, 14,632 tokens).** Cannot be
   resolved without observing each contract's WASM interface. Owned by **0309**
   (classifier design), **0320** (WASM-upgrade reclassify), **0316** (WASM
   observation). Not new work here.
3. **(Optional) `consecutive_mint` in `parse_token_event`.** Add for parser
   symmetry only — zero coverage impact (Path B already covers it). Low priority.

## Dependency notes

- **0359 (`operation_asset_appearances`) is IRRELEVANT to NFTs.** No NFT
  endpoint reads that table (verified). The "0359 not yet on prod" caveat that
  gates 0383's _asset-side_ backfill does **not** gate any NFT coverage.
- 0383's own account-side deploy (PR #332) closes NFT transfer-`from` /
  burn-`from` participants (Path A). Until then prod has only the owner (`to`)
  side for those via Path B — 0383's scope, not this task's.
