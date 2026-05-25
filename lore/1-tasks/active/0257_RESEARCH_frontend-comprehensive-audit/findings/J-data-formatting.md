# J — Data formatting consistency (1.8)

**Wave:** 2
**Stance:** senior fresh-eye, read-only
**Date:** 2026-05-25

Grep-driven sweep across `web/src/` + `libs/ui/src/`.

## Summary table

| # | Check | Verdict | Evidence | Severity | Class |
|---|---|---|---|---|---|
| J-1 | Numeric display through `formatAmount` / `formatCompactAmount` | mostly ✓ | 30+ call sites verified; tables (Assets, Pools, Participants, Contracts, Accounts, Home) all route through `formatAmount`. | — | — |
| J-2 | Direct `toLocaleString('en-US')` bypassing formatter | partial | 7 sites bypass formatter and call `toLocaleString` directly: `web/src/pages/LedgerDetailPage.tsx:84`, `web/src/pages/ledgers/LedgersTable.tsx:63`, `web/src/pages/ledgers/LedgerSummary.tsx:29,74,115`, `web/src/pages/ledgers/LedgerTransactions.tsx:46`, `web/src/pages/nft-detail/NftSummary.tsx:88`, `libs/ui/src/visualization/Tabs.tsx:42`, `libs/ui/src/identifiers/IdentifierDisplay.tsx:73`. All ledger-related → integer "thousands separated" — works correctly but inconsistent with `formatAmount` (which handles same input + null). Locale hardcoded `'en-US'` — fine if i18n out of scope (per 0257 dropped scope N) | 🟡 MEDIUM | D |
| J-3 | Direct `toFixed()` outside formatters | partial | 3 sites: `web/src/pages/home/ChainOverview.tsx:53` `tps_60s.toFixed(1)`; `web/src/pages/liquidity-pools/FeePill.tsx:24` `n.toFixed(2)%`; `libs/ui/src/layout/TopNav.tsx:80,124` `tps_60s.toFixed(1)` + `M`-suffix formatter. Each is a one-off `n.x` style with no null guard; **TopNav `formatNumber` is local re-implementation** that duplicates intent of `formatCompactAmount` (which lives in `web/src/pages/pool-detail/helpers.ts` — different package, hence the duplication) | 🟡 MEDIUM | C |
| J-4 | Stroop ↔ XLM single util | gap | `STROOPS_PER_XLM = 10_000_000` defined **only** in `web/src/pages/transactions/formatters.ts:1`; consumed by `formatFee`. Used at 2 sites: `web/src/pages/ledgers/LedgerSummary.tsx:26` (`formatFee(stroops)`) + raw `stroops.toLocaleString('en-US')` at line 29 ("base_fee 100 (100 stroops)"). **No `libs/ui` util.** If a third call site appears, drift risk | 🟡 MEDIUM | D |
| J-5 | Timestamps — relative + absolute UTC consistent | partial | Relative: `libs/ui/src/timestamps/RelativeTimestamp.tsx` (used widely) — tooltip = ISO, label = relative; Absolute: `web/src/pages/transactions/formatters.ts:22` `formatAbsoluteUtc` returns `YYYY-MM-DD HH:mm:ss UTC` — used in transactions only. Other detail pages (ledger, asset, contract, account, NFT, pool) **do not show absolute UTC anywhere** — only relative via `RelativeTimestamp`. Inconsistent depth of timestamp info per route | 🟡 MEDIUM | C |
| J-6 | `<time dateTime>` semantic element | gap | zero `<time` hits in `web/src/` + `libs/ui/src/`. (See also C-8.) | 🟢 LOW | D |
| J-7 | Address truncation `4+4` single component | partial | Default path: `IdentifierDisplay` (`libs/ui/src/identifiers/IdentifierDisplay.tsx`) uses `truncateMiddle(formatted, cfg)` from `libs/ui/src/identifiers/truncate.ts` with `getDefaultTruncation(type)` — single source. Local re-implementations: `web/src/pages/AccountDetailPage.tsx:23` `shortId(id)` (`slice(0,4)…slice(-4)`, breadcrumb only) + `web/src/pages/contracts/ContractEvents.tsx:47` `shortStr(value)` (length 14, identifier-like substrings in event topics JSON). 2 local re-impls = inconsistency risk | 🟡 MEDIUM | C |
| J-8 | Hash truncation separate from address | partial | Same `IdentifierDisplay` handles `type="transaction"`; per-type `TruncationConfig` lives in one map — good. But the same component handles 8 entity types — single defaults map controls all, which is desirable | — | — |
| J-9 | Strkey vs hex display strategy (per B1) | ✓ ok | Pool ids displayed as `L…` strkey via `poolIdHexToStrkey` (`web/src/utils/poolIdStrkey.ts`); URL also uses encoded strkey via `routes.pool(id) = /liquidity-pools/${encodeURIComponent(id)}`; copy lives on the IdentifierWithCopy chip. Strategy: **display + URL use strkey**, hex is purely the wire format. Documented in `PoolsTable.tsx:51` + `PoolDetailHeader.tsx:37-39` + `PoolSummary.tsx:62` comments — explicit, consistent | — | — |
| J-10 | Asset labels with issuer disambig (per B4) | ✓ ok | `assetLegLabel` in `web/src/pages/pool-detail/helpers.ts:16` — pool legs only. **Note:** per B4 this was reverted/replaced in 0251 — confirm via task body. Current behavior: native → `'XLM'`, non-native → `asset_code` (no issuer concat). 6 call sites: `PoolsTable.tsx:48,98,105`, `PoolSummary.tsx:53-54`, `PoolKpiStrip.tsx:65-66`, `AssetAvatar.tsx:26`, `PoolDetailHeader.tsx:34` — pool detail family only, not list pages → asset page may show ambiguous symbol if multiple issuers exist for same code, but that's a separate render concern | — | — |
| J-11 | Percentages decimal places | partial | `formatAmount(value, 2)` for share_percentage (`PoolParticipants.tsx:48`), `formatAmount(pool.fee_percent, 2)%` (`PoolSummary.tsx:77`), but `FeePill.tsx:24` uses raw `n.toFixed(2)` — same decimals (2) coincidentally aligned, no shared constant. Drift risk if one site changes | 🟢 LOW | D |
| J-12 | Status badge — Success/Failed colors | ✓ ok | `StatusCell` (`web/src/pages/transactions/cells.tsx:35-44`) is the canonical chip: `successful ? 'success' : 'error'`, dot, label `Success`/`Failed`. `SearchResultRow.tsx:21` defines `SUCCESS_CHIP = { color: 'success', label: 'Success' }` — different shape, same color tokens. Theme tokens `success`/`error` defined in `libs/ui/src/theme/`. Consistent. **No "Pending" / "Unknown" state in use** (transactions are binary post-ledger close) | — | — |
| J-13 | Event-type chip colors | ✓ ok | `EVENT_TYPE_COLOR` map (`web/src/pages/contracts/ContractEvents.tsx:31-35`) — single map: contract=blue, system=brown, diagnostic=neutral. Documented "matching Figma events table" comment. Consistent | — | — |
| J-14 | Currency symbol "XLM" | partial | 2 hardcoded sites: `web/src/pages/accounts/AccountBalances.tsx:18` (`native ? 'XLM' : balance.asset_code ?? '—'`) + `web/src/pages/pool-detail/helpers.ts:17` (`if (leg.asset_type_name === 'native') return 'XLM'`). Plus `formatFee` returns `'… XLM'` string suffix. No `NATIVE_ASSET_CODE` constant. Low drift risk (2 sites, both small), but no single source | 🟢 LOW | D |
| J-15 | Em-dash for null/missing | ✓ ok | See C-11. All `?? '—'` patterns + `Dash()` + `FALLBACK = '—'` agree | — | — |

