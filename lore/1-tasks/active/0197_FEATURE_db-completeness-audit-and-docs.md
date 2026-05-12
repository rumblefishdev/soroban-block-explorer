---
id: '0197'
title: 'DB completeness audit + docs: list/detail field allocation verification, schema coverage matrix'
type: FEATURE
status: active
related_adr: ['0007', '0022', '0023', '0029', '0032', '0037', '0043', '0044']
related_tasks: ['0188', '0191', '0194', '0195', '0196', '0210', '0212', '0213']
tags: [priority-medium, effort-medium, layer-docs, layer-audit]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: 'Spawned from M2 enrichment planning session 2026-05-06. Fourth and final of four tasks (0194-0197). Verifies the field allocation rule (ADR 0043) is followed end-to-end after 0194/0195/0196 land.'
  - date: '2026-05-11'
    status: backlog
    who: karolkow
    note: "Scope expanded to absorb 0196's deferred verification work (the 50K enrichment-drain benchmark + per-subcommand integration tests previously folded into 0196 Future Work). 0196 itself still ships as a prerequisite — this task absorbs only the post-merge verification gate, not the crate implementation. Benchmark covers every kind the `enrich` binary exposes (`sep1-assets` mandatory; `nft-metadata` once 0195 §2d Phase E ships; future kinds added via their own delivery PRs)."
  - date: '2026-05-12'
    status: active
    who: karolkow
    note: 'Activated. Bundling 0196 work into this branch since needed here.'
  - date: '2026-05-13'
    status: active
    who: karolkow
    note: >
      Step 7 (enrichment drain benchmark on staging, 50K rows per
      kind, concurrency sweep) removed entirely. The benchmark was
      absorbed from 0196 Future Work in the 2026-05-11 scope-expand,
      but requires staging access + production-scale seeded data + a
      shipped 0195 §2d Phase E NFT fetcher. None of those align with
      the local-only execution decision (2026-05-13) and Phase E
      isn't shipped, so `nft-metadata` would panic on row 1 anyway.
      The `< 30 min` target in `backfill-enrichment-runner` README
      stays unverified; if/when a real staging benchmark is wanted
      it'll be its own task, not a 0197 acceptance criterion.
---

# DB completeness audit + docs: list/detail field allocation verification, schema coverage matrix

## Summary

Final verification gate for the 0194-0197 task chain. Audits every list endpoint to confirm every returned field has a DB column that is indexed (where sortable/filterable) and populated (≠ always NULL on a controlled local sample). Audits every detail endpoint to confirm unique-to-detail fields do **NOT** have dedicated DB columns and are runtime type-2 enrichment instead. Refreshes all `docs/architecture/**` per ADR 0032 evergreen rule. Outputs a one-time snapshot to `docs/audits/`.

**Audit type:** This is **Type A — bulk volumetric** ("does the pipeline fire end-to-end at realistic scale, what are the populated/sentinel/NULL ratios per column?"). The complementary **Type B — fixture-based per-field correctness vs external sources** (Horizon, stellar.expert) is owned by task **0213** (fixture-asset external parity audit). The two are intentionally split: Type A catches "worker not firing / drain incomplete / sentinel emission" classes of bugs; Type B catches "values are wrong / drift vs Horizon / parser returns wrong field" classes. Both must pass for the audited surface to be considered healthy.

**Environment:** All audit work runs **on a local Postgres** (Docker Compose), against a small **controlled mini-backfill** (a few hundred ledgers ingested by `backfill-runner` with `--keep-partitions`, then drained by `backfill-enrichment-runner`). No staging / production access required. Rationale: no real backfill has been run yet on staging or production — the empirical state needed for sample queries does not exist anywhere else. Operating locally also keeps the audit self-contained and re-runnable.

## Context

### Field allocation rule (ADR 0043, locked in 0194 sub-block 1f)

> List endpoint + on-chain (data already in processed ledger) → indexer; off-chain (HTTP / oracle / per-row RPC) → enrichment Lambda 2; detail-only fields → runtime type-2 in API handler, NEVER persisted.

This task verifies the rule is followed across the entire codebase post-0194/0195/0196.

### What "list endpoint" means here

Paginated array endpoints currently include (verify exhaustive list during audit):

