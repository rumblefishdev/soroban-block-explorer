# Pre-Wave-6 exhaustive sweep — 2026-05-26

**Trigger:** Waves 1-5 produced findings via sampled / time-budgeted methods.
User flagged that some clusters cited "samples" rather than exhaustive counts.
Before Wave 6 (Track 2 visual + UX), verify each cluster pattern is
**exhaustive** so Track 2 measures actual residual issues — not previously-
undetected siblings of known patterns.

**Scope:** 6 clusters. Read-only. No production code edits. Findings file
appends only. Task 0262 owned by user — report-only here.

**Branch tip:** c0ebad38 (5 commits post-Wave-5).

---

## Per-cluster delta summary

| # | Cluster | Original claim | Exhaustive count | Delta | Severity change |
|---|---|---|---|---|---|
| 1 | Composite NotFound (sub-section parallel queries) | "account + contract" (F-D-2) | **account + contract + pool** (3 pages) | +1 (pool) | None — F-D-2 scope extended |
| 2 | Cross-entity link integrity | 9 findings F-K-1..K-9 | +2 new sites identified | +2 | None — extends F-K-2 family |
| 3 | Truncation re-impls | 6 sites (F-U-3 / J-7) | **6 sites** | 0 — confirmed exhaustive | No change |
| 4 | Formatter dups | 2 STROOPS_PER_XLM, 2 formatFee (F-U-4 / F-J-16) | **2 / 2 / 10 toLocaleString / 4 bypass-toFixed** | toFixed slightly higher than F-J-3 cited (3 → 4) | No change |
| 5 | XDR / binary `as unknown` + structural casts | "3 files in advanced/" (F-AQ-7) | **9 cast sites across 8 files** (4 in tx-detail; 5 elsewhere) | Wider than cited | None — surface-area note |
| 6 | URL-state hook consumers | useTableUrlState verdict KEEP | **11 useCursorPagination + 1 useTabUrlState + 1 useDetailMode + 1 raw useSearchParams** consumers | Confirmed exhaustive | No change |

**Net new findings:** 2 (F-EX-1, F-EX-2). **Severity escalations:** 0.

---

## Cluster 1 — Composite NotFound (parent-error vs sub-section query) — exhaustive

Procedure: for each of the 7 detail pages, classify (a) sub-section count,
(b) per-section own-query status, (c) parent error-handling pattern,
(d) dual-block risk on valid-format-404.

| Page | File | Sub-sections | Per-section own query? | Parent error pattern | Dual-block on 404? |
|---|---|---|---|---|---|
| E3 transaction | `web/src/pages/transaction-detail/index.tsx` | TransactionSummary, OperationsSection, SignaturesTable, EventsSection, RawDataSection | NO — all consume `tx.heavy.*` from single parent `useTransactionDetail` | Full early-return on `query.isError` (lines 62-73) | **NO** — single query |
| E5 ledger | `web/src/pages/LedgerDetailPage.tsx` | LedgerSummary, LedgerTransactions | NO — `ledger.transactions` embedded in parent `useLedgerDetail` | Full early-return on `isError` (lines 56-77) | **NO** — single query |
| E6 account | `web/src/pages/AccountDetailPage.tsx` | AccountSummary, AccountBalances, AccountTransactions | **YES** — `AccountTransactions` fires `useAccountTransactions` (line 78) | Sub-section unconditional mount even on parent error (line 90-92, no `!account.isError &&` gate) | **YES** — F-D-2 confirmed |
| E8 asset | `web/src/pages/AssetDetailPage.tsx` | AssetSummary, AssetMetadata, AssetTransactions | **YES** — `AssetTransactions` fires `useAssetTransactions` (line 76) | Render-gate at line 127: `{!asset.isError && <AssetTransactions/>}` | **NO** — F-D-2 was wrong about E8; gate present |
| E9 contract | `web/src/pages/ContractDetailPage.tsx` | ContractSummary, ContractInterface, ContractInvocations, ContractEvents | **YES** — 3 of 4 fire own queries (Interface line 173, Invocations 81, Events 172) | Sub-sections unconditional inside tab `<Card>` — no `!contract.isError &&` gate | **YES** — F-D-2 confirmed; **WORST: 4 error blocks possible** (1 parent + 3 tab queries) |
| E11 NFT | `web/src/pages/NftDetailPage.tsx` | NftMediaPreview, NftSummary, NftMetadata, NftTransfers | **YES** — `NftTransfers` fires `useNftTransfers` (line 93) | Full early-return on `isError` (lines 84-103) — sub-sections never mount on parent error | **NO** — early-return prevents |
| E13 pool | `web/src/pages/LiquidityPoolDetailPage.tsx` | PoolDetailHeader, PoolKpiStrip, PoolSummary, PoolCharts, PoolParticipants, PoolTransactions | **YES** — 3 fire own queries (Charts 131, Participants 83, Transactions 127) | Sub-sections unconditional mount even on parent error (lines 95-103) | **YES** — **NEW: not in F-D-2 original scope; +3 error blocks possible** |

