---
id: '0251'
title: 'Frontend QA fixes batch: 13 bugs across 5 clusters'
type: BUG
status: active
related_adr: []
related_tasks: ['0077', '0246', '0249', '0250']
tags: ['frontend', 'qa', 'polish', 'bug', 'priority-high']
links:
  - 'PR #207 (parent — 0077 LP shipped, surfaced these bugs)'
history:
  - date: '2026-05-22'
    status: backlog
    who: karolkow
    note: 'Task created. Spawned from comprehensive Playwright-MCP QA traversal of 13 frontend routes covering whole explorer.'
  - date: '2026-05-22'
    status: active
    who: karolkow
    note: 'Promoted backlog → active.'
---

# Frontend QA fixes batch: 13 bugs across 5 clusters

## Summary

After PR #207 (0077 — Liquidity Pools list + detail) merged, a comprehensive
Playwright-MCP QA traversal was run over all 13 frontend routes plus
cross-cutting topbar/footer. 15 bugs were found. Fresh-eye senior review +
backend spec lookup reduced to **13 actionable**: 1 🔴 CRITICAL (broken
routing), 3 🟠 HIGH (broken UX), 8 🟡 MEDIUM (display/UX), 1 🟢 LOW (cosmetic).
Two items dropped: H3 asset URL "inconsistency" turned out to be by-design
(backend `:id` accepts numeric / C-strkey / CODE-ISSUER per spec) and H9
ScVal decoder defers to its own research task (significant lift, needs
backend serialization format research).

Goal: one PR batch, one lore task, 5 commits clustered by concern. Zero new
npm packages, zero new UI components — every fix is an edit to existing files
reusing existing helpers.

## Status: Backlog

**Current state:** Plan approved by karolkow on 2026-05-22 (plan file at
`~/.claude/plans/rozumiem-ze-przeszedles-teraz-abundant-backus.md`). Ready
to promote → active and start C1.

## Context

QA traversal performed against local stack: Vite dev :4200 + axum::serve
:9000 (local CORS+axum::serve patch on `crates/api/src/main.rs`, NOT
committed — Lambda runtime stays in prod) + Postgres :5433 backfill in
progress (61% done at QA time, 38,986 / 63,999 Soroban-era ledgers,
50,457,424–50,521,423 window).

Approach was read-only manual walkthrough using Playwright MCP per the
team's `[[feedback_playwright_mcp_vs_cli]]` (MCP for exploration). Each
route: `browser_navigate` → `browser_snapshot` (a11y tree) →
`browser_console_messages level=error` → `browser_network_requests` →
interact (filter, paginate, click row → detail, switch tabs) → edge
states (empty filter, invalid id, refresh URL state).

Bugs split into 5 clusters by touched file area, one commit per cluster.

### Bug-to-cluster mapping

| Cluster                     | Bugs                      | Files                                                                                                                                                                                                                               | Severity  |
| --------------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| **C1** LP polish            | B1, B2-LP-fee, B3, B4, B5 | `web/src/pages/pool-detail/PoolDetailHeader.tsx`, `web/src/pages/pool-detail/helpers.ts`, `web/src/pages/liquidity-pools/PoolsTable.tsx`, `web/src/pages/pool-detail/PoolSummary.tsx`, `web/src/pages/pool-detail/PoolKpiStrip.tsx` | 🔴1 / 🟡4 |
| **C2** Network stats wiring | H1, H4, H5                | `web/src/router/AppShell.tsx`, `libs/ui/src/layout/TopNav.tsx`, `libs/ui/src/layout/NetworkSwitcher.tsx`, `web/src/pages/home/ChainOverview.tsx`                                                                                    | 🟠1 / 🟡2 |
| **C3** Transactions filter  | H2, H6, H7                | `web/src/pages/transactions/operationTypes.ts`, `web/src/pages/TransactionsListPage.tsx`                                                                                                                                            | 🟠1 / 🟡2 |
| **C4** Error classification | H8                        | `libs/ui/src/states/classifyError.ts` (verify), `web/src/pages/LedgerDetailPage.tsx` + analogiczne detail pages                                                                                                                     | 🟡1       |
| **C5** Search polish        | H10                       | `web/src/pages/SearchResultsPage.tsx`                                                                                                                                                                                               | 🟢1       |

## Implementation Plan

### Commit 1 — C1 LP polish (5 bugs)