## Cross-references

- **J-3** `TopNav.formatNumber` duplicates `formatCompactAmount` — Wave 1 F-AI-* + F-CO-* noted `libs/ui` vs `web/src` boundary; this is a real consequence — formatter can't move to `libs/ui` because `formatCompactAmount` lives under `web/src/pages/pool-detail/helpers.ts`. Reshuffling formatters into `libs/ui/src/format/` would fix.
- **J-4** Stroop util gap: would benefit from `libs/ui/src/format/stroops.ts` colocated with `formatAmount` (currently in `web/src/pages/format.ts`).
- **J-7** `shortId`/`shortStr` local re-impls — sub-phase 1.9 U Component reuse will flag these as additional candidates.
- **J-10** confirms B4 alignment (no issuer concat post-0251 revert).

## Top issues

1. **J-5 (🟡 MEDIUM, Class C):** absolute UTC timestamp shown in transactions only — other detail pages omit it. Inconsistent depth.
2. **J-7 (🟡 MEDIUM, Class C):** 2 local truncation re-impls (`shortId`, `shortStr`) bypass `IdentifierDisplay` / `truncateMiddle`.
3. **J-3 (🟡 MEDIUM, Class C):** `TopNav.formatNumber` duplicates `formatCompactAmount` across libs/ui ↔ web boundary.
4. **J-4 (🟡 MEDIUM, Class D):** `STROOPS_PER_XLM` constant lives in 1 site; no shared util — drift risk if reused.
5. **J-2 (🟡 MEDIUM, Class D):** 9 direct `toLocaleString('en-US')` calls bypass `formatAmount` (all integer counts, semantically OK but inconsistent).
