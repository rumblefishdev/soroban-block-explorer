---
title: 'Design options — 6 models, red/blue verdicts, fan-out convergence'
type: synthesis
status: developing
spawned_from: '0359'
spawns: []
tags: ['data-model', 'design', 'adr-input']
links: []
history:
  - date: 2026-07-07
    status: developing
    who: karolkow
    note: 'Original proposal + independent options analysis + a 6-way red/blue-team of every modeling option against prod code, converging on a role-tagged fan-out. Feeds the ADR.'
---

# Design options & red/blue verdicts

## Governing decision (karolkow, 2026-07-07)

**NO half-measures, NO forward-only stopgaps. Fundamental fix + complete/correct
data BACKWARD too; forward-fix and backfill are one scope and the backfill IS
done.** See [[feedback_fundamental_complete_backward_data]]. That requirement, plus
the completeness ceiling (one op touches up to 7 declared assets + unbounded
result-side claim atoms — see
[R-external-cross-validation](R-external-cross-validation.md)), settles the
second-slot-vs-fan-out question: **any fixed N slot is structurally incapable of
complete data → out.**

## Fundamental design (proposed — for the ADR)

**Chosen approach: a per-(operation, participating-asset) index.** New table
`operation_asset_appearances` (name TBD), one row per (op, asset) with `asset_id`
as the **leading sort key** — symmetric with `transaction_participants` for
accounts. Native XLM keyed by the existing positive surrogate
`ids::asset_id(0,"",0,0) = cityhash64("native")` (reuse the 0331/ADR-0051
convention — do NOT invent a new native key).

**Complete asset activity = a UNION of three streams** (F-F): the classic-op
participation index (above) ∪ the asset's **SAC-contract invocations**
(`soroban_invocations_appearances` keyed by `sac_contract_surrogate` — already
indexed, complete incl. inner DEX legs) ∪ own-contract invocations for a
type-3 soroban token (already works). The design must define how the endpoint
merges/paginates these streams (single unified keyset vs tagged sources).

Consequences:

- Every asset's tx list becomes complete: payment, BOTH path-payment legs +
  path hops, BOTH offer sides, trustline ops, claimable, LP legs, native, AND
  the asset's SAC/Soroban transfer activity.
- `asset_id`-leading key → fast seeks for any asset (no non-leading-PK scan,
  no per-asset bloom index needed for the driver).
- Native is a normal asset — no special-casing in the query layer.

**Cheap near-term win (F-F, independent of the re-model / backfill):** the
asset-tx query already receives `sac_contract_surrogate` on the row but ignores
it. Adding it as an OR-branch in the existing contract predicate immediately
surfaces the classic-side SAC ops on the asset page; unioning
`soroban_invocations_appearances` for that surrogate surfaces the full
Soroban-transfer stream (native XLM: ~3.9M rows already sitting in the DB). This
can ship before the big re-model and is a strictly better native stopgap than
the reverted variant C.

**Ingestion:** emit one appearance row per participating asset of each op
(parser already has all legs — `operation.rs` sendAsset/destAsset/path, offer
buy/sell). **Backfill: re-parse operations from archived XDR** (S3, per ADR 0018) — the existing CH rows only have one leg, so re-keying is insufficient;
full re-parse is required and accepted.

**Alternative considered — array columns** (`asset_ids Array` on the existing
row, filter via `has()`, like `pool_ids`): cheaper (no new table, no row
multiplication) but `has()` is not a leading-key seek and it's a half-measure.
Rejected in favour of the participation table for a clean fundamental fix — to
be re-confirmed in the ADR.

### Independent options analysis (asked by karolkow — not deferring to Stanisław or the task)

Completeness requirement = for every op, index one participation per **(asset,
role)** it genuinely touches, native as a positive surrogate, role carried so
display is correct (a routing hop must not render as "sent X").

| #   | Option                                                                                                              | Complete?               | Correct display (role)?                           | Per-asset read perf                                                                            | Backfill                                                                         | Verdict                   |
| --- | ------------------------------------------------------------------------------------------------------------------- | ----------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- | ------------------------- |
| 1   | **Fixed N slots** (2–3 cols)                                                                                        | ❌ caps < 7, drops hops | ❌ no role                                        | fast (indexed cols)                                                                            | light (re-stage)                                                                 | **REJECT** — not complete |
| 2   | **Array column(s)** `asset_ids Array` (+ parallel `asset_roles`) on the existing row                                | ✅ arbitrary N          | ⚠️ parallel arrays positionally coupled → fragile | ⚠️ `has()` not a leading-key seek → bloom skip-index; degrades on hot assets (USDC = billions) | medium (re-write same rows, RMT key unchanged)                                   | viable-but-against-grain  |
| 3   | **Fan-out table** `operation_asset_appearances(asset_id, role, ledger, tx, app_order, …)`, `ORDER BY (asset_id, …)` | ✅ arbitrary N          | ✅ role is a first-class column                   | ✅ asset-leading key → O(log) seek + contiguous scan; fast feed/COUNT/latest-N                 | heavy (full XDR re-parse into multiplied table; ~10–15 B rows, > soroban_events) | **RECOMMEND**             |

