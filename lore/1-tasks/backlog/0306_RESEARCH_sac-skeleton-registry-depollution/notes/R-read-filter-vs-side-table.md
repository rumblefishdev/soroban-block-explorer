# R — SAC-skeleton registry de-pollution: read-filter vs side-table

> Research spike, 2026-06-18 (agent dispatch, repo-as-interpretation +
> prod `chq`). Status: mature. Spawned from 0294 Step 3. Decision-ready.

## Problem restated

`soroban_contracts` (ClickHouse, `ORDER BY (contract_id)`, RMT) holds **424,220**
rows (verified, `chq … FINAL`). Of these, **307,247** are SAC *skeleton*
placeholders (`is_sac=true`, `contract_type=0` Token, `coalesce(deployed_at_ledger,0)=0`,
no deployer, `wasm_uploaded_at_ledger=0`, `name` NULL), forward-derived
one-per-classic-asset by the indexer stage (`crates/db-clickhouse/src/persist/stage.rs:556-596`,
tasks 0218/0220/0221). Only **3,906** rows are real deployed SACs
(`is_sac=true AND deployed_at_ledger!=0`); total SAC rows = **311,153**.
`GET /v1/contracts` exposes ALL of them (`fetch_contract_list`,
`queries_ch.rs:106` / `queries.rs:106`), an ~80× inflation. The de-pollution
must NOT reopen the 0221 event-leak: skeletons supply the `Token` routing
verdict that drops un-deployed-SAC NFT-shaped events away from `nfts_pending`.

**C2 sentinel confirmed:** `deployed_at_ledger=0` = 236,199 rows;
`deployed_at_ledger IS NULL` = 76,655; together (`coalesce…=0`) = 307,247. The
NULL-only predicate undercounts by 76,655 — **always use
`coalesce(deployed_at_ledger,0)=0`**. (NB: the 0294 README's "1,297 / 5,607"
figures are the *orphan* set in Step 2, a DIFFERENT population from these SAC
skeletons; the skeleton split here is 76,655 NULL vs 236,199 `=0`.)

## Option A — read-filter on the contracts-list query

**How.** Add one predicate to `fetch_contract_list` in BOTH backends; skeleton
rows STAY in `soroban_contracts`.

- CH (`crates/api/src/contracts/queries_ch.rs:140`, the
  `WHERE 1{cursor_clause}{type_clause}{q_clause}` line): append
  `AND NOT (sc.is_sac AND coalesce(sc.deployed_at_ledger, 0) = 0)`.
- PG (`crates/api/src/contracts/queries.rs:106` builder, after the
  `contract_type` push at line 116-118): same predicate.

**Counts / pagination / `filter[type]` — verified safe:**

- The list response has **NO total-count**. It is purely keyset-paginated:
  `finalize_page(&mut rows, pagination.limit, …)` + `ContractIdCursor{id}`
  (`handlers.rs:105-111`), `ORDER BY sc.id DESC`. No `COUNT(*)` over the list to
  skew. (The only `COUNT(*)` in `queries.rs` is the per-row *invocations*
  subquery at line 78/194 — unrelated.)
- **Cursor-safe:** the cursor keys on `sc.id`; a WHERE predicate narrows rows
  but never changes `id` ordering, so `has_more`/peek-`+1` logic is intact.
- **`filter[type]=token` preserved:** resolves to `sc.contract_type = 0`
  (`handlers.rs:77-82` → `contract_type_name` 0="token"). Real SACs are also
  `contract_type=0`. Verified: of 311,153 `contract_type=0` rows, the
  skeleton-exclusion predicate keeps exactly **3,906** — every real deployed
  SAC, zero skeletons. Exactly the desired semantic.

**0221 impact: NONE.** `query_contract_verdicts` (`persist.rs:380-383`) still
reads the skeleton rows in place; the verdict map is unchanged. Zero leak risk.