**B1 🔴 Pool ID header link routes to strkey instead of hex**

Plik: `web/src/pages/pool-detail/PoolDetailHeader.tsx:57`

Current:

```tsx
<IdentifierDisplay value={strkey} type="pool" />
```

Fix — add `href` override (analog to `PoolSummary.tsx:66-70`):

```tsx
<IdentifierDisplay value={strkey} type="pool" href={routes.pool(poolId)} />
```

Import `routes` from `web/src/router/routes.ts`. `poolIdHexToStrkey`
already in use elsewhere in the file.

**B3 🟡 List page column "Reserves" → "Total shares"**

Plik: `web/src/pages/liquidity-pools/PoolsTable.tsx:79-104`

Current: column id `reserves`, header `Reserves`, cell renders both
`reserve_a` + `reserve_b`. Per Figma final intent + alignment with
`PoolSummary.tsx`, column should be Total shares.

Fix — swap column definition:

```tsx
{ id: 'total_shares', header: 'Total shares', cell: (row) => formatAmount(row.total_shares) }
```

`PoolItem.total_shares` exists in `libs/api-types/src/generated/types.gen.ts:1180`.

**B2 🟡 Fee `0.30000000000000000000%` trailing zeros**

Pliki: `web/src/pages/liquidity-pools/PoolsTable.tsx:75` +
`web/src/pages/pool-detail/PoolSummary.tsx:77`

Current: raw interpolation `${row.fee_percent}%`. Backend serves
Postgres NUMERIC stringification = full precision.

Fix — use existing `formatAmount(value, minDecimals=2)` from
`web/src/pages/format.ts` (already used everywhere else):

```tsx
`${formatAmount(row.fee_percent, 2)}%`; // → "0.30%"
```

**B4 🟡 Fake-XLM disambiguation (always issuer for non-native)**

Plik: `web/src/pages/pool-detail/helpers.ts:16-24`

On Stellar, anyone can issue a `credit_alphanum4` asset with `code === 'XLM'`.
Frontend currently renders real native XLM and fake-XLM identically →
phishing vector.

Current:

```ts
if (leg.asset_type_name === 'native') return 'XLM';
if (leg.asset_code != null && leg.asset_code !== '') return leg.asset_code;
throw …;
```

Fix:

```ts
if (leg.asset_type_name === 'native') return 'XLM';
if (leg.asset_code != null && leg.asset_code !== '') {
  if (leg.issuer != null) {
    const head = leg.issuer.slice(0, 4);
    const tail = leg.issuer.slice(-4);
    return `${leg.asset_code} (${head}…${tail})`;
  }
  return leg.asset_code; // SAC/Soroban without issuer
}
throw …;
```

Recommendation rationale: always issuer for non-native (Option 1).
Predictable mental model ("no parens = real native, parens = check issuer")
beats smart heuristic ("sometimes shown"). Truncated 4+4 keeps label tight.
Approved by karolkow on 2026-05-22 (B4 question in plan).

**B5 🟡 React duplicate key `"XLM reserve"`**

Pliki: `web/src/pages/pool-detail/PoolSummary.tsx` +
`web/src/pages/pool-detail/PoolKpiStrip.tsx`

Consequence of B4. When both legs have `code === 'XLM'`, keys like
`${codeA} reserve === ${codeB} reserve` collide → React warning.

Fix — index + composite key:

```tsx
key={`${idx}-${leg.asset_type_name}-${leg.asset_code ?? 'native'}`}
```

Belt-and-braces: safe even if B4 fix is later reverted (positional key
guarantees no collision).

### Commit 2 — C2 Network stats wiring (3 bugs)

**H4 🟡 TopNav banner 4 stats hardcoded "0"**

Plik: `web/src/router/AppShell.tsx:25-30`

Current:

```tsx
const MOCK_STATS = { tps: 0, ledger: 0, accounts: 0, contracts: 0 };
<TopNav stats={MOCK_STATS} ... />
```

Fix — call existing `useNetworkStats()` in AppShell, map response
(`tps_60s` → `tps`, `latest_ledger_sequence` → `ledger`, `total_accounts`,
`total_contracts`), pass real values to TopNav. Hook lives at
`web/src/api/hooks/useNetworkStats.ts`, already used in
`web/src/pages/home/ChainOverview.tsx`. Loading state: TopNav gets
`stats | undefined` → render placeholder dashes during loading.