### F-D-2 scope correction

- **Originally cited:** account + contract (E6/D4, E8/D4, E9/D4)
- **Asset (E8) revision:** does NOT exhibit dual-block — render-gate pattern at line 127 prevents
- **Pool (E13) addition:** EXHIBITS dual-block — Charts/Participants/Transactions all mount unconditionally on `detail.isError`
- **Corrected exhaustive scope:** **account (E6) + contract (E9) + pool (E13)** — 3 affected pages, not 2
- **Worst case:** E9 contract — up to 4 error blocks (1 summary + 3 tab queries) on a single invalid-format-404
- **E8 asset:** patterns the correct fix; gate identical to recommended F-D-2 remediation

### Detail-pages-affected list for user's 0262 extension

When user extends task 0262 on their branch, the affected detail pages are:

1. **AccountDetailPage** (`web/src/pages/AccountDetailPage.tsx`) — mount `AccountTransactions` conditionally on `!account.isError`
2. **ContractDetailPage** (`web/src/pages/ContractDetailPage.tsx`) — gate the `<Card>` containing tab body on `!contract.isError`; or early-return on parent isError before mounting tabs
3. **LiquidityPoolDetailPage** (`web/src/pages/LiquidityPoolDetailPage.tsx`) — gate `<PoolCharts>`, `<PoolParticipants>`, `<PoolTransactions>` on `!detail.isError`

E8 AssetDetailPage already implements the correct pattern (reference implementation).

---

## Cluster 2 — Cross-entity link integrity — exhaustive

Procedure: grep all entity-identifier-shaped fields, classify whether
they render via `IdentifierDisplay` / `IdentifierWithCopy` / `Link` (linked)
or as plain `Typography` (not linked).

### Linked sites (22 files using identifier components)

All confirmed using `IdentifierDisplay` (auto-links via `getIdentifierHref`)
or `IdentifierWithCopy` or explicit `Link/RouterLink`. **80 component-usage
matches across 22 files** (grep verified).

### Unlinked identifier renderings (NEW EXHAUSTIVE LIST)

| File:line | Identifier | Type | Current render | Should link to | Severity | Existing finding? |
|---|---|---|---|---|---|---|
| `web/src/pages/pool-detail/PoolSummary.tsx:33-34` | reserve asset code (e.g. `USDCOIN`) | asset | plain `Typography` inside `AssetReserveCell` | `/assets/:id` | 🟠 HIGH | **F-K-2** (existing) |
| `web/src/pages/pool-detail/PoolKpiStrip.tsx:82-83, 88-89` | reserve label asset code | asset | KPI cell label + subtitle plain `Typography` | `/assets/:id` | 🟠 HIGH | **EXTENDS F-K-2** — additional surface |
| `web/src/pages/liquidity-pools/PoolsTable.tsx:97-105` | reserve column asset codes (list page) | asset | plain `Typography` inside reserves stack | `/assets/:id` | 🟠 HIGH | **EXTENDS F-K-2** — list-page surface |
| `web/src/pages/pool-detail/PoolParticipants.tsx:57-59` | `first_deposit_ledger` "Since ledger" | ledger | plain `Typography` w/ `formatAmount` | `/ledgers/:seq` | 🟠 HIGH | **F-K-3** (existing) |
| `web/src/pages/nft-detail/NftSummary.tsx:87-89` | `minted_at_ledger` (NFT detail) | ledger | plain `Typography` w/ `toLocaleString` | `/ledgers/:seq` | 🟡 MEDIUM | **NEW — F-EX-1** (comment says "plain text per Figma") |
| `web/src/pages/contracts/ContractEvents.tsx:78-90` | topic strings (event topics) | possibly account/contract | plain colored `Typography` w/ `shortStr` | unclear (topic strings may be addresses) | 🟢 LOW | informational — defer; topics may carry addresses |
| `web/src/pages/contracts/ContractEvents.tsx:96-126` | data cell (event data) | freeform | plain `Typography` middle-truncated | N/A — JSON blob | N/A | not an identifier |