- `GET /v1/assets`
- `GET /v1/contracts`
- `GET /v1/liquidity-pools`
- `GET /v1/liquidity-pools/:id/participants` (list-of-rows even though under detail path)
- `GET /v1/liquidity-pools/:id/transactions` (same)
- `GET /v1/nfts`
- `GET /v1/nfts/:id/transfers` (same)
- `GET /v1/transactions`
- `GET /v1/operations`
- `GET /v1/ledgers`
- `GET /v1/accounts`

Each list endpoint has a corresponding DTO under `crates/api/src/{module}/dto.rs` (e.g. `AssetItem`, `PoolItem`, `NftItem`).

### What "detail endpoint" means here

`GET /v1/{resource}/:id` returning a single object, currently:

- `GET /v1/assets/:id` (`AssetDetailResponse` — includes `description`, `home_page` from runtime type-2 SEP-1 fetch per task 0188)
- `GET /v1/transactions/:hash`
- etc.

The pattern to verify: detail-only fields (e.g. `description`, `home_page` on assets) come from runtime SEP-1 fetch in the handler (`crates/api/src/assets/handlers.rs:176-201` per audit), NOT from a DB column. The drop migration `crates/db/migrations/20260424000000_drop_assets_sep1_detail_cols.up.sql` removed these columns and the runtime fetch is the new source of truth.

### Anti-patterns to flag

- **Column populated only by detail endpoint** — column exists, is written, but never read by any list endpoint. Candidate for drop (forces type-2 instead).
- **List endpoint field with no DB column AND no in-handler computation** — bug, returns NULL.
- **List endpoint sortable field with no index on the underlying column** — performance bug.
- **List endpoint field marked "populated by indexer" but always NULL on backfill sample** — wiring incomplete.
- **List endpoint field marked "populated by Lambda 2" but always NULL on backfill sample** — wiring incomplete.

## Implementation Plan

### Step 0: Local audit environment + initial state snapshot

The audit operates on a self-contained local Postgres populated by a
small controlled backfill. This step prepares that environment and
captures the **PRE-enrichment** state for the coverage matrix.

**Run-time choices locked (2026-05-13):**

- **Initial ledger range:** `51000000..51000300` (300 recent pubnet
  ledgers). Widen via Step 0.4 diversity check if any audited entity
  type (`assets` / `lps` / `nfts` / `soroban_contracts`) has zero
  rows after ingest.
- **Audit doc filename base:** `2026-05-13` (audit start date). All
  three snapshot files + the coverage matrix doc use this stem.