**Pros:** ~6 lines, two files; no migration; no write-path change; 0221 guard
untouched (283's "strictly safer" verdict holds). Reversible. Freshness instant.
**Cons:** Cosmetic only — the 307k rows still exist on disk and still appear in
`/v1/contracts/:id` detail, search, etc. if someone has the strkey. Two query
sites to keep in sync (PG + CH) + the API-types/docs SQL fixtures.
**Risk: LOW.** Subtlety: once 0294 Step 2 flips ~5,607 orphans to `is_sac=true`
(still `deployed_at_ledger=0` via the `wasm_uploaded_at_ledger=0` override
path), those ALSO match the predicate and get correctly hidden — intended.

## Option B — side-table root-fix

**How.** Move the SAC routing-verdict rows OUT of `soroban_contracts` into a
side-table (e.g. `sac_verdicts(contract_id, contract_type)`). `soroban_contracts`
retains only real deployed contracts + real deployed SACs. Requires: (1) a
migration/rebuild to split the table (stage off the `contract_type_rebuild` /
`EXCHANGE TABLES` machinery), (2) change the indexer stage so skeleton/override
rows write to the side-table instead of `out.contract_rows` (`stage.rs:566-597`),
and (3) **repoint `query_contract_verdicts`**.

**The mandatory `query_contract_verdicts` repoint (C3 guardrail).**
`persist.rs:380-383` is
`SELECT contract_id, contract_type FROM soroban_contracts FINAL WHERE … contract_type IN (0,2,3)`.
Its result feeds `prior_contract_verdicts`, consumed by `route_for`
(`stage.rs:1098-1110`): `Token`/`Fungible` → `NftRoute::Drop`; *no verdict* →
`NftRoute::Pending`. If skeletons leave `soroban_contracts` without repointing,
the lookup misses → un-deployed-SAC NFT-shaped events fall to **Pending →
instant 0221 re-leak** (empirically 26.75% of `nfts_pending`, 2.45M rows at the
512k pilot). Fix: `UNION` the side-table into the query, or keep a verdict-only
projection. **This is the ONLY routing reader** — `prior_contract_verdicts` is
populated solely by `fetch_prior_contract_verdicts` → `query_contract_verdicts`
(`persist.rs:316-394`).

**Other-consumer impact — VERIFIED AGAINST PROD, NOT a blocker (overturns the
initial code-audit fear).** An initial read flagged that assets/LP/search/NFT
joins (`ON sc.id = a.contract_id`) would go NULL under Option B. Prod CH refutes
this:

- For all **3,765** SAC assets (`asset_type=2`), `assets.contract_id` resolves
  to a `soroban_contracts` row — and in **0 of 3,765** cases is that row a
  skeleton; all 3,765 resolve to real deployed-SAC rows (`is_sac=true,
  contract_type=0, deployed_at_ledger SET, has_deployer`).
- Skeletons referenced by any `assets` FK: **0**. By any `nfts` FK: **0**. In
  `soroban_invocations_appearances`: **2** (negligible).

So the ~307k skeletons are **orphan placeholders nothing joins to via surrogate
FK**. Asset/LP/search SAC labels join real deployed-SAC rows, which STAY in
`soroban_contracts` under Option B. The initial blocker was a misread of the FK
target.

**Pros:** Root fix — registry genuinely clean everywhere (detail, search,
counts); no per-read predicate across PG+CH+docs.
**Cons:** Migration + table split (RMT `EXCHANGE TABLES`, 0281 window); indexer
write-path change in the hot stage; the `query_contract_verdicts` repoint is
correctness-critical. The stage's "pure function" testability is touched (the
override emit moves).
**Risk: MEDIUM** — concentrated almost entirely in the `query_contract_verdicts`
repoint. The FK-orphan evidence removes the consumer-breakage risk.

## Other consumers audit

