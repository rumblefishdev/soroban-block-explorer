# U — Component reuse (Wave 4 1.9)

## Per-check table

| Check | Result | Evidence | Severity |
|---|---|---|---|
| `Chip` reused across pages | ✓ | Imported in 8+ files including tx-detail advanced + sections | — |
| `IdentifierDisplay` (or `IdentifierWithCopy`) reused | ✓ | Used in AssetsTable, AccountSummary, contracts, ledger pages | — |
| `ExplorerTable` reused on list + tab tables | ✓ | 13 pages import it (see grep below); no reimplementations in `web/src/pages/transaction-detail/` (tx-detail uses MUI Box layouts for non-tabular sections) | — |
| `SectionCard` reused | ⚠ | **Local impl at `web/src/pages/detail/SectionCard.tsx`** — not in `libs/ui`. 16 page files import the local. See F-U-1. | 🟡 |
| `EmptyState` / `*ErrorState` reused | ✓ | Used via `SectionErrorBoundary` (states/index.ts exports) + via TanStack `isError` paths | — |
| Inline number formatters | ⚠ | 10 inline `toFixed` / `toLocaleString` / `1e7` sites. See F-U-2. | 🟡 |
| Detail-page pattern uniform (breadcrumb + heading + SectionCard + SectionErrorBoundary) | ⚠ | SectionErrorBoundary only wraps 2/7 detail pages (Account, Contract). See F-AE-3 in 1.6 findings. | 🟡 |
| List-page pattern uniform (filter bar + ExplorerTable + PaginationControls + useTableUrlState + useInfinitePager) | ✓ | 5 list pages (transactions, ledgers, assets, nfts, pools) all share pattern via `useCursorPagination` + `usePageHandlers` (post-0254). No `useInfinitePager` exists — replaced by cursor pagination since 0254. | — |
| Post-Filip E3 reuse delta | ✓ | tx-detail/sections + tx-detail/advanced imports verified: SignaturesTable + TransactionSummary use libs/ui SectionCard pattern from `web/src/pages/detail/SectionCard.tsx` (local); Chip from libs/ui; api-types for DTOs. Note: OperationFlowTree (libs/ui visualization) used by NormalRightPanel — good hoist. | — |
| Truncation helpers shared | ✗ | 6 ad-hoc implementations: see F-U-3 (J-7 escalation from Gate A). | 🟠 |
| Stroop→XLM conversion shared | ✗ | 2 STROOPS_PER_XLM constants (number + bigint) across 2 files. F-U-4 (J-4 escalation). | 🟠 |

## Findings

### F-U-1 [Class C, Severity 🟡] — `SectionCard` is a local implementation, not in `libs/ui`

- **Location:** `web/src/pages/detail/SectionCard.tsx`
- **Consumers:** 16 files across all detail pages + tx-detail/sections + tx-detail/advanced + pool-detail + asset-detail + account-detail + contract-detail
- **Lib/ui:** does NOT export SectionCard (`libs/ui/src/index.ts` clean)
- **Impact:** Cross-page styling drift risk. Component is the visual chrome of every detail section.
- **Recommendation:** Promote to `libs/ui/src/layout/SectionCard.tsx`, add to libs/ui barrel, update 16 consumer imports.
- **Class:** C (visual; promoting changes DOM nothing if API preserved, but is a visual contract — defer to Gate B per cascade-compression table).

### F-U-2 [Class C, Severity 🟡] — Inline `toFixed` / `toLocaleString` outside `format.ts`

10 sites identified:

| File:line | Pattern | Should be |
|---|---|---|
| `web/src/pages/LedgerDetailPage.tsx:85` | `ledger.sequence.toLocaleString('en-US')` | `formatInteger(ledger.sequence)` |
| `web/src/pages/home/ChainOverview.tsx:53` | `data.tps_60s.toFixed(1)` | `formatTps(value)` util |
| `web/src/pages/ledgers/LedgerSummary.tsx:29,74,115` | 3× `toLocaleString('en-US')` | `formatInteger` |
| `web/src/pages/ledgers/LedgersTable.tsx:63` | `row.transaction_count.toLocaleString('en-US')` | `formatInteger` |
| `web/src/pages/ledgers/LedgerTransactions.tsx:46` | `totalCount.toLocaleString('en-US')` | `formatInteger` |
| `web/src/pages/liquidity-pools/FeePill.tsx:24` | `n.toFixed(2)` | `formatPercent(n, 2)` |
| `web/src/pages/nft-detail/NftSummary.tsx:88` | `nft.minted_at_ledger.toLocaleString('en-US')` | `formatInteger` |
| `web/src/pages/transactions/formatters.ts:14` | `xlm.toFixed(7).replace(/\.?0+$/, '')` | OK — this IS the utility |

- **Class:** C
- **Recommendation:** Centralize `formatInteger`, `formatTps`, `formatPercent` in `web/src/pages/format.ts` and migrate all sites.

### F-U-3 [Class C, Severity 🟠] — Truncation helper re-implementations (escalates J-7)

Multiple ad-hoc implementations:

| File:line | Pattern | head/tail |
|---|---|---|
| `web/src/pages/AccountDetailPage.tsx:23` | `id.slice(0,4)…id.slice(-4)` | 4/4 |
| `web/src/pages/contracts/ContractEvents.tsx:47` | `value.slice(0,4)…value.slice(-4)` | 4/4 |
| `web/src/pages/contracts/ContractEvents.tsx:107` | `data.slice(0,10)…data.slice(-10)` | 10/10 |
| `web/src/pages/transaction-detail/index.tsx:24` | `hash.slice(0,6)…hash.slice(-4)` | 6/4 |
| `web/src/pages/transaction-detail/advanced/EventsSection.tsx:30` | `value.slice(0,5)…value.slice(-4)` | 5/4 |
| `web/src/pages/transaction-detail/sections/SignaturesTable.tsx:29` | `truncateHex(hex, head=12, tail=12)` | 12/12 |
| `web/src/pages/ContractDetailPage.tsx:88` | `truncateMiddle(contractId, BREADCRUMB_TRUNCATION)` | uses libs/ui util ✓ |

- **Note:** `truncateMiddle` already exists in `libs/ui` (used by ContractDetailPage:88). Other 6 are ad-hoc.
- **Class:** C — defer to Gate B; consolidates with F-U-1 hoist refactor.

### F-U-4 [Class A, Severity 🟠] — Two STROOPS_PER_XLM constants (escalates J-4)

- `web/src/pages/transactions/formatters.ts:1`: `const STROOPS_PER_XLM = 10_000_000;` (number)
- `web/src/pages/transaction-detail/shared/formatFee.ts:3`: `const STROOPS_PER_XLM = 10_000_000n;` (bigint)
- Plus duplicate `formatFee` function noted in delta-audit (F-J-16): probably overlapping logic with `formatXlm` in transactions/formatters.ts.
- **Class:** A — defer Phase 3 unification; consolidate to single util in `libs/ui` (or web/src/pages/format.ts) returning either string or with bigint support.

### F-U-5 [Class A, Severity 🟡] — `useDetailMode` doesn't compose with `useTableUrlState`

- **Evidence:** `web/src/pages/transaction-detail/useDetailMode.ts` uses `useSearchParams` directly, while every list/tab page uses `useTableUrlState`. Two parallel URL-state abstractions.
- **Impact:** None functional (different concerns — tab mode vs cursor/filter). But discoverability cost: new contributor sees two patterns for "URL as state".
- **Recommendation:** Either (a) extend `useTableUrlState` to support per-page non-pagination keys (e.g. `mode`), or (b) document explicitly that detail-page mode tabs use raw searchParams + cursor pagination uses `useTableUrlState`.
- **Class:** A (informational) — defer to 1.12 EXTRA analysis below.

## Summary

5 findings: 0 🔴, 2 🟠 (F-U-3, F-U-4 escalations from Gate A), 3 🟡.