**H5 🟡 "TPS Last 60s = 0.0" misleading for historical backfill data**

Plik: `web/src/pages/home/ChainOverview.tsx` (TPS cell)

Backfill indexes 818-day-old historical ledgers → no recent TPS, "0.0"
looks like dead network.

Fix (simple, ship first): if `tps_60s === 0`, render "—" instead of "0.0".
Visually clear that data is unavailable, not "zero TPS".

Fix (later if needed): check `latest_ledger_closed_at` — if > 24h ago,
change caption from "Last 60s" to "(no recent activity)".

**H1 🟠 Network toggle Mainnet/Testnet visual-only**

Plik: `libs/ui/src/layout/NetworkSwitcher.tsx` + `web/src/router/AppShell.tsx:70`

Current: button changes local state but no config swap. No
`VITE_API_BASE_URL_TESTNET`. Click Testnet → nothing happens API-wise.

Fix for MVP (minimal scope): **cut toggle from UI**. AppShell passes
single-network mode (one env var). NetworkSwitcher stays in `libs/ui`
but unmounted in AppShell.

Full multi-network impl (runtime API base URL switch + per-env config)
deferred to a follow-up task — better no feature than broken stub.

### Commit 3 — C3 Transactions filter (3 bugs)

**H6 🟡 Op dropdown 5 → 27 ops (full backend parity)**

Plik: `web/src/pages/transactions/operationTypes.ts:17-23`

Current 5 hardcoded. Backend enum in
`crates/domain/src/enums/operation_type.rs:20-48` defines 27. User chose
full parity over curated subset on 2026-05-22.

Full 27 (Title Case labels):

```
CREATE_ACCOUNT, PAYMENT, PATH_PAYMENT_STRICT_RECEIVE, MANAGE_SELL_OFFER,
CREATE_PASSIVE_SELL_OFFER, SET_OPTIONS, CHANGE_TRUST, ALLOW_TRUST,
ACCOUNT_MERGE, INFLATION, MANAGE_DATA, BUMP_SEQUENCE, MANAGE_BUY_OFFER,
PATH_PAYMENT_STRICT_SEND, CREATE_CLAIMABLE_BALANCE,
CLAIM_CLAIMABLE_BALANCE, BEGIN_SPONSORING_FUTURE_RESERVES,
END_SPONSORING_FUTURE_RESERVES, REVOKE_SPONSORSHIP, CLAWBACK,
CLAWBACK_CLAIMABLE_BALANCE, SET_TRUST_LINE_FLAGS, LIQUIDITY_POOL_DEPOSIT,
LIQUIDITY_POOL_WITHDRAW, INVOKE_HOST_FUNCTION, EXTEND_FOOTPRINT_TTL,
RESTORE_FOOTPRINT
```

UX: verify current combobox supports search (MUI Autocomplete). If plain
Select, upgrade to Autocomplete — 27 entries warrants type-to-filter.

**H7 🟡 Combobox label "Manage Offer" maps to `MANAGE_SELL_OFFER` only**

Resolved automatically by H6 — full enum has separate entries:

```ts
{ label: 'Manage Sell Offer', value: 'MANAGE_SELL_OFFER' },
{ label: 'Manage Buy Offer', value: 'MANAGE_BUY_OFFER' },
```

**H2 🟠 Lowercase URL param `?op=manage_buy_offer` → backend 400**

Plik: `web/src/pages/TransactionsListPage.tsx:33-47`

Backend enum case-sensitive (UPPERCASE only). Lowercase URL → 400 →
SectionErrorBoundary → "Something went wrong".

Fix — normalize + validate:

```tsx
const op = (state.filters.op ?? '').toUpperCase().trim();
const validOp = OPERATION_TYPE_OPTIONS.some((o) => o.value === op) ? op : '';
if (validOp) filters['filter[operation_type]'] = validOp;
```

Better UX: invalid op silently ignored, not red banner.

### Commit 4 — C4 Error classification (1 bug)

**H8 🟡 `/ledgers/99999999999` → "Something went wrong" instead of NotFound**

Pliki: `libs/ui/src/states/classifyError.ts:18-38` (verify) +
`web/src/pages/LedgerDetailPage.tsx:68-84` + analogous detail pages.