| Consumer | file:line | Needs skeleton present? | Blocks A? | Blocks B? |
|---|---|---|---|---|
| Contracts LIST | `queries_ch.rs:106`, `queries.rs:106` | rows ARE the pollution | target | target |
| `query_contract_verdicts` (0221 guard) | `persist.rs:380` | **YES** (only legit consumer) | NO (left in place) | **YES → repoint (C3)** |
| Contracts detail/interface | `queries_ch.rs:226,338` | no | NO | NO |
| Assets list/detail | `assets/queries_ch.rs:100` | **NO** (0/3,765 skeletons, prod) | NO | NO |
| Liquidity-pool SAC labels | `liquidity_pools/queries_ch.rs:178,878` | NO (same FK target) | NO | NO |
| Search (asset/nft hits) | `search/queries.rs:184,225` | NO (real rows) | NO | NO |
| NFTs list/detail | `nfts/queries.rs:72,137` | NO (0 skeletons in FK) | NO | NO |
| Tx/ledger contract aggregation | `transactions/queries_ch.rs:744` | NO (real rows) | NO | NO |
| NFT URI enrichment | `enrichment-shared/.../nft_token_uri.rs:72` | NO (NFTs aren't skeletons) | NO | NO |
| backfill rebuild / nft_reclassify | `backfill-runner/*` | NO | NO | NO |
| db-merge parity diffs | `db-merge/src/diff/*` | informational | NO | maybe add side-table to parity (low pri) |

**Bottom line:** Nothing blocks A. The ONLY Option-B blocker is the
`query_contract_verdicts` repoint (C3) — a known single-query fix. The
asset/LP/search "breakage" is disproven by prod.

## 0221 re-validation test design (runnable)

Strictly required for Option B (A leaves the guard intact).

**Unit-level** (`stage.rs` tests, fast): call `prepare_with_sac_overrides` with
one transfer-shaped `ExtractedNftEvent` whose `contract_id` is a known
un-deployed SAC (derive via `xdr_parser::sac::derive_sac_contract_id` for e.g.
`Asset("WGUARDIAN","GBYBVWOO…GUARD")`); do NOT include it in this ledger's
`contract_deployments`/`sac_overrides` (reproduce the cross-ledger miss); supply
the verdict via `prior_contract_verdicts: {C… → Token}` (the value the repointed
query must now return). **Assert** `out.nft_pending_rows` / `nft_ownership_pending_rows`
EMPTY for that contract (routed `Drop`). Negative control: empty
`prior_contract_verdicts` → the same event lands in pending.

**Integration-level** (`crates/db-clickhouse/tests/`, `#[ignore]`, needs CH):
seed the **side-table** with `(C…, contract_type=0)`, leave `soroban_contracts`
WITHOUT that row (post-migration world); call `fetch_prior_contract_verdicts`,
assert it returns `{C… → Token}` (**this is the assertion that fails if the
repoint is wrong** — the precise 0221 regression); then drive
`persist_ledger_clickhouse` and assert
`count() FROM nfts_pending WHERE contract_id = ids::contract_id(C…)` = 0.

## RECOMMENDATION — phased: A now, B as the root-fix

1. **Ship Option A now** (~6 lines, zero migration/write-path/0221 risk; fully
   satisfies "`/v1/contracts` no longer polluted"). De-pollutes the public
   surface today, de-risks pre-launch (0257 audit) without a maintenance window.
2. **Then Option B** as the Step-3 root-fix, bundled with 0294 Step 2 in the
   0281 window. The prod FK-orphan finding makes B much cheaper than feared (no
   consumer breaks); residual risk = the `query_contract_verdicts` repoint,
   pinned by the integration test. Once B lands, A's predicate is redundant.

**Why phased not B-only:** A buys the user-visible win immediately; B is
migration-gated and shares a window.

**100%-certainty caveats:**

- CERTAIN (code + prod): counts (424,220 / 307,247 / 3,906); skeletons are
  FK-orphans (0 in assets/nfts FK, 2 invocations); list has no total-count and
  is `id`-keyset paginated; `filter[type]=token` keeps exactly 3,906 real SACs;
  `query_contract_verdicts` is the sole routing reader + sole Option-B blocker.
- CERTAIN (code-read): Option A leaves the 0221 guard untouched.
- UNVERIFIED / needs a prod run: the side-table migration mechanics (RMT split
  via `EXCHANGE TABLES`) — size before the window; the 2 skeleton rows in
  `soroban_invocations_appearances` (not individually decoded); whether
  `db-merge` parity diffs need the side-table wired (low pri).

## Open questions

1. **Step ordering with 0294 Step 2.** Step 2 flips ~5,607 orphans to
   `is_sac=true, contract_type=Token` (still `deployed_at_ledger=0`). Under A
   those auto-hide; under B decide whether the flip writes to `soroban_contracts`
   (then swept into the side-table by the split) or directly to the side-table.
   Sequence Step 2 → Step 3 explicitly.
2. **Side-table FINAL cost.** A dedicated `sac_verdicts` RMT (`ORDER BY
   contract_id`) shrinks the verdict read from 424k to ~311k; confirm the
   UNION's read-cost on a prod smoke.
3. **PG path parity.** Both options mirror PG (`queries.rs`) + CH; the
   API-types/docs SQL fixtures under
   `docs/architecture/database-schema/endpoint-queries[-clickhouse]/11_…` must
   be updated in the same PR (CLAUDE.md evergreen-docs gate).

## Sources

**Code (file:line):** contracts list CH `queries_ch.rs:106-208` (WHERE `:140`,
no COUNT, keyset `ORDER BY sc.id`); PG `queries.rs:106-137` (the only `COUNT(*)`
is the per-row invocations subquery `:78`); list handler/pagination
`handlers.rs:72-117` (`finalize_page` `:105`, `ContractIdCursor{id}` `:110`,
`filter[type]→contract_type` `:77-82`); 0221 guard `persist.rs:316-394`
(`query_contract_verdicts` `:380-383`); routing `stage.rs:1064-1110`
(`route_for` `:1098`, `Token|Fungible→Drop` `:1104`); SAC-skeleton write
`stage.rs:556-597`.

**chq (prod, cheap COUNTs):** total 424,220; skeletons 307,247; real deployed
SAC 3,906; all SAC 311,153. Sentinel: `=0` 236,199 / NULL 76,655. SAC assets
(`asset_type=2`) 3,765 → all FK-resolve to real SAC rows, **0** to skeletons.
Skeletons in `assets` FK: 0; in `nfts` FK: 0; in
`soroban_invocations_appearances`: 2. `filter[type]=token`: 311,153 → 3,906
survive skeleton-exclusion (0 real SACs dropped).