1. **Stand up Postgres locally.** `docker compose up postgres` (or
   the project's standard local-dev recipe at audit time). Confirm
   migrations applied via `cargo run -p db-migrate -- migrate`.

2. **Pick a ledger range — start small.** Suggested initial window:
   ~300 ledgers from recent pubnet activity (e.g.
   `LEDGER_RANGE=51000000..51000300`). Sized to be ingestable in
   minutes on a laptop. Rerun with a wider window if Step 0.4
   diversity check fails.

3. **Run indexing backfill (non-destructive).**

   ```bash
   cargo run --release -p backfill-runner -- \
     --start 51000000 --end 51000300 \
     --keep-partitions \
     run
   ```

   The `--keep-partitions` flag preserves the downloaded S3 partition
   archives so re-runs do not re-download. Critical for iterative
   audit work.

4. **Diversity check — confirm coverage of the audited entity types.**
   Small windows may not contain any NFT mint / new LP / SAC asset.
   Run:

   ```sql
   SELECT
     (SELECT COUNT(*) FROM assets)              AS assets,
     (SELECT COUNT(*) FROM liquidity_pools)     AS lps,
     (SELECT COUNT(*) FROM nfts)                AS nfts,
     (SELECT COUNT(*) FROM soroban_contracts)   AS contracts;
   ```

   Each count > 0 means the matrix has a row to verify against.
   Any count == 0 → expand the ledger range and re-run Step 0.3.
   Document the final chosen range in the audit doc.

5. **Capture PRE snapshot.** Run both status commands and save
   raw output as separate snapshot files:

   ```bash
   cargo run -p backfill-runner -- status --start 51000000 --end 51000300 \
     > docs/audits/2026-05-13-pre-indexing-status.txt
   cargo run -p backfill-enrichment-runner -- status \
     > docs/audits/2026-05-13-pre-enrichment-status.txt
   ```

   The `backfill-enrichment-runner status` output is already a
   markdown table reporting NULL count + sentinel count + total per
   audited column. This is the empirical baseline.

6. **Run enrichment drain.**

   ```bash
   cargo run --release -p backfill-enrichment-runner -- sep1-assets
   cargo run --release -p backfill-enrichment-runner -- nft-metadata
   ```

   Capture exit codes + any worker-log errors for the audit doc.
   Permanent errors (TOML 404, IPFS gateway 404, etc.) become
   sentinels by design — these are expected, not failures.

7. **Capture POST snapshot.**
   ```bash
   cargo run -p backfill-enrichment-runner -- status \
     > docs/audits/2026-05-13-post-enrichment-status.txt
   ```

PRE vs POST diff is the central empirical evidence: indexer-driven
columns must be non-NULL already PRE; enrichment-driven columns must
flip NULL → (populated or sentinel) between PRE and POST.

**Note on the `enrich status` output shape.** The command reports
NULL count + `''`-sentinel count + total rows per kind. "Populated
with a real value" is derived implicitly as `total - NULL - sentinel`.
Auditor should make that derivation explicit when transferring the
status table into the audit md so the third category is visible
without arithmetic. (Adding a fourth explicit column to the binary's
status output is a small follow-up improvement; not blocking this
audit.)

### Step 1: Coverage matrix (audit deliverable)

**Scope:** Postgres store only, against the local mini-backfill DB
built in Step 0. ClickHouse pilot (ADR 0044, tasks 0204/0206/0207)
maintains a parallel store with a separate endpoint-query reference
set (`endpoint-queries-clickhouse/`); CH is **not** yet wired to the
API read-path. A CH-side equivalence audit is deferred to a follow-up
task that runs once CH serves a real `/v1/*` handler — running it now
would only verify the 0207 reference SQL against itself.

Output `docs/audits/2026-05-13-list-endpoint-completeness.md` with a single table per endpoint:

| Endpoint     | DTO field         | DB column         | Indexed? | Populated by           | PRE drain (NULL / sentinel / populated) | POST drain (NULL / sentinel / populated) |
| ------------ | ----------------- | ----------------- | -------- | ---------------------- | --------------------------------------- | ---------------------------------------- |
| `/v1/assets` | `id`              | `assets.id`       | PK       | indexer (insert)       | 0 / 0 / N                               | 0 / 0 / N (unchanged)                    |
| `/v1/assets` | `asset_type_name` | computed          | n/a      | SQL CASE               | 0 / 0 / N                               | 0 / 0 / N (unchanged)                    |
| `/v1/assets` | `icon_url`        | `assets.icon_url` | btree    | Lambda 2 (sep1_assets) | N / 0 / 0                               | small / some / most                      |
| ...          | ...               | ...               | ...      | ...                    | ...                                     | ...                                      |

Indexer-driven columns must already be populated at PRE (the indexer
writes during ingest). Enrichment-driven columns flip NULL → (populated
or sentinel) between PRE and POST. Any indexer-driven column showing
NULL at PRE = bug. Any enrichment-driven column showing 0 populated at
POST (with sentinel only or all NULL) = bug.

Workflow:

1. For each endpoint, read DTO struct from `crates/api/src/{module}/dto.rs`.
2. For each field, trace the SQL query that produces it (canonical SQL files in repo, e.g. `crates/api/src/sql/15_get_assets_list.sql`).
3. Map to DB column or in-SQL computation.
4. Lookup column index status from migrations / `\d` of the local DB.
5. Identify population owner (indexer / Lambda 2 / handler-computed / SQL-computed).
6. Lift the per-column row from the PRE and POST snapshots captured in Step 0.5 + 0.7 (`backfill-enrichment-runner status` output already gives NULL + sentinel + total per audited column). For columns NOT covered by the status command (anything outside `assets.icon_url` / `assets.name` / `nfts.*` — i.e. indexer-driven columns and LP analytics), run a raw `SELECT COUNT(*) FILTER (...) FROM <table>` query against the local DB and record the same `NULL / sentinel / populated` triple.
7. **Treat `''` as "populated"** per 0191 Design Decision #12: the sentinel means "fetch attempted, no data published by source" — distinct from NULL ("not yet attempted") and from a real value, but does count as "enrichment wired correctly". The matrix shows all three categories separately so the distinction stays visible; the FAIL rule looks only at "NULL after POST drain on a non-skipped row" for enrichment columns.
8. **One-time live-smoke check** for the SEP-1 (`sep1_assets`) and NFT (`nft_token_uri`) enrichment kinds: manually fetch one known live issuer's stellar.toml and one known JSON-metadata NFT collection's `token_uri`, run the full `enrich_and_persist::*` flow against the local DB, and log the resulting row + observed `icon_url` / `media_url` value in the audit md. Purely a one-shot verification during this audit — the persistent `#[ignore]` regression suite that turns this into a recurring CI gate is owned by task **0212** (enrichment live-smoke suite).

Expected outcome: every list endpoint field is a row in the table with no "FAIL" entries. Any FAIL = bug, spawn follow-up task per the
0210 pattern.

### Step 2: Detail-endpoint anti-pattern sweep

For each `:id` detail endpoint:

- List unique-to-detail fields (in detail DTO but not in list DTO)
- For each, verify NO dedicated DB column (or column is dropped/scheduled to be dropped)
- For each, verify implementation = runtime fetch in handler OR computed in SQL on detail-only query
- Flag candidates to drop
- Flag **UI fallback contracts** that depend on sentinels: e.g. `assets.name = ''` (no SEP-1 TOML available) requires the frontend to render `asset_code` instead of an empty string (0195 §2a). Listing these in the audit doc ensures the contract is documented at the API↔FE boundary even when neither side has explicit code for it.

Output: appended section in same audit md. Flag each anti-pattern with proposed fix (drop column, refactor handler).

### Step 3: Docs refresh per ADR 0032 evergreen

Per `lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md`, every PR changing the shape of the system updates `docs/architecture/**`. After 0194/0195/0196 land, the shape has changed substantially. This task picks up any gaps the implementation PRs missed:

- `docs/architecture/database-schema/**` — column matrix, every newly populated column attributed to its source (indexer / Lambda 2 / type-2 handler). Includes one signpost paragraph in `database-schema-overview.md` cross-linking the two endpoint-query reference dirs: `endpoint-queries/` (Postgres canonical, source of truth for `/v1/*` handlers) and `endpoint-queries-clickhouse/` (CH parallel reference, owned by task 0207, **not** wired to the API read-path). Audit scope is PG-only; no edits inside `endpoint-queries-clickhouse/`.
- `docs/architecture/indexing-pipeline/**` — runtime_enrichment umbrella, type-1 SQS model, type-2 runtime model, backfill crate
- `docs/architecture/backend/**` — list vs detail boundary, type-2 detail enrichment pattern, anti-patterns to avoid
- `docs/architecture/xdr-parsing/**` — new responsibilities (volume/fee_revenue/holder_count/classic credit `total_supply`); note `name` for classic credit lives in Lambda 2 (0195 2a), NOT indexer

### Step 4: ADR cross-check

- ADR 0043 (field allocation rule, from 0194) — re-affirm without amendment, OR amend if implementation revealed edge cases
- ADR 0029 (abandon-parsed-artifacts) — read-path ADR. After 0188 the `runtime_enrichment` module became an umbrella covering both `stellar_archive` (heavy-field S3 reread, the original 0029 scope) and `sep1` (issuer TOML fetch). 0188's "Out of Scope" explicitly deferred a 0029 amendment until "a unified description across both submodules is worth writing." Decide one of two outcomes during this audit and document the choice in 0197's Implementation Notes: either (a) amend 0029 to describe `runtime_enrichment` as the umbrella concept (preferred if the audit surfaces frontend / docs confusion about the two submodules' relationship), or (b) keep 0029 unchanged and record an explicit "no amendment required because X" rationale. **No silent skip.**
- ADR 0037 (current-schema-snapshot) — confirm 0194's amendment landed
- ADR 0044 (ClickHouse pilot — parallel store) — confirm no audit-driven amendment required. Audit scope is Postgres-only (see Step 1 preamble); CH parity audit deferred until CH wired to the API read-path. Any amendment here would be out of 0197 scope.

### Step 5: Audit doc supersession

`docs/audits/2026-04-10-pipeline-data-audit.md` Section 8/9 referenced multiple "write-only columns" + "enrichment pipeline gap" findings that are now obsolete. Add a header block to that doc noting it is "partially superseded by 2026-MM-DD list-endpoint-completeness.md" and link forward.

### Step 6: Spawn follow-up tasks for any FAIL findings

Each anti-pattern or wiring gap discovered → backlog task with `audit-gap` tag. Don't fix in this task — this task is the meta-audit, fixes go elsewhere.

## Acceptance Criteria

- [ ] `docs/audits/{TIMESTAMP}-list-endpoint-completeness.md` committed
- [ ] Every list endpoint field has a row in the matrix with non-FAIL status (or follow-up task spawned for each FAIL)
- [ ] Every detail endpoint anti-pattern flagged with proposed fix (or no anti-patterns found)
- [ ] `docs/architecture/database-schema/**` refreshed
- [ ] `docs/architecture/indexing-pipeline/**` refreshed
- [ ] `docs/architecture/backend/**` refreshed
- [ ] `docs/architecture/xdr-parsing/**` refreshed
- [ ] ADR 0043/0029/0037 cross-checked
- [ ] Audit doc 2026-04-10 supersession header added
- [ ] **Docs updated** — this is the task, mark all checked
- [ ] **API types regenerated** — N/A (audit-only, no code changes expected)

## Future Work (out of scope)

- **Periodic completeness check**: this audit is one-shot. If we want continuous protection, add a CI gate that diffs `docs/architecture/database-schema/**` against actual schema and fails if drift detected. Captured as a separate optional task.
- **API contract test**: end-to-end test that hits each list endpoint on a sample DB and asserts no NULL fields. More expensive than this audit. Defer until production complaints surface.
- **Dormant-asset re-verification**: when a real backfill runs on staging (hundreds of millions of pubnet ledgers), re-run the audit and add a per-column sample restricted to assets / contracts that have no activity in the last N ledgers. Catches "live ledgers fine, old rows still NULL" gaps where the drain didn't actually run end-to-end on the dormant set. Deliberately deferred from this task because the local mini-backfill (300 ledgers) is too small to have a meaningful dormant set — every entity present is recent.
- **Per-field external parity (Type B)**: every audited column compared per-row against Horizon, stellar.expert, and the raw issuer source. Owned by task **0213** (fixture-asset external parity audit). 0213 is the natural follow-up: 0197 surfaces "wired correctly?" gaps, 0213 surfaces "value is correct?" gaps.

## Notes

- **Order in chain**: this task is intentionally LAST in the M2 enrichment chain. Running it before 0194/0195/0196 land would surface tons of pre-existing failures — wasted effort. Running it after gives a clean baseline. **0213** (Type B per-field parity) is the natural sibling that runs after this one.
- **Local mini-backfill iteration**: 300 ledgers is the suggested starting window, sized for laptop ingestion. If the Step 0.4 diversity check finds no NFTs (or LPs, or SAC assets) in the chosen range, **widen the range** until each entity type is represented before running enrichment. Pure indexing on a wider range is cheap; the audit value of a row that's never present is zero.
- **Skill invocation**: while writing the audit md, follow `/lore-framework` documentation patterns. While committing, follow `/lore-framework-git`. For spawned follow-ups, follow `/lore-framework-tasks`. (0125 superseding-archive already landed on develop.)
- **Sentinel-aware sample queries**: when checking "non-NULL" for `assets.icon_url`, treat both real URL and `''` sentinel as "populated" — see 0191 design decision #12 + sentinel taxonomy from 2026-05-06 session.
- **Dry-run audit performed 2026-05-06** (during planning session, BEFORE 0194/0195/0196 spawn): confirmed 95% pre-coverage, surfaced 1 misallocation (classic credit `assets.name` was placed in 0194 indexer, should be Lambda 2 — fixed via amendment to 0194 1b + new 0195 2a icon-name extension). Real run of this task should not need to surface that issue again. Other dry-run flags (`AssetDetailResponse.deployed_at_ledger`, `account_balances_current.first_deposit_ledger`) were false positives — first is a legitimate entity-record column read by `/v1/search`, second doesn't exist in that table (subagent confused with `lp_positions.first_deposit_ledger`). Real audit should still verify these independently.
