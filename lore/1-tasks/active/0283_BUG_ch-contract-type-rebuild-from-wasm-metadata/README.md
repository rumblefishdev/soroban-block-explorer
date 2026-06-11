---
id: '0283'
title: 'BUG: CH never writes Nft/Fungible verdicts to soroban_contracts — contract-type rebuild from wasm_interface_metadata + prod NFT reclassification'
type: BUG
status: active
related_adr: ['0046']
related_tasks: ['0118', '0217', '0220', '0221', '0228', '0231', '0259', '0282']
blocked_by: []
tags:
  [
    clickhouse,
    nft,
    quarantine,
    contract-classification,
    pre-launch,
    priority-high,
    effort-medium,
    layer-data,
  ]
milestone: 1
links:
  - docs/runbooks/0217_nfts_pending_migration_and_drain.md
  - docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md
  - docs/runbooks/artifacts/phase6_validation_20260521.md
history:
  - date: 2026-06-10
    status: backlog
    who: karolkow
    note: >
      Spawned from a deep-dive triggered by 0231 NFT enrichment testing:
      prod CH hot nfts/nft_ownership are empty (0 rows) while pending
      holds 59.7M/138.5M and grows ~1M/day. Root cause found in code:
      CH stage writes contract_type verdicts only when WASM upload and
      contract deploy land in the SAME ledger (near-never on Soroban —
      separate txs), the "re-emission on next observation" path
      documented in ADR 0046 does not exist in code, and nothing
      rebuilds verdicts post-hoc — so `backfill-runner nft-reclassify`
      promotes 0 forever (empirically: promoted_nfts=0 in 0228 Phase 5).
      Full evidence chain in notes/S-deep-dive-root-cause.md.
  - date: 2026-06-10
    status: active
    who: karolkow
    note: 'Activated; work starts in worktree 0283-nft-reclassify.'
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      Investigation session (Claude). Restored the small tables from the
      local CH backup snapshot_b_post_0252 into a throwaway container and
      ran Step 0 queries as a prod proxy (2026-05-21 state). Empirical
      confirmation + sizing, a SAC/asset-model deep dive, the crate-location
      decision (NOT backfill-runner → new `ch-maintenance-runner`), and the live-gap
      latency simulation (option c) all captured in
      notes/S-snapshot-findings-location-and-live-decisions.md. New Step 6
      added for the Bachini/i128 event-extraction gap.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      Scope consolidated into one task (operator decision): assets-fungible
      backfill + live fix pulled in (Steps 2, 5). Measured the batch pipeline
      end-to-end (~9 s full-scale) — the "heavy mutation / 0281 window" worry
      was over-cautious. Fixed `queries_ch.rs::contract_type_name` (2→nft,
      3→fungible) + test (DONE). LIVE design changed from inline dict/cache to
      a 3rd async reclassification Lambda (enrichment-worker pattern, scheduled/
      coalesced, singleton guard) — removes the 4 s-budget/dict concern. Live
      analysis + "why lookups are normally cheap" in the findings note.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      (pm) Indexer-reads claim verified: the PG path DID do cross-ledger DB
      reads at persist (reclassify_contracts_from_wasm + assets bridge +
      promote) — dropped at the 0241 CH cutover; bug #4 is a parity gap, not
      a design constant. Live decision RE-OPENED: dev-cost comparison favors
      the inline port (~2-3 d, no infra) over the 3rd Lambda (~4-6+ d);
      recommendation A, operator to confirm. Crate renamed to
      ch-maintenance-runner (consistent *-runner family). Classifier stays in
      xdr-parser (used by indexer at staging.rs:561; "not used in indexer"
      claim was false). Details in the findings note addendum.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      (eve) LIVE DECISION FINAL: inline in the indexer; 3rd-Lambda proposal
      dropped after CTO review of the research brief (brief delivered, then
      removed — content folded into Step 5 + the findings note). Fundament audit:
      quarantine is NOT speculative classification (API never reads pending;
      hot = WASM-confirmed only) — with inline G1+G9 it degrades to a DLQ;
      elimination ladder defined (inline → deploy-linkage fix → TRUNCATE/drop).
      Simulations on the full-scale snapshot killed the cost fears: batched
      lookup 4–8 ms flat, "peak 59 deploys" = 1 unique wasm hash, routing
      cache ~9 misses/day, dictionary option built+validated+rejected.
      CORRECTED: ~99.4% of pending stays after reclassify (no-deploy-link
      contracts) — NEW follow-up findings: deploy-linkage gap (4,461
      contracts) and SAC-skeleton exposure in /v1/contracts (294,963 rows).
      Step 5 rewritten as decided; AC updated. Earlier 2026-06-11 entries
      mis-attributed to stkrolikiewicz (stale session file) — corrected, the
      whole session was karolkow.