Backend returns 400 INVALID_SEQUENCE for i64-overflow
(`crates/api/src/common/path.rs:145-155`). `classifyError` classifies
400 as `'validation'`. LedgerDetailPage only branches on `'not-found'`
→ falls through to GenericErrorState.

For `/ledgers/404` (in-range, no record) → 404 → 'not-found' → nice
"Ledger not found". Two different UX paths for what user perceives as
same concept ("this ledger doesn't exist").

Fix (preferred — Option A): extend LedgerDetailPage switch:

```tsx
if (kind === 'not-found' || kind === 'validation') {
  return <NotFoundState entity="Ledger" identifier={String(sequence)} />;
}
```

Unify same pattern across other detail pages (AccountDetailPage,
AssetDetailPage, ContractDetailPage, PoolDetailPage, NftDetailPage)
where applicable.

Option B (granular `InvalidIdentifierState`) deferred — Option A
sufficient for MVP.

### Commit 5 — C5 Search polish (1 bug)

**H10 🟢 "SearchRefine your query..." missing whitespace**

Plik: `web/src/pages/SearchResultsPage.tsx`

Heading "Search" + description "Refine your query..." rendered without
separator → concatenated text. Trivial inline fix — add space, `<br>`,
or separate element.

## Acceptance Criteria

- [ ] **C1 LP polish** — B1 routing fixed, B3 col swapped, B2 fee trimmed,
      B4 issuer suffix for non-native, B5 React keys composite.
- [ ] **C2 Network wiring** — H4 stats wired, H5 TPS "—" on zero, H1 toggle
      cut from UI.
- [ ] **C3 Filter UX** — H6 27 ops in dropdown, H7 separate Sell/Buy labels,
      H2 lowercase normalized + validated.
- [ ] **C4 Error states** — H8 LedgerDetailPage handles 'validation' as
      NotFound; pattern unified across other detail pages.
- [ ] **C5 Search polish** — H10 whitespace fixed.
- [ ] **Playwright MCP regression** — re-run QA over 13 routes after all
      5 commits land. Goal: 0 console errors, 0 generic-error-state hits
      for invalid-id paths, 0 missing-data placeholders where data exists.
- [ ] **Docs updated** — `N/A — frontend-only fixes, no architecture change.` Per ADR 0032.
- [ ] **API types regenerated** — `N/A — no changes under crates/api/**, Cargo.{toml,lock}, or libs/api-types/**.`
- [ ] **CI green** — `nx affected:lint`, `nx affected:test`,
      `nx affected:build` all pass before PR.

## Reused (no new code)

- `formatAmount(value, minDecimals)` — `web/src/pages/format.ts:12-26`
- `useNetworkStats()` — `web/src/api/hooks/useNetworkStats.ts`
- `classifyError()` — `libs/ui/src/states/classifyError.ts`
- `NotFoundState` — `libs/ui/src/states/NotFoundState.tsx`
- `IdentifierDisplay` / `IdentifierWithCopy` with `href` override —
  `libs/ui/src/identifiers/`
- `poolIdHexToStrkey()` — `web/src/utils/poolIdStrkey.ts`
- `routes.pool(hex)` — `web/src/router/routes.ts`
- `OPERATION_TYPE_OPTIONS` (extended in place) —
  `web/src/pages/transactions/operationTypes.ts`

Zero new npm deps, zero new UI components. Each bug = edit existing file.

## Verification

### Per-commit smoke (Playwright MCP)

| Commit       | Test                                                                                                                                                                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C1 LP        | `/liquidity-pools` → "Total shares" col, fee `0.30%`. Click pool → detail header link → hex URL. Find fake-XLM pool (both legs `code=XLM`) → KPI/Summary show issuer suffix `XLM (XXXX…YYYY)`, console clean. |
| C2 Network   | `/` topbar shows real numbers (ledger 50,49x,xxx, accounts 343k+, contracts 14k+, TPS real value or `—`). Mainnet/Testnet toggle absent from UI.                                                              |
| C3 Tx filter | `/transactions` → dropdown 27 ops with separate "Manage Sell Offer" + "Manage Buy Offer". URL `?op=manage_buy_offer` (lowercase) → FE uppercases, request `MANAGE_BUY_OFFER`, 200 OK, no generic error.       |
| C4 Errors    | `/ledgers/99999999999` → "Ledger not found" entity-specific. Same for `/accounts/INVALID`, `/assets/INVALID`, `/contracts/INVALID`, `/nfts/INVALID`, `/liquidity-pools/INVALID`.                              |
| C5 Search    | `/search?q=bogus` → "Search" heading + description with proper whitespace.                                                                                                                                    |