**Recommendation: Option 3 — fan-out `operation_asset_appearances`, one row per
(op, asset, role).** It is the only model that is simultaneously complete (N up to
7), correct (role column disambiguates hop / endpoint / sold / bought / trustline
/ escrowed / released / admin — so no misleading display), fast on the core
asset-page read path (asset-leading sort key), and symmetric with the codebase's
proven `transaction_participants` pattern. The task already chose fan-out but
justified it only as "cleaner"; the sharper, independent reasons are **role-carry,
leading-key seek, and the 7-asset ceiling** — array (Option 2) is complete but
loses role integrity and the seek. Row blow-up + heaviest backfill are the price;
per the decision above (backward completeness always, backfill accepted) that
price is in-scope, so the only real objection to fan-out (Stanisław's cost
argument) is explicitly overridden by requirement.

**Two deeper completeness points fan-out must also nail (beyond the slot question,
required for "truly complete/correct"):**

1. **Claimable-balance claim asset is NOT in the op body.** `ClaimClaimableBalance`
   / `ClawbackClaimableBalance` carry only `balanceId` (`operation.rs:352`) → the
   asset must be resolved by joining the CB-id back to its `CreateClaimableBalance`
   (a claimable_balances lookup). Sub-task; without it, claim/clawback are
   asset-blind even under fan-out.
2. **Result-side is authoritative, not just the declared op body.** The `path[]` in
   the op body is the INTENDED route; the assets actually crossed live in the
   result `ClaimAtom`s (order-book + LP crossings — parser already extracts LP
   atoms into `pool_ids` and offer claim atoms). Truly complete participation
   should union assets from the **result claim atoms**, not only the declared legs.
   This is the completeness ceiling above "parser already has all legs."

**Updated recommendation (unchanged verdict, sharper reasons): Option 3 fan-out
`operation_asset_appearances(asset_id, role, …)` with a `role` enum** —
`sent` / `received` (path-payment + payment endpoints), `sold` / `bought` (offers),
`traded` (result claim atoms — this is where hops live), `trustline`, `escrowed`,
`released`, `admin`, `lp_a` / `lp_b`. The role column lets the asset-page query
choose which grains to surface (endpoints + trades by default; declared-path never
needed as its own attribution). Fixed slots (Option 1) can't hold the unbounded
trade grain; arrays (Option 2) can hold it but lose role integrity and the seek —
so **fan-out with role is the only complete + correct + queryable model**, matching
the reference stack on endpoints and going one correct step further by making the
trade grain first-class per-asset (which even Hubble leaves un-indexed).

## Red/blue-team verdicts on all six options (2026-07-07)

Six modeling options were each handed to an independent adversarial agent
(blue-team steelman → red-team attack → calibrated verdict), grounded in prod
code. This supersedes the single-option "RECOMMEND fan-out" pass above.

| #   | Option                                   | Verdict                           | Decisive finding (code-grounded)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --- | ---------------------------------------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Fixed N slots (2–3 cols)                 | **DIES**                          | Result-side `offers_claimed` is an unbounded XDR vec (`operation.rs:100-118`) — no fixed N holds it. Real prod scar: OR-across-slots already defeated the skip-index → ~6.2 B-row / 115 GiB / 7.5 s full scan that blew `api_throttle.read_rows` (CH Code 201, zkSync, 2026-06-29; `init.sql:558-588`).                                                                                                                                                                                                                                                                                                                             |
| 2   | Array `asset_ids` + `has()` + bloom      | **CONDITIONAL** — secondary only  | `has()` is not a leading-key seek; the identical `pool_ids` case is box-measured at a **6.75 B-row scan** for a popular key (`liquidity_pools/queries_ch.rs:489-492`, 2026-06-17). Bloom prunes ~0% of granules for a hot asset (USDC). A parallel `asset_roles` array desyncs the RMT identity fold (`stage.rs:935-936`) → duplicate rows that never collapse. Dies as the primary hot-asset index; fine as a completeness-carrying secondary column (its `pool_ids` role).                                                                                                                                                        |
| 3   | Fan-out fat (payload inlined)            | **CONDITIONAL**                   | The correct generalization of `transaction_participants` (`init.sql:594-601`) — survives ONLY if (a) the RMT sort key includes `role` **and** a re-parse-deterministic `leg_index` (else multi-asset legs silently dedup-collapse = under-count, or backfill re-parse fails to collapse = double-count, invisible until diffed vs Horizon), (b) `amount` fold-count (`stage.rs:975`) is dropped from this table, (c) the ×2–4 row / merge budget (~15–25 B rows, larger than any current table) is accepted. Read endpoint STILL unions SAC/invocations/trades — fan-out only cleans the classic arm.                               |
| 4   | Fan-out thin (pointer + join)            | **CONDITIONAL**                   | Same asset-leading seek + row count as #3 but ~10× less storage (~0.1–0.15 TB vs ~1–1.3 TB on 15 B rows). The join-back is NOT a weak distributed JOIN — it is a bounded PK-prefix seek that **already ships** (`common/ch.rs:112-143`, documented "primary-key seek"). Survives iff the asset feed renders **transaction-summary** rows; dies if it needs **per-participation op-level payload** (join becomes a wide non-collapsible scatter → fat #3 wins).                                                                                                                                                                      |
| 5   | Union-of-streams (no single table)       | **CONDITIONAL** — enrichment only | The 3-arm UNION already ships (`transactions/queries_ch.rs:414-434`) but ONLY works single-partition-scoped; an asset spans ~112 partitions with **4 incompatible sort keys** (ops `ledger`-leading, both SAC arms `contract_id`-leading, trades TBD) → a time-ordered merge can't early-terminate, and `COUNT(participations)` is undefined across grains (dedup-to-tx undercounts, no-dedup triple-counts). The trades stream it needs **doesn't exist and must be built anyway** = option 4. Dies as the primary model; survives as the L2 read-time enrichment layer over a real fan-out.                                       |
| 6   | Hybrid: 2-slot endpoints + trades stream | **CONDITIONAL**                   | Op-participation IS bounded ≤2 for declared legs (enumerated over every `operation.rs` arm) — EXCEPT `ChangeTrust`-PoolShare, which declares assetA + assetB + pool = 3 entities (`operation.rs:509`). But the trades stream it delegates to IS a fan-out, so #6 = option 1 + option 4 bolted together = **strictly more moving parts** (two write paths, two backfills, cross-grain merge-paginate) than one unified role-tagged fan-out. Survives only if the product wants two separately-paginated surfaces (Operations tab + Trades tab); a single unified "activity" feed → it collapses into #5's merge complexity and DIES. |