---

# BUG: CH never writes Nft/Fungible verdicts — contract-type rebuild + prod NFT reclassification

## Summary

On the ClickHouse path, `soroban_contracts.contract_type` never receives a
WASM-derived `Nft`/`Fungible` verdict (except a same-ledger coincidence that
practically never happens), so the entire NFT promotion machinery
(`backfill-runner nft-reclassify`, runbook 0217 §Part 2) is a no-op: hot
`nfts`/`nft_ownership` stay at 0 rows, NFT endpoints E15/E16/E17 serve
nothing, and 59.7M/138.5M quarantine rows can never drain. The classification
INPUT already exists in CH (`wasm_interface_metadata`, 3,216 WASMs with
function lists) — what's missing is one rebuild step joining it back into
`soroban_contracts.contract_type`. This task adds that step and runs the
full reclassification on prod.

**Scope broadened (operator decision 2026-06-11): everything in one task.** The
same same-batch bug also under-populates `assets.asset_type=3` (Soroban
fungibles) — so the assets backfill + live fix are pulled in (Steps 2, 5).
**Live fix DECIDED (2026-06-11, after CTO review): inline in the indexer** —
re-implementation of the cross-ledger bridges the PG path had, in the CH
writer (~2–3 days, no new infra, fail-open); the 3rd-Lambda alternative was
evaluated and dropped (option analysis: Step 5 + the findings note).
New crate `ch-maintenance-runner` hosts the batch logic (one-shot history
rebuild + by-design-batch ops) — complementary to inline, not an alternative.

## Root cause (evidence)

1. **Verdict written only on same-ledger coincidence.**
   `crates/db-clickhouse/src/persist/stage.rs:376-389` — deploy row gets
   `Nft`/`Fungible` only if the WASM was classified "in the same ledger"
   (comment verbatim). `wasm_classification` map is built per stage call
   from this call's `contract_interfaces` (`stage.rs:343-363`), and a stage
   call covers exactly one ledger (`persist.rs:63-99`). On Soroban, upload
   WASM and create-contract are separate transactions (1 op/tx) — almost
   always different ledgers. Non-SAC deploys therefore persist the parser
   default `Other` (`crates/xdr-parser/src/state.rs` deployment tests).

2. **ADR 0046's "re-emission on next observation" does not exist.**
   `route_for` (`stage.rs:909-918`) consults only `verdict_by_contract`
   built from THIS ledger's `contract_rows`; events emit at most stub rows
   with `contract_type: None`, which the map skips. A later NFT event from
   a classified contract still routes to Pending. The ADR documents intent,
   not code.

3. **Nothing rebuilds verdicts post-hoc.**
   `crates/backfill-runner/src/repair_tier1.rs:327` passes `sc.contract_type`
   through unchanged. `crates/backfill-runner/src/nft_reclassify.rs:193-194`
   promotes `WHERE contract_type = 2` — a set nothing ever populates.
   Empirical proof: 0228 Phase 5 run on the full merged backfill reported
   `promoted_nfts=0` (`docs/runbooks/artifacts/phase6_validation_20260521.md`).

4. **Live ingest has the same gap.** Indexer (post-0241 cutover) persists
   via the same `persist_ledger_clickhouse` path
   (`crates/indexer/src/handler/mod.rs:30`). New NFT contracts deployed
   today also stay `Other`. Pending grows ~1M rows/day.

Full deep-dive (incl. corrections to prior assumptions, prod state
numbers, verification SQL): [notes/S-deep-dive-root-cause.md](notes/S-deep-dive-root-cause.md).

## Empirical findings (local snapshot proxy, 2026-06-11)

Step 0 queries run on the restored `snapshot_b_post_0252` (2026-05-21 / Phase 6
state — re-run on live prod for go-live sizing). Full detail incl. asset/SAC
model, contracts-vs-rows, pending breakdown, location + live decisions:
[notes/S-snapshot-findings-location-and-live-decisions.md](notes/S-snapshot-findings-location-and-live-decisions.md).