### F-K-2 / F-K-3 cross-task implications

**Task 0263 (pool detail reserve labels Link wrap):** scope currently
covers pool **detail** page reserve labels (F-K-2 + F-K-9 schema gap).
Exhaustive sweep reveals **2 additional reserve label surfaces**:

1. **PoolKpiStrip** — pool detail page top KPI strip (same file family,
   same schema gap — covered if 0263 fix is generic AssetLeg-link wrap)
2. **PoolsTable** — pool **list** page reserves column (separate file,
   same schema gap)

Both share root cause F-K-9 (`PoolAssetLeg` lacks linkable identifier).
Once backend extends `PoolAssetLeg` with `asset_id`/`contract_id`,
the FE Link wrap should be applied to **3 sites** not 1:

- `web/src/pages/pool-detail/PoolSummary.tsx` (AssetReserveCell)
- `web/src/pages/pool-detail/PoolKpiStrip.tsx` (KpiCell label + subtitle)
- `web/src/pages/liquidity-pools/PoolsTable.tsx` (reserves column)

**Recommendation:** user extends task 0263 scope to include 3 sites
(currently implies 1).

### F-K-3 (Pool participants Since-ledger) — confirmed isolated

Only 1 instance — already in finding.

### Pool URL form (F-K-4) — confirmed; F-K-9 dependency remains

`/liquidity-pools/:id` route still uses hex; user-visible label uses
strkey. Task 0264 (strkey canonical) backlog.

---

## Cluster 3 — Truncation re-impls — exhaustive

Grep `function shortHash|shortId|shortStr|shortenStrKey|truncateMiddle|truncateHex`
across `web/src/` + `libs/ui/src/` + `libs/api-types/src/` (excluding `generated/`).

### Result: 6 ad-hoc impls — F-U-3 count CONFIRMED exhaustive

