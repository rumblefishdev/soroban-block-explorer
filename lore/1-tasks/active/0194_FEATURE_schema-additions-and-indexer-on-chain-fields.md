---
id: '0194'
title: 'DB completeness: schema additions + indexer for on-chain NULL fields needed by list endpoints'
type: FEATURE
status: active
related_adr: ['0007', '0022', '0023', '0029', '0032', '0037', '0043']
related_tasks:
  ['0119', '0125', '0135', '0156', '0188', '0191', '0195', '0196', '0197']
tags:
  [
    priority-medium,
    effort-large,
    layer-indexer,
    layer-db,
    layer-xdr-parser,
    audit-gap,
  ]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: 'Spawned from M2 enrichment planning session 2026-05-06. First of four tasks (0194-0197) implementing the field allocation rule: list endpoint + on-chain → indexer; off-chain → enrichment Lambda; detail-only → runtime type-2.'
  - date: '2026-05-06'
    status: active
    who: karolkow
    note: 'Activated to start implementation. Branch cut from develop; 0191 SQS enrichment branch will be merged into the feature branch since 0191 PR has not yet landed on develop and 0194 needs its enrichment-shared crate context.'
  - date: '2026-05-06'
    status: active
    who: karolkow
    note: >
      Implementation pass: sub-blocks 1b (total_supply SUM via per-ledger
      recompute on touched assets), 1c (holder_count = COUNT(*) FILTER
      (WHERE balance > 0) — active-holder semantics matching Stellar
      ecosystem convention), 1d (LP volume + fee_revenue via post-INSERT
      UPDATE with prior-snapshot reserve delta + swap-only NOT EXISTS
      filter excluding deposit/withdraw ops 22/23), and 1e (verify-only
      — 0119 trustline path confirmed in
      `crates/indexer/src/handler/persist/write.rs`) landed. Sub-block
      1a removed (usd_price + indexes pulled as speculative). ADR 0043
      already on develop. cargo check + clippy + cargo test -p api/indexer
      all clean. API types regen baseline (no DTO changes net).
---

# DB completeness: schema additions + indexer for on-chain NULL fields needed by list endpoints

## Summary

Audit of list-endpoint DTOs vs DB schema vs actual writes shows a population gap: several columns exist in the schema but are always NULL because the indexer never writes them, and at least two list-endpoint sort fields (asset USD price + timestamp) need new schema columns. This task lands the schema additions atomically and wires indexer-side population for every NULL field whose source data is **already in the processed ledger** (no external HTTP, no per-row RPC). Off-chain fields (oracle prices, SEP-1, NFT `token_uri()` RPC) are the sister task 0195's scope. The governing [ADR 0043](../../2-adrs/0043_field-allocation-rule.md) (field allocation rule) was merged to develop independently before this task's implementation landed.

## Status: Backlog

Cannot start until 0191 PR (`feat/0191_type1-enrichment-worker-lambda`) merges to develop — that PR introduces the SQS enrichment infrastructure and `enrichment-shared` crate that 0195 builds on, and 0194 should land first so 0195's column writes have somewhere to go.

## Context

### Field allocation rule (locked this session)

Per Karol 2026-05-06: any field returned by a **list endpoint** (paginated array endpoints — `/assets`, `/liquidity-pools`, `/nfts`, `/transactions`, etc.) whose source data is **already in the processed ledger** must be populated by the indexer, **not** by enrichment Lambda 2. Off-chain data (HTTP fetches, per-row RPC, oracle calls) goes to Lambda 2. Detail-only fields (returned only by `/:id` endpoints) must NOT have dedicated DB columns — they are runtime type-2 enrichment in the API handler (per task 0188 SEP-1 fetcher pattern, e.g. `assets.description` and `assets.home_page` were dropped in migration `20260424000000_drop_assets_sep1_detail_cols.up.sql`).

### NULL-column inventory verified this session

Subagent audit confirmed by reading `crates/xdr-parser/src/state.rs`, `crates/indexer/src/handler/persist/{staging.rs,write.rs}`, `crates/api/src/{assets,liquidity_pools,nfts}/dto.rs`, and `crates/db/migrations/`:

| Table.column                            | DB type       | Currently                                                | On-chain?                                         | This task scope                                                                           |
| --------------------------------------- | ------------- | -------------------------------------------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `assets.holder_count`                   | INTEGER       | always NULL                                              | ✅ trustline delta (`change_trust create/delete`) | sub-block 1c                                                                              |
| `assets.name` (classic credit)          | VARCHAR(256)  | NULL for classic credit (Soroban handled by 0156 active) | ❌ — classic credit names come from SEP-1 TOML    | **OUT OF SCOPE — moved to 0195 sub-block 2a** (icon kind extended to also persist `name`) |
| `assets.total_supply` (classic credit)  | NUMERIC(28,7) | NULL for classic credit                                  | ✅ SUM of trustline balances                      | sub-block 1b (depends on 1e)                                                              |
| `liquidity_pool_snapshots.volume`       | NUMERIC(28,7) | always NULL                                              | ✅ PathPayment ops + LP swap event delta          | sub-block 1d                                                                              |
| `liquidity_pool_snapshots.fee_revenue`  | NUMERIC(28,7) | always NULL                                              | ✅ derived `volume × fee_bps / 10000`             | sub-block 1d                                                                              |
| `account_balances_current` (trustlines) | row data      | only native XLM populated                                | ✅ TrustLine ledger entries                       | sub-block 1e                                                                              |

Sources hardcoded `None`:

- `crates/xdr-parser/src/state.rs:468` → `tvl: None` (LP, off-chain → 0195)
- `crates/xdr-parser/src/state.rs:484-486` → `tvl/volume/fee_revenue: None` (snapshot, mixed → split)
- Audit doc `docs/audits/2026-04-10-pipeline-data-audit.md` §5.2 line 261-264 confirms

### List-endpoint schema gaps