- **Verdict breakdown**: of 26,401 non-SAC contracts, exactly **1** ever got
  `Nft` (type=2), 2 got `Fungible` (type=3) — confirms root cause empirically.
- **Would-be-Nft after rebuild: 107 contracts** (vs 1 today); would-be-Fungible
  **3,937** with the exact classifier predicate (an earlier 4,159 figure used a
  looser OR-predicate). Decision is per **contract**; promote acts per **row**.
- **Promote volume**: 107 NFT collections hold **11,023** token rows in
  `nfts_pending` + **19,451** ownership events (one collection `CBHUX3RS…` =
  10,056 tokens). Real data exists — `promoted_nfts` will be >0, not 0.
- **What reclassify does (CORRECTED 2026-06-11 pm)**: promote ~0.02% (Nft),
  drop SAC+Fungible (~0.5%), and **~99.4% STAYS in pending** — it belongs to
  4,461 contracts with **no deploy/wasm_hash link at all** (deploy never
  observed; top offender `CDP5RUMSC7YJ…` alone = 4.86M rows). Unresolvable at
  write time AND at rebuild time → the TRUNCATE decision (Step 6) governs it,
  and the **deploy-linkage gap is a new follow-up finding** (earlier claim
  "~99.97% dropped" was based on a LEFT JOIN mislabel — FixedString non-match
  fills zero-bytes, not NULL).
- **Bachini** (`CDA5FGE4…`, the one verified real NFT): sits as `Other`
  (rebuild fixes it) **but has 0 rows in either pending table** → a separate
  event-extraction gap, see **Step 6**.
- **Assets — SAME bug class, second table.** `asset_type` enum is
  explorer-synthetic (0 Native / 1 ClassicCredit / 2 Sac / 3 Soroban-fungible).
  `asset_type=3` (Soroban bespoke fungible) has only **2 rows** — the _same_
  same-batch-coincidence bug: ~3,935 would-be-fungible non-SAC contracts are
  **missing from `assets`**. The PG persist path has a late-WASM assets bridge
  (`insert_assets_from_reclassified_contracts`); the **CH path never ported
  it**. Now IN SCOPE (Steps 2 + 5) per operator decision.

## Why this gates launch

- E15/E16/E17 (`/nfts*`) serve zero data on prod (0259 documented the gap;
  this task is its actual unblocking dependency).
- 0231 (NFT `token_uri` enrichment) Step 4 prod drain has an empty queue
  until hot `nfts` fills; 0282 needs a real NFT population sample.
- ~27.6M accumulated SAC-leak rows (0221) in pending get dropped by the
  same reclassify run — separate manual drains become unnecessary.

## Implementation Plan

### Step 0 — verification queries on prod CH (no code)

Run the four queries from
[notes/S-deep-dive-root-cause.md §Verification queries](notes/S-deep-dive-root-cause.md)
(verdict breakdown; would-be-Nft count via `wasm_interface_metadata` join;
Bachini `CDA5FGE4…` sanity row; pending volume under would-be-Nft
contracts). These size the promote volume BEFORE building anything and
confirm the root cause empirically. Requires mTLS cert
(`infra-hetzner/ca/issue-client-cert.sh`) + `~/.config/soroban-prod.env`.

### Step 1 — `ch-maint contract-type-rebuild` (NEW crate, not backfill-runner)

**Location decision (2026-06-11):** this does **NOT** belong in
`backfill-runner` (its charter is S3 historical ingestion, a one-shot job
complete per task 0228 — the CH-maintenance ops squatting there only because
that crate already had a CH sink). Create a new crate
**`crates/ch-maintenance-runner`** (bin `ch-maint`), a CH post-hoc maintenance
toolbox modeled on the standalone-CLI precedent of `backfill-enrichment-runner`.
**Relocate** `repair-tier1`, `asset-aggregates`, `nft-reclassify` into it too
(same family; rebuild → nft-reclassify is one ordered pipeline). Rationale +
deps in [the findings note](notes/S-snapshot-findings-location-and-live-decisions.md).

New module `crates/ch-maintenance-runner/src/contract_type_rebuild.rs`, modeled
on `repair_tier1.rs` (staging table + `EXCHANGE TABLES` swap):

- Read `wasm_interface_metadata` (wasm_hash → metadata JSON with
  `functions[].name`), classify each hash in Rust by reusing
  `xdr_parser::classify_contract_from_wasm_spec` (exact PG parity — do
  NOT reimplement the rule set in SQL).