### Full regression after all 5 commits

Re-run QA traversal: navigate 13 routes, count console errors (target: 0),
re-verify each of 13 bugs from this task gone, spot-check that nothing
regressed on `/liquidity-pools` and `/liquidity-pools/:id` (chart
placeholder, participants empty, breadcrumb, stale badge — all from
0077 should remain intact).

### Unit tests (where applicable)

- `assetLegLabel` (helpers.ts) — extend existing test (if any) with
  case: classic_credit with `code='XLM'` and `issuer='GABC…WXYZ'` →
  `'XLM (GABC…WXYZ)'`.
- `formatAmount` — existing test should already cover trailing-zero
  trim.
- No new tests required for the remaining bugs — visual/wiring changes
  covered by Playwright e2e regression.

## Issues Encountered

_(to be filled during implementation)_

## Design Decisions

### From Plan

1. **Scope reduction 15 → 13 bugs.** H3 (asset URL inconsistency) dropped
   after backend spec lookup: `crates/api/src/assets/handlers.rs:47-67` +
   error msg lines 158-159 confirm three accepted `:id` formats by design
   (numeric / `C…strkey` / `CODE-ISSUER`). AssetsTable→numeric and
   AccountBalances→CODE-ISSUER are both correct per spec.
2. **H9 (ScVal decoder for Contract Events) deferred** to its own
   research task. No existing decoder, no `@stellar/stellar-base`
   dependency (~2MB). Needs backend serialization format research
   first. ~1 day work — doesn't fit a polish batch.
3. **Single lore task, 5 commits.** karolkow chose 1-task batch over
   5 separate tasks on 2026-05-22 (plan question).
4. **B4 format = always issuer for non-native (Option 1).** karolkow
   asked for recommendation 2026-05-22; chose predictable-over-smart.
5. **H6 = full 27 ops parity** (not curated). karolkow chose on
   2026-05-22.
6. **H1 toggle cut from UI** for MVP. Multi-network config swap
   deferred to dedicated follow-up task.
7. **H8 = Option A** (unified NotFoundState for 'not-found' OR
   'validation'). Option B (granular `InvalidIdentifierState`)
   deferred.

### Emerged

_(to be filled during implementation)_

## Future Work

After this batch lands:

1. **ScVal decoder for Contract Events** — separate `RESEARCH` or
   `FEATURE` task. Investigate backend serialization format (XDR string
   vs pre-decoded JSON), pick `@stellar/stellar-base` vs custom decoder.
2. **Network runtime toggle** — full multi-network implementation
   (per-env `VITE_API_BASE_URL`, runtime config swap, client recreate).
   Separate `FEATURE` task.
3. **Transaction detail page real implementation** — already tracked as
   0249 in archive (different 0249 — the one in archive is "destroy AWS
   infra"). Re-check whether the FE-side TransactionDetailPage stub
   replacement has its own follow-up task; if not, spawn one.
4. **Searchable Autocomplete for ops dropdown** — if MUI Select used
   today, may need upgrade to Autocomplete for usable 27-entry list.
   Decide during C3 implementation; spawn task if surface area exceeds
   commit scope.

(Each spawned as its own backlog task with `related_tasks: ['0251']`.)

## Notes

- **Plan file:** `~/.claude/plans/rozumiem-ze-przeszedles-teraz-abundant-backus.md`
  (approved 2026-05-22).
- **QA traversal evidence:** see plan file body + the consolidated bug
  report in the conversation history that birthed this task.
- **No commits without explicit signal** per project convention. Each
  of 5 commits awaits user instruction before `git commit`.
- **Worktree decision:** open. Likely a fresh worktree off develop
  (post-promote) so feat-0077 + the local axum::serve CORS edits on
  main.rs stay untouched.
- **Local main.rs edits (axum::serve + CORS layer)** — uncommitted,
  local-dev only. Lambda runtime stays in prod main.rs.
- **Backfill in background** — ~61% done at task creation, ~10.6M tx,
  12,859 pools, 343k accounts, 14,256 contracts indexed so far.
  Soroban-era window 50,457,424–50,521,423 (64,000 ledgers).
