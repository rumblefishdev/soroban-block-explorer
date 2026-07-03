---
id: '0348'
title: 'Frontend UX sweep — consolidated fixes (19 findings across all pages)'
type: FEATURE # bundle of bug-fixes + UX polish from a full-page audit
status: backlog
related_adr: []
related_tasks: ['0341', '0351']
tags: ['frontend', 'ux', 'phase-polish', 'effort-large', 'priority-medium']
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      Created from a full-page UX audit of the web app (16 pages swept live
      against prod-read API, light + dark). 19 findings consolidated into one
      task. Analysis-only — no code changed yet. Root causes + file refs
      captured per finding.
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      Video-prep subset (the eye-catching, FE-only findings) split into 0351
      for the milestone-2 frontend video: F3, F4, F5, F6, F7, F8, F10, F11,
      F14, F17, F18, F19. 0348 stays the full record; 0351 is the curated
      punch-list. Fix each finding once — mark it done in both when landed.
---

# Frontend UX sweep — consolidated fixes (19 findings)

## Summary

A full audit of the explorer frontend (every list page, every detail type,
search, 404 — 16 pages, light + dark) surfaced 19 issues spanning real
data/logic bugs, layout breaks, dead/misleading UI, and inconsistent
formatting standards. This task bundles them so they can be fixed as one
coherent polish pass rather than 19 micro-tasks. Each finding below carries
its root cause and the exact file(s) to touch.

## Status: Backlog

**Current state:** Audit complete, root-caused, not started. No files changed.

> **Video-prep subset → [[0351]].** The eye-catching, FE-only findings needed
> before the milestone-2 video are tracked in 0351: **F3, F4, F5, F6, F7, F8,
> F10, F11, F14, F17, F18, F19**. This task remains the full 19-finding record
> (incl. backend/subtle items F1, F2, F9, F12, F13, F15, F16). Fix a finding
> once and check it off in both places.

## Locked decisions (from audit review)

- **Truncation standard = first 4 + last 4** (`GCSO…DB2Z`). Applied everywhere,
  enforced consistently. See finding 13.
- **Native XLM asset = link it** (finding 15). `/assets/native` already resolves.
- **LP TVL filter = hide behind a flag** using the task-0341 pattern
  (module-level `const … = false` + early return / conditional render). See
  finding 10.

---

## Findings

Severity: **Bug** (wrong data/broken) · **Design** · **Data-honesty** ·
**Consistency** · **Polish** · **Env**.

### Data / logic

**1. [Bug] Contract invocations count always 0 while the table has rows.**
KPI card "Invocations (last 7 days)" + Invocations tab badge read
`stats.recent_invocations` from `GET /contracts/{id}` — a **7-day window**
(`l.closed_at >= now64() - INTERVAL 7 DAY`). The table reads
`GET /contracts/{id}/invocations` — **no time filter, all-time**. Dormant
contract (>7d) → count 0, table still lists old rows.
Fix (product call): either make the KPI all-time, or relabel it honestly and
show an all-time count on the badge.
Refs: `docs/architecture/database-schema/endpoint-queries-clickhouse/11_get_contracts_by_id.sql`
(stats, ~L81-92) vs `.../13_get_contracts_invocations.sql` (rows);
`web/src/pages/ContractDetailPage.tsx:65`, `web/src/pages/contracts/ContractSummary.tsx:31`.
(CH is source of truth; ignore PG path — retired.)

**2. [Bug?] Native XLM asset detail shows "No transactions yet".** XLM must
have transactions — suspect the native-asset transactions query/data path
returns empty. Investigate the `/assets/native` transactions endpoint before
assuming display bug. Ref: `web/src/pages/AssetDetailPage.tsx`.

**3. [Bug] Home auto-scrolls ~218px on load**, hiding the hero headline; user
lands mid-page on the search bar. Almost certainly `autoFocus` on the search
input scrolling itself into view. Fix: drop autoFocus or force
`scrollTo(0,0)` on mount. Ref: `web/src/pages/HomePage.tsx`, GlobalSearchBar.

### Layout / design

**4. [Design] Asset detail: supply value overlaps the "Holders" label.**
`SummaryRow` renders Total supply + Holders as two side-by-side `flex:1`
cells; the raw full-precision supply (`105,477,412,834.034398`) is one
unbreakable token wider than its half → overflows into the adjacent label
(overlapping text in dark mode). Fix ties to finding 14: compact-format the
supply (→ `105.5B`) so it fits; consider not pairing supply+holders on one
row. Refs: `web/src/pages/detail/SummaryRow.tsx:27-69`,
`web/src/pages/assets/AssetSummary.tsx:131-151`.