- Rebuild `soroban_contracts` into staging:
  `contract_type = classify(wasm) if wasm_hash matches AND NOT is_sac AND
verdict ∈ {Nft, Fungible}`, else passthrough. SAC rows untouched.
- `EXCHANGE TABLES`, drop staging. Idempotent, `--dry-run` support
  (counts per verdict transition), same logging shape as `nft_reclassify`.

> **`status` command?** backfill-runner's `Status` is S3-ingest progress —
> untouched by this work, stays as-is. Any status/report for the maintenance
> ops (verdict breakdown, would-be-Nft, promote/drop volumes) is a NEW
> `ch-maint` concern (`--dry-run` summaries or a `ch-maint status`).

### Step 2 — `ch-maint assets-fungible-backfill` (NEW — pulled into scope)

The same bug hits a second table: `assets.asset_type=3` (Soroban bespoke
fungible) is emitted only on the same-batch WASM+deploy coincidence
(`xdr-parser/src/state.rs:853-871`), so ~3,935 would-be-fungible contracts are
missing from `assets` (only 2 present). The rebuild (Step 1) makes
`soroban_contracts.contract_type=3` authoritative, so this is then mechanical:

- One-shot `INSERT INTO assets` of the missing type-3 rows:
  `SELECT asset_type=3, contract_id=sc.id, name=sc.name, … FROM soroban_contracts
sc WHERE sc.contract_type=3 AND NOT is_sac AND NOT EXISTS(matching asset row)`.
- Identity is the 4-tuple `(asset_type, asset_code, issuer_id, contract_id)`;
  type-3 rows carry `contract_id` only (code/issuer NULL). `--dry-run` + counts.
- Lives in `ch-maintenance-runner` next to `contract-type-rebuild` (shares the CH
  client + staging helpers). SAC (type-2) and classic (type-1) untouched.

### Step 3 — run on prod: rebuild → assets-backfill → nft-reclassify

1. `ch-maint contract-type-rebuild --dry-run` → compare with Step 0; real run.
2. `ch-maint assets-fungible-backfill --dry-run` → real run.
3. `ch-maint nft-reclassify --dry-run` → real run. Existing code
   (relocated into `ch-maintenance-runner`) covers BOTH `nfts_pending` AND
   `nft_ownership_pending` (promote type=2, drop SAC/fungible, OPTIMIZE FINAL).
4. Record before/after counts (pending totals, promoted, dropped, hot totals,
   assets type-3 count).
5. **Runtime measured (local snapshot, full-scale):** whole pipeline ~**9 s**
   — rebuild 0.43 s + EXCHANGE 0.13 s, promote 0.25 s, `ALTER DELETE` full
   drain 1.15 s (48.8M) + 6.64 s (112M), OPTIMIZE ~0.2 s. The "coordinate with
   0281 maintenance window / heavy ALTER DELETE" worry is over-cautious — it's
   seconds. Still verify on the single remote Hetzner node under live merge
   load; don't start a second run while a previous one's mutations are still
   merging (`system.mutations` check).

### Step 4 — API: `contract_type_name` fix (DONE) + verify contracts-list

`GET /v1/contracts` is a pure consumer of `soroban_contracts.contract_type`
(`queries_ch.rs:103`, reads via `FINAL`; `filter[type]=nft` → `= 2`), so the
rebuild flips its counts **1 Nft / 2 Fungible → 107 / ~3,937 with NO API code
change**. **DONE 2026-06-11:** fixed `queries_ch.rs::contract_type_name`
(2→nft, 3→fungible) + its stale test (was the CH/PG divergence that returned
`contract_type_name:null` for the new rows). Verify the live counts after the
prod run.

### Step 5 — LIVE fix: **DECIDED — inline in the indexer** (2026-06-11, after CTO review)

The 3rd-Lambda proposal was evaluated against measurements and dropped; full
option analysis + devil's advocate per option in the
[findings note](notes/S-snapshot-findings-location-and-live-decisions.md).

**What it is:** re-implementation, on the CH writer, of the three cross-ledger
bridges the PG path ran in production (dropped at the 0241 cutover):
`reclassify_contracts_from_wasm` (`indexer/handler/persist/write.rs:240-325`),
`insert_assets_from_reclassified_contracts` (`write.rs:543-584`),
`promote_pending_nfts_to_hot` (`write.rs:337-417`). PG itself is being removed
from the project — we port the **algorithm**, not the database. The stage
stays pure (no DB); the post-stage step runs in the writer/handler (both
already hold a `clickhouse::Client` — `writer.rs:72`, `handler/mod.rs:130`).

