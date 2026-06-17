---
id: '0283'
title: 'BUG: CH never writes Nft/Fungible verdicts to soroban_contracts — contract-type rebuild from wasm_interface_metadata + prod NFT reclassification'
type: BUG
status: backlog
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

### Step 1 — `backfill-runner contract-type-rebuild` subcommand

New module `crates/backfill-runner/src/contract_type_rebuild.rs`, modeled
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

### Step 2 — run on prod: rebuild → nft-reclassify

1. `backfill-runner contract-type-rebuild --dry-run` → compare counts with
   Step 0 expectations; then real run.
2. `backfill-runner nft-reclassify --dry-run` → real run. Existing code
   (`nft_reclassify.rs`) already covers BOTH `nfts_pending` AND
   `nft_ownership_pending` (promote type=2, drop types 0/2/3, legacy
   cleanup, OPTIMIZE FINAL) — no new drain code needed.
3. Record before/after counts in this task (pending totals, promoted,
   dropped, hot totals).
4. Schedule within / coordinate with the 0281 maintenance window if the
   mutations are heavy (ALTER DELETE on ~30M+ rows).

### Step 3 — 0217 Part 2 TRUNCATE decision (explicitly OUT of first run)

Do NOT truncate pending in the same pass. After rebuild+reclassify, what
remains in pending is contracts with no WASM interface observed or genuine
`Other`. Decide TRUNCATE separately (0217 §Part 2 sanity probe first),
once live-gap strategy (Step 4) is settled — otherwise truncated rows for
late-classified contracts are unrecoverable.

### Step 4 — live-gap strategy (spawn follow-up, decide, don't build here)

One-shot rebuild fixes the backfill snapshot, but live ingest keeps
writing `Other` (gap #4 above). Options: (a) periodic
`contract-type-rebuild + nft-reclassify` (cron/EventBridge), (b) DB-lookup
in CH stage (0221 Option A — breaks stage purity), (c) verdict cache
bootstrap at indexer cold start. Spawn a follow-up task with the decision;
this task only documents the trade-off.

### Step 5 — docs

- **ADR 0046 correction**: the CH "re-emission on next observation"
  promotion path is documented but not implemented; amend with the actual
  mechanism (rebuild + reclassify pass) and link this task.
- Update `docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 2
  CH section: prepend the contract-type-rebuild step (without it the
  promote SELECT matches nothing).
- Mark 0221 manual drain runbook as subsumed by `nft-reclassify` (keep for
  reference).

## Acceptance Criteria

- [ ] Step 0 verification queries run on prod; results recorded in task
      notes (verdict breakdown, would-be-Nft contract count, pending
      volume under those contracts).
- [ ] `backfill-runner contract-type-rebuild` subcommand implemented
      (staging+EXCHANGE pattern, Rust-side classifier reuse, `--dry-run`,
      idempotent re-run).
- [ ] Unit/integration test: contract with `Other` verdict + matching
      `wasm_interface_metadata` carrying `owner_of` flips to `Nft`;
      SAC row untouched; contract without metadata untouched.
- [ ] Prod run executed: rebuild → `nft-reclassify`; before/after counts
      recorded (hot `nfts`/`nft_ownership` non-zero iff would-be-Nft
      count > 0; SAC/fungible pending rows dropped).
- [ ] E15/E16/E17 smoke against prod after the run (links 0259; full
      validation stays in 0259).
- [ ] Follow-up task spawned for the live-ingest gap strategy (Step 4).
- [ ] ADR 0046 amended (re-emission correction) + runbook 0217 updated.
- [ ] **Docs updated** — `docs/architecture/database-schema/clickhouse-pilot.md`
      §quarantine: add the rebuild step to the promotion lifecycle;
      other architecture docs N/A (no API/schema/infra shape change).
- [ ] **API types regenerated** — N/A unless `crates/api/**` or
      `Cargo.{toml,lock}` touched; expected N/A (backfill-runner only —
      but `Cargo.lock` WILL change if deps are added → regen then).

## Notes

- TRUNCATE of pending is deliberately deferred (Step 3) — destructive,
  and the live-gap decision changes what "safe to truncate" means.
- Expected scale: 96% of 321k contracts are SAC; would-be-Nft population
  likely tiny (possibly only Bachini `CDA5FGE4…` and a handful). Empty-ish
  hot tables after a CORRECT run are a product reality, not a bug —
  Step 0 gives the hard number first.