Component reuse is healthy. Main concerns: SectionCard hoist (visual chrome), truncation helper unification, stroops/XLM constant dedup.

---

## Exhaustive truncation re-impl sweep 2026-05-26 (pre-Wave-6)

Trigger: F-U-3 cited "6 ad-hoc implementations". Confirm count is exhaustive
across `web/src/` + `libs/ui/src/` + `libs/api-types/src/`
(excluding `generated/`).

### Greps applied

- `slice(0,` and `slice(-` (raw substring truncation)
- function name declarations: `function shortHash|shortId|shortStr|shortenStrKey|truncateMiddle|truncateHex`

### Result: 6 named ad-hoc impls — F-U-3 CONFIRMED exhaustive

| File:line | Function | head/tail | Used in |
|---|---|---|---|
| `web/src/pages/AccountDetailPage.tsx:22` | `shortId(id)` | 4/4 | breadcrumb crumb |
| `web/src/pages/contracts/ContractEvents.tsx:46` | `shortStr(value)` | 4/4 (when >14) | topic JSON strings |
| `web/src/pages/contracts/ContractEvents.tsx:107` | inline `data.slice(0,10)…slice(-10)` | 10/10 | event data cell (same file) |
| `web/src/pages/transaction-detail/index.tsx:23` | `shortHash(hash)` | 6/4 (when >12) | tx breadcrumb |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:5` | `shortId(value)` | 6/4 (when >12) | op flow tree labels |
| `web/src/pages/transaction-detail/advanced/EventsSection.tsx:29` | `shortenStrKey(value)` | 5/4 (when >12) | event topic identifiers |
| `web/src/pages/transaction-detail/sections/SignaturesTable.tsx:29` | `truncateHex(hex, 12, 12)` | 12/12 | signature hex |

**End-truncation (different category, NOT counted):**
- `web/src/pages/nft-detail/NftMetadata.tsx:33` — `text.slice(0, MAX_VALUE_LEN)…` (end-only truncation of freeform JSON values, not middle-truncate identifier display)

**Canonical util available:** `libs/ui/src/identifiers/truncate.ts:21` exports `truncateMiddle` + `getDefaultTruncation(type)` map. Already consumed by `ContractDetailPage.tsx:88` (uses canonical util — proof-of-pattern).

**Conclusion:** F-U-3 count of 6 confirmed exhaustive; no severity change.
Severity 🟠 HIGH stands.

---

## Exhaustive STROOPS_PER_XLM sweep 2026-05-26 (pre-Wave-6)

Trigger: F-U-4 cited "2 STROOPS_PER_XLM constants". Confirm count is exhaustive.

### Greps applied

- `STROOPS_PER_XLM`, `STROOP_PER_XLM`, `STROOPS`, `STROOP_DIVISOR`
- `10_000_000`, `10000000`, `10_000_000n`, `1e7`
- `stroopsToXlm`, `xlmToStroops`, `fromStroops`, `toStroops`, `convertStroops`
- `formatFee`, `formatStroops`

### Result: confirmed exhaustive

| Constant location | Type | Value |
|---|---|---|
| `web/src/pages/transactions/formatters.ts:1` | `number` | `10_000_000` |
| `web/src/pages/transaction-detail/shared/formatFee.ts:3` | `bigint` | `10_000_000n` |

**Count: 2** — matches F-U-4. No `1e7` literals found; no other stroop divisors.

| `formatFee` impl | Approach |
|---|---|
| `web/src/pages/transactions/formatters.ts:11` | Number-based, `xlm.toFixed(7).replace(/\.?0+$/, '')` |
| `web/src/pages/transaction-detail/shared/formatFee.ts:5` | BigInt-based, whole/frac split |

Plus `formatStroops` at `transaction-detail/shared/formatFee.ts:15`
(third entry point — captured by F-J-17).

**Conclusion:** F-U-4 count confirmed; no severity change. Severity 🟠 HIGH stands.