**Scope (gap inventory G1–G9, Addendum 2):**

- **G1** verdict at deploy: one batched `wasm_hash IN(...)` lookup, only on
  deploy-bearing ledgers (**0.18%** of ledgers).
- **G2** `assets` type-3 row on a Fungible verdict (same trigger).
- **G3** promote pending→hot on an actual Nft flip (~once per 4 days).
- **G5** name-write clobber fix (name-only RMT row must not NULL out
  wasm_hash/deployer — read-merge before re-emit).
- **G9** verdict at event-routing time: lazy in-memory verdict cache
  (5,707 distinct emitting contracts; **~9 cache-misses/day**; never cache
  unknown; Nft/Fungible verdicts are immutable once set). G9 **also closes the
  0221 write-time SAC leak** (SAC-emitted events get dropped at routing instead
  of leaking into pending — the leak regrew 8.6M/18.9M rows since Phase 5).
- G2 also covers the `assets.name` mirror (PG pass-2 behavior never ported).
- G6 (asset aggregates) + G8 (first_seen watermarks) stay batch **by design**
  (every-ledger triggers); G4 (SAC) already covered in stage; G7 obsolete.

**Measured (full-scale snapshot):** batched lookup **4–8 ms** flat
(IN 1…1000); the feared "59-deploy ledger" = 59 instances of ONE wasm hash →
1 query; per-contract promote (10k rows) **5–8 ms**; option-(d) dictionary
built and validated end-to-end (load 110 ms, 508 KiB, dictGet 3–7 ms) —
works but adds nothing at these frequencies, rejected. Net cost: **0 ms on
~99% of ledgers, 4–8 ms otherwise** vs the ~4 s budget. Only unmeasured
variable: Lambda→Hetzner RTT (assumed 30 ms — confirm with one probe).

**Fail-open ladder (zero correctness risk):** cache hit → route (0 ms); miss →
one batched SELECT (4–8 ms); DB doesn't know / query fails → behave exactly
as today (`Other` + quarantine) → batch backstop drains later. The new code
can only degrade to current behavior, never below it.

**Endgame:** with G1+G9 live, pending degrades from a pipeline stage to a
**DLQ** (inflow = only unknown-deploy contracts = the deploy-linkage bug).
Elimination ladder: inline fix → fix deploy-linkage gap (follow-up) →
TRUNCATE / drop the pending tables entirely ("classify once, correctly").

### Step 6 — 0217 Part 2 TRUNCATE decision (explicitly OUT of first run)

Do NOT truncate pending in the same pass. After rebuild+reclassify, what
remains (~99.4%) belongs to the 4,461 no-deploy-link contracts (the
deploy-linkage bug) plus genuine `Other`. TRUNCATE only AFTER the elimination
ladder's step 2 (deploy-linkage fix) — truncating earlier silently destroys
data that may include real NFTs (the exact Patch-C mistake ADR 0046 rejected).

### Step 7 — Bachini / i128 token_id event-extraction gap

Step 0 found Bachini (`CDA5FGE4…`, the only verified real mainnet NFT) has
**0 rows in both pending tables** — so even a correct rebuild + reclassify
surfaces nothing for it. Its events were never extracted; deep-dive flags it
as SEP-39 with **i128 token_id**, a shape the event parser likely doesn't
capture. This is a **different subsystem** (XDR event extraction, not
classification) — keep as a tracked step here, but it may graduate to its own
task. Without it, "NFTs fixed" still leaves the flagship NFT empty.

### Step 8 — docs

- **ADR 0046 correction**: the CH "re-emission on next observation"
  promotion path is documented but not implemented; amend with the actual
  mechanism (rebuild + reclassify, live worker) and link this task.