**5. [Design] Wide transaction table clips the Time column.** Fixed column
widths sum to ~1100px (160+120+160+190+120+140+**210**) > card width; last
col (Time) clips ("06:29:2…"). Affects the transactions list **and** the
"Transactions in this ledger" table on ledger detail — anywhere the
Fee+Time column set appears. Refs: `web/src/pages/transactions/TransactionsTable.tsx:138-190`.

**6. [Design] Fixed table height → empty void with few rows.** Rows pinned to
44px + skeleton/card reserves a full page of rows, so a 1-2 row result leaves
a blank gap before the footer bar (seen on NFT transfer history, pool
participants). Make height fit actual row count for small sets. Refs:
`libs/ui/src/table/ExplorerTable.tsx:290` (+ line 227 skeleton), DataListCard
skeletonRows default.

**7. [Design] NFT trait values oversized.** `TraitCard` renders each trait
**value** in `heading5SemiBold` (24px/600) — tiny values ("PFP") blown up
like headlines. Drop to a body variant (~16px). Grid layout itself is fine.
Ref: `web/src/pages/nft-detail/NftMetadata.tsx:98`.
(Note: the transfer-history hash renders correctly via `IdentifierDisplay` —
no bug there; the oversized trait value is the "big bold" thing.)

**8. [Design] NFT list: Collection column is `—` on every row** (dead column)
and most thumbnails are broken-image placeholders. Populate/drop the column;
add a letter/identicon fallback. Ref: `web/src/pages/NftsListPage.tsx`.

### Data-honesty / dead UI

**9. [Data-honesty] LP Fee column is constant.** Verified via ClickHouse: all
**51,815** pools have `fee_bps = 30` (0.30%) — a single distinct value (Stellar
protocol fixes classic AMM at 30 bps; not a bug). A column identical on every
row carries no signal — de-emphasize or drop it. Ref: `liquidity_pools.fee_bps`,
`web/src/pages/liquidity-pools/PoolsTable.tsx`.

**10. [Data-honesty] LP "Any TVL" filter with no TVL column.** Hide the filter
behind a flag (locked decision). Field exists (`PoolItem.tvl`, null on stale
pools) but is never rendered. Add `const TVL_FILTER_ENABLED = false` gating the
`<Select>`, mirroring task 0341's `CHARTS_ENABLED` pattern
(`web/src/pages/pool-detail/PoolCharts.tsx:274`). Filter control:
`web/src/pages/liquidity-pools/PoolsFilterBar.tsx:10-80`; wiring in
`web/src/pages/LiquidityPoolsListPage.tsx:26-40`.

**11. [Data-honesty] Accounts list "Last Seen"/"First Seen" = raw ledger
numbers, identical every row, mislabeled.** Columns show ledger sequence
(`63,306,152`) under temporal-sounding headers; the detail page correctly says
"…**ledger**". Fix the labels and/or pair with human time. Ref:
`web/src/pages/AccountsListPage.tsx`.

**12. [Data-honesty] Ledger sequence used as the time reference app-wide.**
"Deployed at ledger", "First/Last seen ledger", "Since ledger" — all raw
numbers, no human date. Novices can't read `62,537,656` as "when". Pair with
relative time across the app. (Systemic; touches multiple pages.)

### Consistency / standards

**13. [Consistency] Truncation inconsistent — one standard, enforced.**
6 distinct patterns today: 6/4, 4/4, 12/12, 10/10, 4/4-topics, 120-prefix-only.
Central helper exists (`truncateMiddle` / `getDefaultTruncation`,
`libs/ui/src/identifiers/truncate.ts`) but is bypassed in `ContractEvents.tsx`,
`humanizeOp.ts` (hardcoded 6/4), `SignaturesTable.tsx` (12/12),
`NftMetadata.tsx` (120-prefix). **Also**: some long values are not truncated
at all. Standard = **first 4 + last 4** (locked). Set the helper defaults,
remove the one-offs, and audit for untruncated identifiers so the standard is
consistent everywhere. (Subject-of-page hashes, e.g. the tx-detail hash, may
stay full — those are the subject, not a reference.)