**No new columns needed.** Initial draft proposed `assets.usd_price` + `assets.usd_price_updated_at` for future stellarchain.io/markets-style sort-by-value on `/v1/assets`. **2026-05-06 review (Karol): pulled.** Both columns and the proposed sort feature are speculative — no PM ticket, no frontend mock, no committed product goal beyond "stellarchain.io parity if/when we ever want it" (lifted from 0191 future-work bullet #6). Per YAGNI: defer columns + indexes until a real product ask materialises. Asset USD price work moves entirely to **future-work**; `0195 §2c (asset_usd_price kind)` is dropped from M2.

NFT/LP/transactions/ledgers/contracts list DTOs all map cleanly to existing columns — no schema additions needed for those either.

### Why split from sister tasks

- **vs 0195** (Lambda 2 enrichment): 0195 fills off-chain NULL columns. (Original blocker on 1a removed — 1a deleted.)
- **vs 0196** (enrichment-backfill crate): 0196 drains pre-existing un-enriched rows for fields populated by 0195 (or 0191's `assets.icon_url`). 0196 depends on 0195 having shared `enrich_*` functions ready.
- **vs 0197** (audit + docs): 0197 is the final verification — confirms every list field is in schema, indexed, and populated.

## Implementation Plan

### Sub-block 1a — REMOVED (2026-05-06)

Originally specified an atomic schema migration adding `assets.usd_price`, `assets.usd_price_updated_at`, plus 5 partial indexes. Pulled after Karol review: the columns serve speculative `sort-by-USD-price` and the indexes back speculative sort variants that no shipped endpoint uses. Both are deferred to future-work; this task no longer touches `assets` schema. Sub-blocks 1b/1c/1d/1e cover indexer-side population only on existing nullable columns.

### Sub-block 1b — Classic credit `assets.total_supply`

**Gap origin:** `crates/xdr-parser/src/extract_assets/` only emits Soroban + SAC deployments. Classic credits (USDC, EURT, etc.) reach the DB only via the `account_state` TrustLine path and never carry `total_supply`. The `total_supply` part of 0191 known gap "priority #2 classic credit enrichment" is on-chain (SUM of trustline balances) → indexer per [ADR 0043](../../2-adrs/0043_field-allocation-rule.md) (field allocation rule).

**Scope clarification (post-2026-05-06 preliminary planning audit):** classic credit `assets.name` is **OUT OF SCOPE for this sub-block**. Classic credits have no on-chain `name` field — full names like "USD Coin" come from issuer SEP-1 TOML `CURRENCIES[].name`. Per [ADR 0043](../../2-adrs/0043_field-allocation-rule.md) that's off-chain → Lambda 2 territory. Allocated to **0195 sub-block 2a (icon kind extension)** which already fetches the same TOML and can persist `name` alongside `icon_url` in a single fetch. `Sep1Currency.name` field will be added to the DTO there.

For Soroban tokens, `name` continues to be populated by task **0156** (active) — `name` from on-chain `ContractData`. SAC `name` continues to be populated by indexer at deploy time. This sub-block does NOT touch `assets.name`.

**Implementation (`total_supply` only):**

- Audit `crates/indexer/src/handler/persist/staging.rs:1234-1264` PathPayment + ChangeTrust branches
- For classic credit `total_supply`: derivable as `SUM(account_balances_current.balance) WHERE asset_code/issuer matches`. Compute incrementally on trustline writes, persist on the `assets` row.
- Depends on sub-block 1e (trustline balance extraction) being implemented — without trustline rows there is nothing to SUM.

### Sub-block 1c — `assets.holder_count` inline indexer counter

**Reactivates blocked task 0135** (`0135_FEATURE_token-holder-count-tracking`).

- Inline `+1` on `change_trust create` (new trustline), `-1` on `change_trust delete` (trustline removal), no-op on balance updates
- Edge cases: trustline-flag changes, authoreized-to-maintain-liabilities transitions — verify with audit's holder-count semantics
- One-time recount Lambda subcommand needed post-backfill — captured as Future Work, separate ops job
- Wire in `crates/xdr-parser/src/account_state.rs` and `crates/indexer/src/handler/persist/staging.rs` UPSERT path

### Sub-block 1d implementation note (2026-05-06)

**Status: landed.** Approach taken in `crates/indexer/src/handler/persist/write.rs::upsert_pools_and_snapshots`:

After snapshot rows for the current ledger are inserted, a single CTE-driven UPDATE looks up the prior ledger's snapshot per touched pool and computes:

- `volume = ABS(cur.reserve_a − prior.reserve_a)` (single-leg, conventional)
- `fee_revenue = ROUND(volume × lp.fee_bps / 10000.0, 7)`

**Swap-only filter:** the UPDATE adds `NOT EXISTS (... operations_appearances oa JOIN transactions t ... WHERE oa.pool_id = cur.pool_id AND oa.ledger_sequence = $2 AND oa.type IN (22, 23) AND t.successful = TRUE)` — excluding any pool that saw a **successful** `LiquidityPoolDeposit` (op type 22) or `LiquidityPoolWithdraw` (23) in the same ledger. Those ops move reserves but their delta is not trading volume. Failed deposit / withdraw ops still land in `operations_appearances` per Stellar semantics, so joining on `transactions.successful = TRUE` is required to avoid excluding a pool whose only deposit attempt failed (no actual state change → real swap delta on that ledger should still count as volume). Conservative semantics: a pool that mixed a successful swap with a successful deposit in the same ledger gets `volume = NULL` for that ledger rather than an inflated number. Pure-swap ledgers (the common case) populate volume correctly because reserve_a only changes when an asset crosses the pool, so reserve delta = sum of swap amounts.

First-snapshot-per-pool (no prior row) leaves both fields NULL — chart endpoints already handle NULL gracefully.

The hardcoded `volume: None, fee_revenue: None` at `xdr-parser/src/state.rs:622-624` is left in place — values are filled at the persist layer post-INSERT, not at extraction. Phase 2 (Soroban DEX adapters: Soroswap, Phoenix) remains explicit Future Work — separate task, separate PR.

### Sub-block 1d original spec (Phase 1 classic AMM)

**Phase 1 scope only — classic AMM via PathPayment ops.** Phase 2 (Soroban DEX adapters: Soroswap, Phoenix) is explicit Future Work, separate task.

Implementation:

- In `crates/indexer/src/handler/persist/staging.rs:1234` PathPayment branch, detect when path contains a `liquidityPoolId` (already extracted on line 1254 for op detail). Compute reserve delta from before/after `LiquidityPoolEntry` ledger entry change.
- Volume contribution per swap = the asset amount that crossed the pool. Increment the **live current snapshot row** for that pool (per the existing snapshot windowing logic — verify whether windowing is hourly/daily and where rollover happens).
- `fee_revenue = volume × fee_bps / 10000` computed in the same write — `fee_bps` lives on `liquidity_pools` row.
- Drop `volume: None, fee_revenue: None` hardcoding at `xdr-parser/src/state.rs:485-486`.

**Audit doc Section 9.3** (`docs/audits/2026-04-10-pipeline-data-audit.md:512-535`) originally proposed scheduled cron Lambda for both TVL **and** volume. The volume part is explicitly overridden here per the field allocation rule — volume is on-chain derivable, no oracle, no HTTP, so it belongs in the indexer. [ADR 0043](../../2-adrs/0043_field-allocation-rule.md) records this override; ADR 0032 evergreen requires `docs/architecture/indexing-pipeline/**` + `docs/audits/2026-04-10-pipeline-data-audit.md` Section 9.3 amendment in same PR.

### Sub-block 1e — `account_balances_current` trustline balances

Audit finding F7 (`docs/audits/2026-04-10-pipeline-data-audit.md`): `extract_account_states()` populates only native XLM; trustline balances are extracted nowhere despite the column existing. **Task 0119 (FilipDz, completed 2026-04-15) implemented trustline balance extraction across 4 files (+758 lines), 6 unit + 3 integration tests, with `[x]` acceptance items confirmed.** Default plan for 1e is therefore **verify-only**.

**Verify-only plan:**

- Confirm `account_balances_current` on backfill region contains non-XLM rows (sample query: `SELECT COUNT(*) FROM account_balances_current WHERE asset_type != 0`).
- Spot-check that all NOT NULL columns (`account_id`, `asset_type`, `asset_code`, `issuer_id`, `balance`, `last_updated_ledger`) are populated on the non-XLM rows.
- Confirm sub-block 1b (classic credit `total_supply` SUM) and 1c (holder_count from change_trust) can build on existing 0119 infrastructure without modification.

**Contingency (only if verify-only surfaces a regression):** if non-XLM rows are missing or NOT NULL columns are NULL on the backfill region, re-open the trustline extraction work in `crates/xdr-parser/src/account_state.rs` and `staging.rs`. This contingency is unlikely — 0119 has shipped acceptance — but keeps the sub-block honest.

### Sub-block 1f — REMOVED (ADR creation is independent of this task)

[ADR 0043](../../2-adrs/0043_field-allocation-rule.md) (field allocation rule) is **not** created inside this task. Per project policy, ADRs land independently on develop, prior to the tasks that reference them.

**Sequencing status:** ADR 0043 was merged to develop (commit `745e56b` plus template-alignment follow-up `148bf3c`) **before** this task's implementation pass landed. Tasks 0195/0196/0197 reference ADR 0043 as established law.

## Acceptance Criteria

- [x] Sub-block 1b: code shipped (`recompute_asset_aggregates` SUM(balance) per touched (code, issuer_id)). Sample-query verification on backfill region pending PR-time check. `name` for classic credits is NOT in this task's scope — see 0195 sub-block 2a (icon kind extension to also persist SEP-1 `name`).
- [x] Sub-block 1c: code shipped (`recompute_asset_aggregates` COUNT(\*) FILTER (WHERE balance > 0) — active-holder semantics). Sample-query verification + one-time recount tooling spawned as separate ops task pending.
- [x] Sub-block 1d: code shipped (post-INSERT UPDATE in `upsert_pools_and_snapshots` with prior-snapshot reserve delta + swap-only `NOT EXISTS` filter on `transactions.successful = TRUE`). Sample-query verification on backfill region pending. Phase 2 DEX adapters spawned as separate task.
- [x] Sub-block 1e: verify-only — `upsert_balances_credit` (write.rs:2119) populates all NOT NULL columns; 0119 trustline path confirmed. Sample-query spot-check on backfill pending PR-time.
- [x] ADR 0043 merged on develop (separate, independent PR landed prior to this task's review)
- [x] **Docs updated** per ADR 0032: `docs/architecture/database-schema/database-schema-overview.md` §4.10 (assets — `total_supply` / `holder_count` recompute attribution + ADR 0043 link) + §4.15 (lp_snapshots — `volume` / `fee_revenue` post-write recompute attribution + ADR 0043 link); `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` §5.2 steps 13 + 14 (recompute passes documented); `docs/audits/2026-04-10-pipeline-data-audit.md` Section 9.3 amendment — TODO before this task's PR closes (small follow-up paragraph noting volume → indexer override).
- [ ] **API types regenerated** — N/A in current scope (no DTO additions; sub-block 1a removed). Trigger if future sub-blocks touch `crates/api/**` shape.

## Future Work (out of scope, spawn separate tasks)

- **Phase 2 LP volume**: Soroban DEX adapters (Soroswap, Phoenix, etc.) — per-DEX event format, dynamic fees. Spawn after Phase 1 lands.
- **Holder_count one-time recount**: post-backfill ops Lambda subcommand to fully recount. Spawn after 1c lands.
- **Classic credit `assets.name`** moved out of this task entirely — see 0195 sub-block 2a (icon kind extended to also persist `name` from same SEP-1 fetch). Decision rationale: classic credit names are off-chain (issuer SEP-1 TOML `CURRENCIES[].name`) → Lambda 2 per ADR 0043.
- **Full Horizon-parity `total_supply`** — current MVP sums only trustline balances. Stellar protocol stores no on-chain `AssetEntry` / `AssetSupplyEntry` (10 LedgerEntry types: ACCOUNT, TRUSTLINE, OFFER, DATA, CLAIMABLE_BALANCE, LIQUIDITY_POOL, CONTRACT_DATA, CONTRACT_CODE, CONFIG_SETTING, TTL — none persists supply). Horizon `/assets` aggregates 4 sources: trustlines + claimable_balances + liquidity_pool reserves + SAC contract holdings. To match Horizon and avoid drift on popular DeFi assets (e.g. USDC w/ heavy Soroswap + SAC use, ~20-50% under-count today), a follow-on must:
  - Add **liquidity-pool reserve aggregation** — schema already in place (`liquidity_pools` + `liquidity_pool_snapshots`); SQL was prototyped in Round 4 (LATERAL on latest snapshot per pool, index-only seek via `idx_lps_pool`) and benchmarked clean. Trivial to re-land standalone.
  - Add **`claimable_balances` table + extraction** — new ledger entry type (`CLAIMABLE_BALANCE`) currently not extracted by xdr-parser. Requires new staging row + write path + DTO + canonical SQL. Rare in practice.
  - Add **per-asset SAC contract holdings tracking** — needs `contract_data` ledger entry decoding to detect `Balance(address)` storage keys per SAC contract. Most complex of the three; potentially material drift for popular DeFi assets.
  - Update `recompute_asset_aggregates` SQL to sum all 4 sources, or refactor to materialized view (see Round 5 in Implementation Journal).

## Notes

- **Branching**: cut from develop after 0191 PR merge.
- **Bundling rationale** (per Karol's "bundle related work" rule): 4 sub-blocks (1b, 1c, 1d, 1e) are heterogeneous (4 indexer sub-systems) but all share the rule "fix what indexer should have populated", same test surface. Splitting per sub-block would be 4 PRs of <100 lines each — micro-decomposition penalty exceeds review-load benefit. Sub-block 1a (speculative `usd_price` columns + indexes) was pulled per Karol review, see "List-endpoint schema gaps" above. ADR 0043 is **explicitly excluded** from this bundle and lands as its own PR off develop (governance docs land independently of code that references them).
- **0125 disposition**: superseded by 0195 sub-block 2a (LP TVL via Lambda 2). The volume/fee_revenue parts of 0125's scope move to **this** task (1d).

---

## Implementation Journal (2026-05-06)

This task evolved through several review rounds. Captured here so the chain of reasoning + measured-vs-speculated decisions remain part of the task record.

### Round 1 — Initial implementation pass

Sub-blocks landed:

- **1b** (`total_supply` for classic credit): implemented via per-ledger `recompute_asset_aggregates` in `crates/indexer/src/handler/persist/write.rs`. Approach: collect every `(asset_code, issuer_id)` pair touched by this ledger's credit-balance writes / trustline removals, run a single `UNNEST + LEFT JOIN LATERAL` UPDATE that recomputes `SUM(balance)` from `account_balances_current`. Recompute (not delta tracking) chosen because PG `ON CONFLICT DO UPDATE` cannot reliably introspect insert-vs-update on the upsert path.
- **1c** (`holder_count`): same query, `COUNT(*)` from same LATERAL.
- **1d** (`volume` + `fee_revenue`): post-INSERT UPDATE in `upsert_pools_and_snapshots` against the prior ledger's snapshot per touched pool — `volume = ABS(reserve_a_post − reserve_a_pre)`, `fee_revenue = volume × fee_bps / 10000`.
- **1e** (`account_balances_current` trustlines): verify-only — confirmed task 0119 (FilipDz, completed 2026-04-15) populates non-XLM rows in `upsert_balances_credit` with all NOT NULL columns set.

### Round 2 — Bug fixes (CodeRabbit + manual review)

Five real correctness / performance bugs found across multiple audit rounds:

1. **`holder_count` semantics**: changed `COUNT(*)` → `COUNT(*) FILTER (WHERE balance > 0)` to match the Stellar ecosystem convention used by StellarExpert / Stellarchain.io ("active holders" = trustlines with non-zero balance, not opt-ins). Block-explorer UX must agree with peer tools.
2. **LP volume swap-only filter**: original UPDATE attributed every reserve delta to volume, including `LiquidityPoolDeposit` / `LiquidityPoolWithdraw` ops. Added `NOT EXISTS` filter on `operations_appearances` for op types 22 / 23.
3. **Failed-tx leak in (2)**: `extract_operations` (in `crates/indexer/src/handler/process.rs`) emits ops regardless of `transactions.successful`. Without `successful = TRUE` join, a failed deposit attempt could mask a real swap on the same ledger. Added the join in the NOT EXISTS subquery.
4. **Partition prune miss**: `liquidity_pool_snapshots` and `operations_appearances` are RANGE-partitioned by `created_at`. Filtering by `ledger_sequence` alone forced full-partition scans. Added `created_at < $3` on the prior-snapshot CTE and `oa.created_at = cur.created_at` on the NOT EXISTS subquery so the planner can prune to a single partition + leverage `idx_lps_pool` / `idx_ops_app_pool` (both `(pool_id, created_at DESC)`).
5. **Section numbering**: my new step labelled `13b'` (apostrophe) was non-standard. Renumbered to `13c`, shifted existing `13c` (lp_positions) → `13d`, updated 2 cross-references.

Plus: `aggregates_ms` per-step timing instrumentation added to `StepTimings` + `total_ms` + log breakdown so `recompute_asset_aggregates` cost is observable in CloudWatch logs out of the box.

ADR 0043 frontmatter completed (added `0119` / `0125` / `0156` to `related_tasks`; `0033` / `0034` to `related_adrs`) for traceability of body references.

### Round 3 — Performance benchmark (`backfill-bench`, local Docker PG)

**Setup:** Docker Postgres 17.6 on port 54322, two test DBs (`backfill_bench_baseline` + `backfill_bench_changes`), schema migrated + monthly partitions provisioned via `db-partition-mgmt` CLI. Two cached pubnet partitions reused (no S3 download cost).

**Methodology:** `git stash push` → `cargo build --release -p backfill-bench` → run baseline → `git stash pop` → rebuild → run with-changes. Both runs against fresh empty DB. Per-step timings extracted from `persist breakdown` log lines.

**Range 1 — 50432000–50432499** (500 ledgers, low-activity Soroban era, near-zero credit-balance traffic):

| Metric              | Baseline | With changes | Δ                |
| ------------------- | -------- | ------------ | ---------------- |
| `total_ms` mean     | 71.4 ms  | 73.4 ms      | **+2.0 (+2.8%)** |
| `total_ms` p99      | 136 ms   | 140 ms       | +4               |
| `aggregates_ms` p99 | —        | 1 ms         | +1               |
| `pools_ms` mean     | 0.7 ms   | 2.4 ms       | +1.7             |

**Range 2 — 62016000–62017499** (1500 ledgers, late 2024, higher Soroban + LP activity):

| Metric              | Baseline | With changes | Δ                 |
| ------------------- | -------- | ------------ | ----------------- |
| `total_ms` mean     | 85.1 ms  | 88.9 ms      | **+3.8 (+4.5%)**  |
| `total_ms` p99      | 142 ms   | 146 ms       | +4                |
| `aggregates_ms` p99 | —        | 1 ms         | +1                |
| `aggregates_ms` max | —        | 3 ms         | +3                |
| `pools_ms` mean     | 1.0 ms   | 4.9 ms       | **+3.9**          |
| `balances_ms` mean  | 4.3 ms   | 4.4 ms       | +0.1              |
| Wall-clock (1500)   | 197 s    | 205 s        | **+7.8s (+4.0%)** |

**Findings:**

- `recompute_asset_aggregates` is **near-free at current pubnet trustline counts** — p99 = 1 ms, max = 3 ms even on a high-activity range.
- LP volume post-INSERT UPDATE is the dominant overhead — `pools_ms` ~5× baseline because every snapshot row triggers a CTE + LEFT JOIN UPDATE.
- Total per-ledger overhead measures **+4%**, lower than the +8% paper estimate. Storage growth: 0 (HOT updates possible — `holder_count` / `total_supply` are not indexed).

**10-year backfill projection:**

- Baseline: ~85 ms / ledger × 63M ledgers ≈ ~62 days single-thread
- With changes: ~89 ms / ledger × 63M ≈ ~65 days single-thread
- 16 parallel workers: **+5–8 hours wall-clock added across the full backfill**
- Marginal cost increase for 16-worker setup: **<$10 RDS compute over the entire backfill**

### Round 4 — Total-supply scope review (added LP reserves, then reverted to MVP)

Karol questioned whether `SUM(trustlines)` is the right source for `total_supply`. Web research against Stellar protocol XDR confirmed:

- The 10 `LedgerEntry` types (`ACCOUNT`, `TRUSTLINE`, `OFFER`, `DATA`, `CLAIMABLE_BALANCE`, `LIQUIDITY_POOL`, `CONTRACT_DATA`, `CONTRACT_CODE`, `CONFIG_SETTING`, `TTL`) contain **no `AssetEntry` / `AssetSupplyEntry`** — there is no on-chain place a per-asset supply is persisted.
- Horizon `/assets` exposes 4 separate amounts that must be summed for true total: `balances` (trustlines), `claimable_balances_amount`, `liquidity_pools_amount`, `contracts_amount`.

**First attempt:** added a second LATERAL to `recompute_asset_aggregates` that summed liquidity-pool reserves into `total_supply` alongside trustline balances. Implementation passed clippy + tests + benchmark with no measurable perf hit.

**Reverted on review:** that addition was **scope creep beyond MVP**. Task spec literally says `SUM(account_balances_current.balance) WHERE asset_code/issuer matches`. Adding LP reserves moves the column from "spec-conformant trustline SUM" to "Horizon-parity multi-source aggregate" — a different feature with different acceptance bar. Worse, it would land an _incomplete_ Horizon-parity (still missing claimable balances + SAC contract holdings) and ship known wrong-by-design numbers. Cleaner: ship the MVP spec, document the gap clearly, leave full-parity work for a properly-scoped follow-on (covered in **Future Work** below).

Implementation reverted to single-source `SUM(account_balances_current.balance)`. Documentation under "Future Work" captures the full Horizon-parity gap with concrete schema requirements. No new task spawned per Karol — kept inside 0194's Future Work bucket.

### Round 5 — Materialized-view alternative (analysed, not implemented)

The `holder_count` + `total_supply` aggregates could move to a materialized view:

```sql
CREATE MATERIALIZED VIEW assets_aggregates AS
SELECT a.id,
       COUNT(*) FILTER (WHERE abc.balance > 0) AS holder_count,
       SUM(abc.balance) AS total_supply
FROM assets a
LEFT JOIN account_balances_current abc
  ON abc.asset_code = a.asset_code AND abc.issuer_id = a.issuer_id
WHERE a.asset_type IN (1, 2)
GROUP BY a.id;
CREATE UNIQUE INDEX ON assets_aggregates (id);
-- Refresh: cron Lambda every 5 min via REFRESH MATERIALIZED VIEW CONCURRENTLY
```

**Pros:** drops `recompute_asset_aggregates` from indexer hot path entirely (saves ~0–1 ms p99 today, more as trustline counts grow). Indexer code surface shrinks. Better backfill story (zero per-ledger aggregate work).

**Cons:** adds CDK infra (cron Lambda + EventBridge rule + IAM grant), 5-minute staleness window on `holder_count` / `total_supply`, full-table refresh cost grows with asset+trustline count, additional ongoing AWS cost (~$150 / year Lambda).

**Why NOT MV for `volume` + `fee_revenue`:** `liquidity_pool_snapshots` is a per-ledger time-series table, not an aggregate. Each snapshot row has its own historic `volume`. An MV would have to mirror the table row-for-row — no compaction win. Indexer-side post-INSERT UPDATE stays.

**Decision (deferred):** measure shows current implementation is acceptable (+4% backfill, +1 ms p99 overhead). MV refactor is a **future optimisation candidate**, not a required fix. Spawn as a follow-on task only when (a) trustline counts grow enough that aggregates_ms dominates the budget, OR (b) we want indexer code surface reduction independent of cost. Option list (incremental delta tracking, threshold skip, async cron Lambda, fillfactor tuning) tracked here for future reference; MV is the strongest candidate of the five.