- Update `docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 2
  CH section: prepend the contract-type-rebuild step; commands `ch-maint …`.
- Mark 0221 manual drain runbook as subsumed by `nft-reclassify`.
- `docs/architecture/database-schema/clickhouse-pilot.md` §quarantine: add the
  rebuild step to the promotion lifecycle.

## Acceptance Criteria

- [ ] Step 0 verification queries run on prod; results recorded in task
      notes (verdict breakdown, would-be-Nft contract count, pending
      volume under those contracts).
- [ ] New crate `crates/ch-maintenance-runner` (bin `ch-maint`) created; `repair-tier1`,
      `asset-aggregates`, `nft-reclassify` relocated out of backfill-runner into it.
- [ ] `ch-maint contract-type-rebuild` implemented (staging+EXCHANGE, Rust-side
      classifier reuse, `--dry-run`, idempotent).
- [ ] `ch-maint assets-fungible-backfill` implemented (Step 2 — insert missing
      type-3 Soroban-fungible `assets` rows from `contract_type=3`).
- [ ] Unit/integration test: contract with `Other` verdict + matching
      `wasm_interface_metadata` carrying `owner_of` flips to `Nft`;
      SAC row untouched; contract without metadata untouched.
- [ ] Prod run executed: rebuild → assets-backfill → `nft-reclassify`;
      before/after counts recorded (hot `nfts`/`nft_ownership` non-zero —
      local proxy ~11,023 / 19,451 promote; SAC/fungible pending dropped;
      assets type-3 grows ~2 → ~3,937).
- [ ] E15/E16/E17 smoke against prod after the run (links 0259).
- [x] **`queries_ch.rs::contract_type_name` fixed** (2→nft, 3→fungible) + test
      updated — DONE 2026-06-11. Verify `GET /v1/contracts` counts post-run.
- [ ] **LIVE fix DECIDED — inline in the indexer** (Step 5): G1 verdict at
      deploy + G2 assets row + G3 promote-at-flip + G5 name-clobber fix +
      G9 routing cache — fail-open, batched, gated to rare ledgers
      (measured: 0 ms on ~99% of ledgers, 4–8 ms otherwise). 3rd-Lambda
      alternative evaluated and dropped after CTO review.
- [ ] RTT Lambda→Hetzner measured (one probe via mTLS) — confirms the last
      assumption (30 ms) behind the live numbers.
- [ ] **Inline step instrumented + verified on prod**: emit per-ledger timing
      of the new step, cache hit/miss counters, and a fail-open counter
      (lookup skipped due to error). After deploy, compare a week of prod
      metrics against the simulated numbers (~0 ms typical / 10–40 ms on
      deploy-WASM-miss ledgers / fail-open ≈ 0) — closes the "if it's real"
      question with production data.
- [ ] Follow-up task spawned: **deploy-linkage gap** — 4,461 contracts emit
      events but have no deploy/wasm_hash ever (99.4% of pending; top
      `CDP5RUMSC7YJ…` = 4.86M rows); blocks the TRUNCATE endgame.
- [ ] Follow-up task spawned: **SAC skeleton exposure** — 294,963 derived
      skeleton rows (92% of `soroban_contracts`) visible in `/v1/contracts`
      with no filter (real violation of "no speculative user-facing rows").
- [ ] Bachini/i128 SEP-39 event-extraction gap (Step 7) — investigated;
      tracked here or graduated to its own task.
- [ ] ADR 0046 amended (re-emission correction → actual mechanism: inline
      bridges + batch backstop) + runbook 0217/0221 updated; command strings
      `backfill-runner …` → `ch-maint …`.
- [ ] **Docs updated** — `clickhouse-pilot.md` §quarantine (rebuild step +
      pending-as-DLQ); ingestion-pipeline docs for the new inline writer step;
      infra topology N/A (no new Lambda).
- [ ] **API types regenerated** — `crates/api/**` touched (contract_type_name) + new `Cargo.lock` → run `nx run @rumblefish/api-types:generate` before
      commit (label change likely no-op on the spec, but the gate checks it).

## Notes

- TRUNCATE of pending is deliberately deferred (Step 6) — destructive; safe
  only after the deploy-linkage fix (elimination ladder step 2).
- **Quarantine is NOT speculative classification** — it's the opposite: the
  API never reads `*_pending`; hot tables receive only WASM-confirmed rows
  (pre-quarantine design measured 99.4% garbage in `/v1/nfts*`). With the
  inline fix it degrades to a DLQ; rows arriving there = bug signal.
- Expected scale (measured on the snapshot proxy): 294,963/321,364 contracts
  are SAC (~92%); would-be-Nft is **107 contracts → ~11,023 token rows**.
  Empty-ish hot tables after a CORRECT run are a product reality, not a bug —
  re-confirm on live prod (Step 0).
- Investigation env still running locally: container `ch-snap` (restored
  small tables from `~/snapshots/snapshot_b_post_0252`, port 8123) + `ch-ui`
  (port 3488). Originals/backup untouched; benches ran on copies.