**14. [Consistency] Compact-number helper exists but under-adopted.**
`formatCompactAmount` (`libs/ui/src/format/amount.ts:68`,
`Intl.NumberFormat notation:'compact'` → `1.5K`/`11B`) is used only in pools
(PoolsTable, PoolKpiStrip). Large numbers render raw elsewhere: home KPIs
("Accounts 24,620,044" while the nav shows "24.6M"), asset total_supply
("9,000,000,000"), holder counts, asset tables. Wire the helper into all
large-number display sites. Also directly fixes finding 4. Refs:
`web/src/pages/home/ChainOverview.tsx:75,88`, `web/src/pages/assets/AssetSummary.tsx`,
`web/src/pages/assets/AssetsTable.tsx`.

**15. [Consistency] Native XLM asset not clickable** — the only unlinked asset,
though `/assets/native` resolves (validated, tested, already linked on the
assets list). Root: `web/src/pages/accounts/AccountBalances.tsx:35` sets
`href: undefined` for native → renders plain `<Typography>`. Fix (locked = link
it): `href: routes.asset('native')`.

### Polish

**16. [Polish] Search: redundant per-row type chip.** Results under the
"Accounts" tab each carry an "Accounts" chip. Drop the chip when already scoped
by tab. Ref: `web/src/search/SearchResultRow.tsx`.

**17. [Polish] Home stat counters garble mid-animation** — rolling-digit tween
stacks digits during transitions (clean at rest). Ref: `web/src/pages/home/`.

**18. [Polish] Copy: "Stellar Lumen" → "Stellar Lumens"** on the native asset
detail header.

_(Also seen, minor: asset-list and NFT-list use "?" / broken-image placeholder
avatars — no asset/NFT logos. Fold into finding 8's fallback work.)_

### Environment / theme

**19. [Env] Chrome-light vs Firefox-dark, and no visible theme toggle.**
`ThemeProvider` (`libs/ui/src/theme/ThemeProvider.tsx:28-46`): with no stored
choice it falls to `prefers-color-scheme: dark`; Chrome and Firefox report
different OS-appearance values (Firefox has its own "Website appearance"
override), then `useEffect` persists whatever resolved to per-browser
localStorage → the two lock into different modes. Not a bug per se, but
`toggleMode` exists in context with **no toggle rendered** → user can't
override. Add a visible theme toggle (and decide a single default).

---

## Acceptance Criteria

- [ ] 1 — Contract invocations KPI/badge and table agree (or KPI honestly labeled)
- [ ] 2 — Native XLM asset detail shows its transactions (or root cause documented)
- [ ] 3 — Home loads at scroll-top, hero visible
- [ ] 4 — Asset detail supply no longer overlaps Holders
- [ ] 5 — Time column no longer clips in the wide transaction tables
- [ ] 6 — Tables fit their row count for small result sets (no empty void)
- [ ] 7 — NFT trait values use a body-size variant
- [ ] 8 — NFT list Collection column resolved; image fallback added
- [ ] 9 — LP Fee column de-emphasized/dropped
- [ ] 10 — LP TVL filter hidden behind a 0341-style flag
- [ ] 11 — Accounts list seen-columns relabeled and/or given human time
- [ ] 12 — Ledger-sequence fields paired with human time app-wide
- [ ] 13 — Single truncation standard (first 4 + last 4) applied and enforced;
      one-offs removed; untruncated identifiers found and fixed
- [ ] 14 — `formatCompactAmount` wired into all large-number display sites
- [ ] 15 — Native XLM asset is a link
- [ ] 16 — Search redundant per-row chip removed when tab-scoped
- [ ] 17 — Home stat counters don't garble mid-animation
- [ ] 18 — "Stellar Lumens" copy fix
- [ ] 19 — Visible theme toggle added; single default decided
- [ ] **Docs updated** — N/A unless the invocations-count decision (finding 1)
      changes an endpoint's documented semantics; if so, update the relevant
      `docs/architecture/**` query docs in the same PR.
- [ ] **API types regenerated** — N/A unless finding 1's fix touches
      `crates/api/**` (if the KPI window/shape changes server-side, run
      `npx nx run @rumblefish/api-types:generate`).

## Notes

- Audit method: live sweep of `:4200` (Vite dev proxy → prod-read API),
  Playwright screenshots, 7 parallel code-reading agents for root causes,
  one ClickHouse `chq` query to verify the LP fee constant.
- Most findings are frontend-only. Finding 1 (and possibly 2) may need
  backend/CH work — split into a sub-task at implementation time if so.
- Suggested implementation order: quick wins first (15, 18, 16, 3, 10),
  then the shared-primitive changes (13 truncation, 14 compact numbers — these
  cascade and fix 4), then per-page layout (5, 6, 7, 8, 11), then the
  data/product calls (1, 2, 9, 12, 19).
