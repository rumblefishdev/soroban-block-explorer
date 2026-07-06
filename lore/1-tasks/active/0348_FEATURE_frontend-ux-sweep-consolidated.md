---
id: '0348'
title: 'Frontend UX sweep — consolidated fixes (19 findings across all pages)'
type: FEATURE # bundle of bug-fixes + UX polish from a full-page audit
status: active
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
  - date: 2026-07-03
    status: active
    who: karolkow
    note: >
      Promoted to active to begin the consolidated fix pass.
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
**→ DEFERRED to [[0357]] (2026-07-06).** Root-caused: not a display bug — the
asset-tx query has no native branch (native = empty-string identity). The
investigation escalated into a system-wide data-model audit; the fundamental
fix (per-(op,asset) participation index, native first-class) is a backend epic
tracked in task 0357, not FE polish. The stopgap "variant C" built here was
reverted. This finding is closed in 0348 as deferred, owned by 0357.

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

## Progress (2026-07-06)

Branch `feat/0348` (off develop). This task's own scope is the **non-video
subset** (F1, F2, F9, F12, F13, F15, F16); the rest (F3–F8, F10, F11, F14,
F17–F19) is the curated video punch-list tracked in [[0351]].

**Done (on `feat/0348`):**

- **F15** — native XLM asset is a link (`AccountBalances.tsx`, `href:
routes.asset('native')`). Commit `15cd2a27`.
- **F13** — single 4/4 truncation standard: collapsed the redundant per-type
  map to one `DEFAULT_TRUNCATION`, removed inline one-offs (`humanizeOp`,
  `SignaturesTable`, `ContractEvents` topic), updated 2 tests. Commit
  `f205fe99`. Verified live on `:4301` (`af27…be98`).
- **F16** — dropped the redundant per-row type chip in search
  (`SearchResultRow.tsx`): rows are always tab-scoped, so the chip only repeated
  the tab label. Uncommitted (working tree).

**Deferred:**

- **F2** (native XLM tx empty) → [[0357]]. Root-caused as a data-model / backend
  problem (single-asset-slot index + native-as-absence), not FE polish. The
  stopgap "variant C" built during the investigation was reverted. F2 escalated
  into a full system-wide audit now living in 0357.

**Remaining in 0348 (FE):**

- **F16** commit + optional `/ux-expert` pass on the landed fixes.
- ~~**F9** — LP Fee column de-emphasize/drop~~ → **DONE** (2026-07-06).
  Verified: `fee_bps` parsed from on-chain XDR (`LiquidityPoolEntry.params.fee`,
  `xdr-parser/src/state.rs:855`); prod is 100% `30` across all 51,969 pools —
  protocol-fixed (`LIQUIDITY_POOL_FEE_V18`), not a bug, structurally constant
  for classic pools (Soroban AMMs are a separate contract data-path, not this
  table). Dropped the list column + the loud header `FeePill`; kept the quiet
  Summary "Fee: 0.30%" cell (canonical per-pool fact). `FeePill.tsx` → `.trash`
  (dead). Upgrade path: if a Soroban-AMM pool source is ever unified into this
  list, re-introduce Fee as a data-driven column (render iff `COUNT(DISTINCT
fee) > 1`). Typecheck + 111 web tests green.
- ~~**F12** — ledger-sequence fields paired with human time app-wide~~ →
  **SKIPPED** (2026-07-06, permanent drop — not deferred).
- ~~**F1** — invocations KPI 0-vs-table~~ → FE relabel half **DONE**
  (2026-07-06). Root cause: `recent_invocations`/`recent_events` are a 7-day
  activity window (bounded mainly to cap the 9.5B-row events count); the table
  pages all-time and never counts. The tab badge reused the 7d number as if it
  were an item-count → "0" over a full table on the 84.6% of contracts dormant
  > 7d. Fix (Option A): dropped `count` from the Invocations + Events tab badges;
  > KPI cards keep the honest "(last 7 days)" label. All-time-total badge (Option
  > B) deferred to [[0357]] K4-1 — invocations all-time is cheap, events all-time
  > isn't → product call.

## Acceptance Criteria

- [x] 1 — Invocations/Events tab badges no longer show a 7d count over an
      all-time table (FE relabel half) — **DONE** (feat/0348, uncommitted). The
      all-time-count _data_ half (make the badge show a real total) stays in
      [[0357]] K4-1: invocations all-time is a cheap seek, but events all-time
      is a 9.5B-row cost problem → product decision, not FE polish.
- [~] 2 — Native XLM tx — DEFERRED to 0357 (root-caused: data-model, backend epic)
- [ ] 3 — Home loads at scroll-top, hero visible
- [ ] 4 — Asset detail supply no longer overlaps Holders
- [ ] 5 — Time column no longer clips in the wide transaction tables
- [ ] 6 — Tables fit their row count for small result sets (no empty void)
- [ ] 7 — NFT trait values use a body-size variant
- [ ] 8 — NFT list Collection column resolved; image fallback added
- [x] 9 — LP Fee column dropped (list) + detail deduped — **DONE** (feat/0348, uncommitted)
- [ ] 10 — LP TVL filter hidden behind a 0341-style flag
- [ ] 11 — Accounts list seen-columns relabeled and/or given human time
- [~] 12 — Ledger-sequence + human time — **SKIPPED** (2026-07-06, permanent drop)
- [x] 13 — Single truncation standard (first 4 + last 4) applied and enforced;
      one-offs removed; untruncated identifiers found and fixed — **DONE** (feat/0348 `f205fe99`)
- [ ] 14 — `formatCompactAmount` wired into all large-number display sites — → **0351** (video subset)
- [x] 15 — Native XLM asset is a link — **DONE** (feat/0348 `15cd2a27`)
- [x] 16 — Search redundant per-row chip removed when tab-scoped — **DONE** (feat/0348, uncommitted)
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