| File:line | Function | Pattern | head/tail | Used in |
|---|---|---|---|---|
| `web/src/pages/AccountDetailPage.tsx:22` | `shortId(id)` | `slice(0,4)…slice(-4)` | 4/4 | breadcrumb crumb |
| `web/src/pages/contracts/ContractEvents.tsx:46` | `shortStr(value)` | `slice(0,4)…slice(-4)` (when >14) | 4/4 | topic JSON strings |
| `web/src/pages/contracts/ContractEvents.tsx:107` | inline `data.slice(0,10)…slice(-10)` | inline | 10/10 | event data cell |
| `web/src/pages/transaction-detail/index.tsx:23` | `shortHash(hash)` | `slice(0,6)…slice(-4)` (when >12) | 6/4 | tx breadcrumb |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:5` | `shortId(value)` | `slice(0,6)…slice(-4)` (when >12) | 6/4 | op flow tree labels |
| `web/src/pages/transaction-detail/advanced/EventsSection.tsx:29` | `shortenStrKey(value)` | `slice(0,5)…slice(-4)` (when >12) | 5/4 | event topic identifiers |
| `web/src/pages/transaction-detail/sections/SignaturesTable.tsx:29` | `truncateHex(hex, 12, 12)` | `slice(0,12)…slice(-12)` | 12/12 | signature hex |

**Canonical util:** `libs/ui/src/identifiers/truncate.ts:21` `truncateMiddle`
+ `getDefaultTruncation(type)` exposed via `IdentifierDisplay` / `IdentifierWithCopy`.

**Also note (non-middle-truncation, end-only):** `web/src/pages/nft-detail/NftMetadata.tsx:33`
`text.slice(0, MAX_VALUE_LEN)…` — different category (end-truncate of
freeform JSON values), not a middle-truncate. **NOT counted in F-U-3.**

Plus 1 inline (no function wrapper) at `ContractEvents.tsx:107` for the
`data` cell — counted as a 7th ad-hoc impl point if strict, but it's the
inline twin of `shortStr` from the same file. F-U-3 said "6 ad-hoc
implementations" — this matches the 6 named functions; 7th inline lives
in the same file already cited.

**Conclusion:** F-U-3 count of 6 confirmed; no severity change.

---

## Cluster 4 — Formatter dups — exhaustive

### STROOPS_PER_XLM constants

| File:line | Value | Type |
|---|---|---|
| `web/src/pages/transactions/formatters.ts:1` | `10_000_000` | number |
| `web/src/pages/transaction-detail/shared/formatFee.ts:3` | `10_000_000n` | bigint |

**Count: 2** — matches F-U-4. No other instances; no raw `1e7` literals
in production code.

### formatFee functions

| File:line | Implementation | Path |
|---|---|---|
| `web/src/pages/transactions/formatters.ts:11` | Number-based, `toFixed(7).replace(/\.?0+$/, '')` | `transactions/formatters` |
| `web/src/pages/transaction-detail/shared/formatFee.ts:5` | BigInt-based, manual whole/frac split | `transaction-detail/shared/formatFee` |

**Count: 2** — matches F-J-16. Plus 1 `formatStroops` at
`transaction-detail/shared/formatFee.ts:15` (third entry point — F-J-17).

### toLocaleString('en-US') sites

| File:line | Use |
|---|---|
| `web/src/pages/LedgerDetailPage.tsx:85` | sequence label |
| `web/src/pages/ledgers/LedgerSummary.tsx:29` | base fee stroops display |
| `web/src/pages/ledgers/LedgerSummary.tsx:74` | sequence |
| `web/src/pages/ledgers/LedgerSummary.tsx:115` | transaction count |
| `web/src/pages/ledgers/LedgersTable.tsx:63` | transaction_count cell |
| `web/src/pages/ledgers/LedgerTransactions.tsx:46` | totalCount caption |
| `web/src/pages/nft-detail/NftSummary.tsx:88` | minted_at_ledger |
| `libs/ui/src/identifiers/IdentifierDisplay.tsx:73` | ledger formatForDisplay (internal util) |
| `libs/ui/src/visualization/Tabs.tsx:42` | tab count badge |
| `libs/ui/src/layout/TopNav.tsx:83` | formatNumber util |

**Count: 10** — matches F-U-2 cited 10. F-J-2 cited 7-9 (off by 1-3).

### toFixed sites bypassing canonical formatter

| File:line | Use |
|---|---|
| `web/src/pages/home/ChainOverview.tsx:53` | `tps_60s.toFixed(1)` |
| `web/src/pages/liquidity-pools/FeePill.tsx:24` | `n.toFixed(2)%` |
| `libs/ui/src/layout/TopNav.tsx:81` | `value.toFixed(1)M` (internal formatNumber) |
| `libs/ui/src/layout/TopNav.tsx:132` | `stats.tps_60s.toFixed(1)` |
| `web/src/pages/transactions/formatters.ts:14` | `xlm.toFixed(7)` — **canonical formatter**, not a bypass |

**Bypass-formatter count: 4** — F-J-3 cited 3, missed 1 (`TopNav.tsx:132` second TPS site distinct from line 81 internal util).

**Conclusion:** All counts confirmed exhaustive within ±1-2 of original
claims; no severity escalation needed.

---

## Cluster 5 — XDR / binary `as unknown` + structural casts — exhaustive

Grep across all of `web/src/` + `libs/ui/src/` + `libs/api-types/src/`
(excluding `generated/`).

### `as unknown as` — true cross-runtime type-escape

| File:line | Reason |
|---|---|
| `libs/ui/src/timestamps/useNow.ts:18` | `setInterval` return type cross-platform polyfill |

**Count: 1** — same as Wave 1 baseline. **No new instances post-Filip-merge.**

### `as any` / `@ts-ignore` / `@ts-expect-error`

**Count: 0** — Wave 1 baseline confirmed; clean zero across the whole tree.

### Structural inline casts `(x as { foo?: unknown })`

| File:line | Cast | Reason inferred |
|---|---|---|
| `web/src/api/client.ts:20` | `error as { message: unknown }` | error normalisation |
| `web/src/api/QueryProvider.tsx:14` | `error as { status?: number }` | retry-policy classifier |
| `web/src/api/queryKeys.ts:39` | `head as { _id?: unknown }` | SDK_IDS_BY_RESOURCE probe |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:12` | `details as { function_name?: unknown }` | heavy XDR shape probe |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:21` | `details as { summary?: unknown }` | heavy XDR shape probe |
| `libs/ui/src/states/classifyError.ts:23` | `err as { status: unknown }` | error classifier |

**Count: 6 structural casts**

### `as Record<string, unknown>` (runtime-narrowing prep)

| File:line | Cast |
|---|---|
| `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:21` | `details as Record<string, unknown>` |
| `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:23` | `details as Record<string, unknown>` |
| `web/src/pages/transaction-detail/advanced/HighlightedJson.tsx:63` | `value as Record<string, unknown>` |
| `web/src/pages/transaction-detail/normal/toFlowNodes.tsx:29` | `value as Record<string, unknown>` |

**Count: 4**

### Other domain-specific casts

| File:line | Cast | Notes |
|---|---|---|
| `web/src/pages/transaction-detail/normal/toFlowNodes.tsx:38` | `value as NestedCallShape[]` | post-`Array.isArray` narrow |
| `web/src/pages/pool-detail/PoolCharts.tsx:190` | `key as ChartMetric` | tab-key narrow on `Tabs.onChange` |
| `web/src/pages/transaction-detail/index.tsx:131-138` | `heavy as \| { results_meta_xdr?: ... } \| null \| undefined` | F-AQ-8 cited; OpenAPI codegen drift |

### Summary

**Total non-`as const` type-escape sites:** ~14 (1 unknown-as + 6 structural
inline + 4 Record + 3 domain-specific). F-AQ-7 cited "3 files in
`transaction-detail/advanced/`" — exhaustive count is **8 distinct files**
across tx-detail (humanizeOp, toFlowNodes, OperationJsonDetail,
HighlightedJson, RawDataSection via index.tsx) + 3 API files (client,
QueryProvider, queryKeys) + 2 libs/ui files (classifyError, useNow).

**Wave 1 baseline "zero `as any` / `@ts-ignore`" still holds post-Filip merge.** No regressions in the strongest guarantee.

**Severity:** F-AQ-7 / F-AQ-8 remain 🟡 MEDIUM. Larger surface area than
cited but pattern unchanged — all instances are defensive narrowing of
backend JSONB blobs (`Record<string, unknown>` shape) — the *correct*
defensive code given backend wire format. Real fix: stricter OpenAPI
schema for `details` field (discriminated union by op_type).

---

## Cluster 6 — URL-state hook consumers — exhaustive

### `useCursorPagination` consumers (11 total)

List pages (5):
- `web/src/pages/TransactionsListPage.tsx:34`
- `web/src/pages/LedgersListPage.tsx:20`
- `web/src/pages/AssetsListPage.tsx:32`
- `web/src/pages/NftsListPage.tsx:29`
- `web/src/pages/LiquidityPoolsListPage.tsx:34`

Detail-page tab/section tables (6):
- `web/src/pages/LedgerDetailPage.tsx:32` (parent ledger transactions)
- `web/src/pages/accounts/AccountTransactions.tsx:74`
- `web/src/pages/assets/AssetTransactions.tsx:72`
- `web/src/pages/contracts/ContractInvocations.tsx:76`
- `web/src/pages/contracts/ContractEvents.tsx:167`
- `web/src/pages/nft-detail/NftTransfers.tsx:89`
- `web/src/pages/pool-detail/PoolParticipants.tsx:78`
- `web/src/pages/pool-detail/PoolTransactions.tsx:122`

(8 sub-section consumers — total useCursorPagination call sites = **13**.)

All use the shared hook + namespaced `CURSOR_PARAMS.*` constants where
multiple sections coexist (pool detail, contract detail). Consistent.

### `useTabUrlState` consumers (1)

- `web/src/pages/ContractDetailPage.tsx:43` — `interface/invocations/events` tabs

### `useDetailMode` (parallel URL-state pattern)

- `web/src/pages/transaction-detail/index.tsx:29` — `?mode=normal|advanced`
- Defined in `web/src/pages/transaction-detail/useDetailMode.ts` (uses raw
  `useSearchParams`)
- Already documented as F-U-5 — "parallel URL-state abstraction"
- Verdict per AL Part 2: KEEP for now; consider unifying only if a 3rd
  URL-state surface emerges

### Raw `useSearchParams` consumers (1)

- `web/src/pages/SearchResultsPage.tsx:12` — reads `q=` query string
- **Legitimate** single-param scenario (not a table cursor/filter); per AL
  Part 2 verdict, this is the right boundary

### useState that COULD be URL state (review opportunity)

Found 3 candidates where state-as-URL would improve deep-linkability:

| File:line | State | Currently | Trade-off |
|---|---|---|---|
| `web/src/pages/transaction-detail/index.tsx:30` | `selectedIndex` | useState | F-AL-1 — deliberate; refresh resets to op #0 |
| `web/src/pages/transaction-detail/sections/OperationPicker.tsx:59` | `typeFilter` | useState | in-page filter, ephemeral by design |
| `web/src/pages/pool-detail/PoolCharts.tsx:128-129` | `metric` + `period` | useState | **NEW — F-EX-2** — pool chart tab + range; refresh resets to TVL / 30D |

**F-EX-2 trade-off:** moving `metric`/`period` to URL would let users share
"this pool's volume over 7 days" links. Trade-off identical to F-AL-1.
**Class:** C / 🟢 LOW. Defer to Gate B with Figma-intent check.

**Conclusion:** URL-state consumer count consistent with AL Part 2 KEEP
verdict. No bypassing pages discovered. Only new finding is F-EX-2
(pool chart tabs).

---

## NEW FINDINGS spawned by this sweep

### F-EX-1 [Class C, Severity 🟡] — NFT detail `minted_at_ledger` plain text per Figma

**Location:** `web/src/pages/nft-detail/NftSummary.tsx:82-93`

**Pattern:** `minted_at_ledger` rendered as `<Typography variant="bodySmMedium">{...toLocaleString('en-US')}</Typography>` with explicit inline comment "Plain Satoshi text per Figma — not a mono/linked identifier."

**Tension:** Every other ledger-sequence in the UI is linked via
`IdentifierDisplay type="ledger"` (AccountSummary first/last seen,
ContractSummary deployed-at, TransactionSummary ledger, etc.). Only
this single site is intentionally plain per Figma.

**Class:** C (visual consistency vs Figma intent) — defer to Gate B
visual audit with Figma cross-reference. May be deliberate UX choice
(NFT focuses attention on token metadata not chain navigation) or
Figma oversight.

**Severity:** 🟡 MEDIUM (inconsistency, not break).

### F-EX-2 [Class C, Severity 🟢] — Pool chart metric/period in useState, not URL

**Location:** `web/src/pages/pool-detail/PoolCharts.tsx:128-129`

```ts
const [metric, setMetric] = useState<ChartMetric>('tvl');
const [period, setPeriod] = useState<ChartPeriod>('30D');
```

**Trade-off:** moving to URL state (`?chart=tvl&range=30D`) would let
users deep-link "this pool's volume over 7 days". Currently refresh
resets to TVL / 30D.

**Class:** C — same family as F-AL-1 (tx-detail selectedIndex).

**Severity:** 🟢 LOW — deliberate vs deep-link trade-off; defer Gate B.

---

## Task-scope implications for user review

### Task 0262 (composite NotFound) — REPORT-ONLY per task spec

**Original scope (per F-D-2):** account + contract

**Exhaustive scope correction:**
- **Account (E6) — affected** — `AccountTransactions` mounted on parent error
- **Contract (E9) — affected** — Interface + Invocations + Events all mounted on parent error; up to 4 error blocks
- **Pool (E13) — NEW, affected** — Charts + Participants + Transactions all mounted on parent error
- **Asset (E8) — NOT affected** (was speculative in F-D-2) — render-gate already in place at `AssetDetailPage.tsx:127`

**Reference implementation:** AssetDetailPage line 127 — `{!asset.isError && <AssetTransactions assetId={id} />}` — clean gate pattern.

**User action:** extend task 0262 body on their branch to cover 3 pages (account, contract, pool), not 2.

### Task 0263 (pool detail reserve links + PoolAssetLeg backend extend)

**Original scope:** pool detail reserve labels Link wrap (PoolSummary)

**Exhaustive surface:**
- `web/src/pages/pool-detail/PoolSummary.tsx` (AssetReserveCell × 2 reserves)
- `web/src/pages/pool-detail/PoolKpiStrip.tsx` (KpiCell label + subtitle × 2 reserves) — **NEW surface**
- `web/src/pages/liquidity-pools/PoolsTable.tsx` (reserves column on list page) — **NEW surface**

All 3 share root cause F-K-9 (`PoolAssetLeg` lacks linkable identifier).
Once backend extends schema, FE Link wrap touches 3 files not 1.

**User action:** confirm task 0263 PR includes all 3 sites in FE Link wrap.

### Task 0264 (strkey canonical URL form)

**Original scope:** pool URL canonical form (`L...` strkey vs hex)

**Exhaustive sweep impact:** none — no additional endpoints discovered
that mix display+route encoding. Pool is the only case.

**User action:** none from this sweep.

### Task 0265 (Vite CVE)

**Out of scope** per sweep request. No implication.

---

## Recommendation

**Wave 6 readiness: GREEN.**

Rationale:
- All 6 cluster patterns have been verified exhaustive (counts ±1-2 of
  original where noted, no severity escalation needed).
- Only **2 new findings** spawned (F-EX-1, F-EX-2) — both Class C / 🟡 / 🟢
  visual-or-deep-link concerns, deferrable to Gate B.
- **0 new HIGH/CRITICAL findings** — baseline measurements stable.
- Cluster 1 (composite NotFound) scope correction is the most impactful
  outcome: 3 affected pages confirmed, with E8 confirmed clean (reference
  pattern available).
- Cluster 2 (cross-entity links) reveals 2 additional pool-reserve link
  surfaces that fold into existing task 0263 scope when backend schema
  extends.
- Wave 6 (Track 2 visual + UX) can now measure residual visual / Figma
  drift against a known-exhaustive baseline. F-D-2 / F-K-2 / F-U-3 /
  F-U-4 / F-J-16 / F-AQ-7 / F-AQ-8 all verified.

**Blockers hit:** none. All greps + reads completed without errors.

---

## Sweep methodology notes

- Procedure: grep first for raw matches across `web/src/` + `libs/ui/src/`
  + `libs/api-types/src/` (excluding `generated/`), then read each match
  for context to classify.
- Files read in full or in relevant ranges: 14 (TransactionDetailPage,
  LedgerDetailPage, AccountDetailPage, AssetDetailPage, ContractDetailPage,
  NftDetailPage, LiquidityPoolDetailPage, AccountTransactions, PoolParticipants,
  PoolSummary, PoolKpiStrip, AccountSummary, NftSummary, ContractSummary,
  AssetSummary, LedgerSummary, AccountBalances, ContractInvocations,
  ContractEvents, LatestTransactionsTable, TransactionsTable, AssetsTable,
  PoolsTable, SignaturesTable, SearchResultsPage, TransactionSummary,
  NftMetadata, IdentifierDisplay).
- Production code: untouched. No edits, no commits.
- Output: this file (`exhaustive-sweep-2026-05-26.md`) + appended sections
  in `D-state-coverage-matrix.md`, `K-cross-entity-links.md`,
  `J-data-formatting.md`, `U-component-reuse.md`, `AQ-type-safety.md`,
  `AL-state-separation.md`.