### Convergence

Every non-dead option secretly requires a **fan-out** for the unbounded result
side (#5 and #6 both admit they must build the trades table = option 4). So:

- **Core (mandated): one unified, `role`-tagged fan-out** — one row per
  (op, asset, role), RMT key
  `(asset_id, ledger_sequence, transaction_id, application_order, role, leg_index)`
  where `leg_index` is derived **deterministically across live-parse and
  S3-backfill re-parse**. Carries BOTH grains: declared endpoints
  (`sent`/`received`/`sold`/`bought`/`trustline`/`escrowed`/`released`/`admin`/`lp_a`/`lp_b`)
  AND result-side crossings (`traded`). This absorbs #6's trades stream and its
  two-tabs presentation (a `role` filter, not a second table), plus the unbounded
  crossings.
- `amount` fold-count is **dropped** from this table (it corrupts under fan-out).
- **#5 union survives as the L2 enrichment layer only** — union SAC
  `soroban_invocations_appearances` + decoded `soroban_events` token flow on top of
  the fan-out (a separate Layer-2 workstream, not a replacement).
- **#2 array** allowed only as a secondary completeness column, never the primary
  seek. **#1 fixed slots dead.**

### Fat vs thin (#3 vs #4) — deferred; thin is the lean default

Same table, differ only in payload width. Decision driver = whether the
asset-page feed renders transaction-summary rows (→ thin, join-back already proven
cheap in `common/ch.rs:112-143`) or per-participation op-level detail (→ fat).
**Default thin** unless the render spec (Figma) needs per-leg payload. Resolve
against the asset-page design before the ADR.

### Open ADR decisions (surfaced by the red-team)

> **Resolution status (2026-07-08, post devils-advocate — no separate ADR):**
> (1) `leg_index` → specced content-addressed (fixed XDR-order enumeration + shared
> lib + differential test), see [G-schema-and-roles](G-schema-and-roles.md);
> (2) fat vs thin → **THIN** (frontend renders a tx-summary); (3) ChangeTrust-
> PoolShare → **resolved** (2 asset rows `lp_a`/`lp_b` + a `pool_id` column; the
> pool is its own already-indexed dimension); (4) claimable-claim asset → from the
> same-op `LedgerEntryChanges` (meta), not a CB-id join; (5) row/merge budget →
> bounded to the **Soroban era** (~13 M ledgers, min ledger 50,457,424), exact
> sizing still to confirm on FINAL. The items below are the original framing.

1. **`leg_index` determinism across re-parse** — the killer-or-saver for
   correctness. Must be bit-identical between live ingest and S3 backfill or RMT
   double/under-counts. Design the derivation explicitly (mirror the `pool_ids`
   sort+dedup determinism trick, `stage.rs:935-936`).
2. **Fat vs thin payload** — gate on the asset-page render spec (tx-summary vs
   per-leg).
3. **`ChangeTrust`-PoolShare (3 entities: assetA+assetB+pool)** — how to key
   (two `lp_a`/`lp_b` rows + the pool identity).
4. **Claimable-claim asset resolution** — `ClaimClaimableBalance` / `Clawback`
   carry only `balanceId` (`operation.rs:352`); resolve the asset via a CB-id →
   `CreateClaimableBalance` join.
5. **Row / merge budget sign-off** — ~15–25 B rows, a new RMT larger than
   `operations_appearances`; confirm storage + backfill window.
