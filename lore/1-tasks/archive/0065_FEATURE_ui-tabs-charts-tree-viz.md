---
id: '0065'
title: 'UI lib: tabs, charts, and graph/tree visualization primitives'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-high, effort-large, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Promoted to active — Figma-first implementation of visualization primitives.'
  - date: 2026-05-18
    status: completed
    who: karolkow
    note: >
      Implemented 4 components + 2 hooks in libs/ui/src/visualization/
      (Tabs, TimeSeriesChart, OperationFlowTree, LazySection). Chose
      @mui/x-charts. Unified OperationFlowTree replaces the planned
      separate InvocationCallTree per Figma. typecheck/lint/build green.
---

# UI lib: tabs, charts, and graph/tree visualization primitives

## Summary

Implement tabs, time-series charts, operation flow tree/graph, and Soroban invocation call tree visualization components in `libs/ui/src/visualization/`. These are the most complex shared UI primitives in the explorer, used on transaction detail, contract detail, and liquidity pool detail pages.

## Status: Completed

**Current state:** Implemented on `feat/0065_ui-tabs-charts-tree-viz`.

## Context

Several explorer pages require rich visualization beyond simple tables:

- Transaction detail needs an operation flow tree (normal mode) and a Soroban invocation call tree
- Liquidity pool detail needs time-series charts for TVL, volume, and fee revenue
- Contract detail and transaction detail use tabbed sections

Design constraints:

- Tabs: no hard reloads, active tab reflected in URL
- Charts: time-series line/area for TVL, volume, fee revenue. Interval selector (1h/1d/1w) + date range. Responsive for small screens.
- Operation flow tree: source account to operations to affected accounts/contracts. Each node shows human-readable summary (e.g., "Sent 1,250 USDC to GD2M...K8J1"). Linked identifiers.
- Soroban invocation call tree: nested contract-to-contract hierarchy with function names
- Lazy loading: only load/render when section is visible or tab is active. Do not fetch chart data for offscreen sections.

## Implementation Plan

### Step 1: Tab component

Create `libs/ui/src/visualization/Tabs.tsx`:

- Props: tab definitions (label, key), active tab, onChange callback
- Active tab synced to URL query param (e.g., `?tab=invocations`)
- No hard reload on tab change -- React Router search param update
- MUI Tabs as base with explorer-specific styling
- Keyboard accessible: arrow keys navigate tabs, Enter/Space activates

### Step 2: Time-series chart component

Create `libs/ui/src/visualization/TimeSeriesChart.tsx`:

- Props: data points (timestamp + value series), chart type (line/area), title, y-axis label
- Interval selector: 1h, 1d, 1w buttons that trigger data re-fetch
- Date range picker (optional, for custom ranges)
- Responsive: readable on small screens, axes adapt
- Tooltips on hover showing exact values and timestamps
- Lazy rendering: use IntersectionObserver to only render when visible

### Step 3: Operation flow tree component

Create `libs/ui/src/visualization/OperationFlowTree.tsx`:

- Renders transaction operation flow as a graph/tree structure
- Nodes: source account (root) -> operations -> affected accounts/contracts
- Each node displays human-readable summary (e.g., "Sent 1,250 USDC to GD2M...K8J1", "Swapped 100 USDC for 95.2 XLM on Soroswap")
- Identifiers in nodes are linked (using identifier components from task 0062)
- Expandable/collapsible for complex transactions
- Supports both classic operations and Soroban invocations

### Step 4: Soroban invocation call tree

Create `libs/ui/src/visualization/InvocationCallTree.tsx`:

- Renders nested contract-to-contract invocation hierarchy
- Each node shows: contract ID (linked), function name, status
- Nested calls indented to show caller-callee relationship
- Expandable/collapsible for deep call stacks
- Function names displayed prominently, contract IDs as secondary

### Step 5: Lazy loading wrapper

Create `libs/ui/src/visualization/LazySection.tsx`:

- Wraps chart or visualization sections
- Uses IntersectionObserver to detect visibility
- Only renders children (and triggers data fetch) when section enters viewport
- Shows placeholder/skeleton until visible
- Used for chart sections and heavy visualizations on detail pages

### Step 6: Exports

Export all visualization components from `libs/ui` barrel.

## Acceptance Criteria

- [x] Tabs render with active state synced to URL query params
- [x] Tab changes do not cause hard page reloads
- [x] Tabs are keyboard accessible (arrow keys, Enter/Space)
- [x] TimeSeriesChart renders line/area charts with interval selector — built `1D/7D/30D/1Y` per Figma, not `1h/1d/1w` (see Design Decisions)
- [x] Charts are responsive and readable on small screens
- [x] Charts show tooltips with exact values on hover
- [x] OperationFlowTree renders source account -> operations -> affected entities as tree/graph
- [x] Each tree node shows human-readable summary with linked identifiers
- [x] InvocationCallTree renders nested contract-to-contract hierarchy with function names — merged into the unified `OperationFlowTree` per Figma; no separate component
- [x] Call tree supports expandable/collapsible nodes for deep hierarchies
- [x] LazySection only renders when section is visible in viewport (presentational — children own any fetch)
- [x] All components exported from `libs/ui`

> All criteria are met in code. Visual 1:1 verification against Figma is
> pending a consumer page — no render harness exists yet; it lands with
> tasks 0070 / 0075 / 0077.

## Notes

- This is the largest effort task in the frontend shared layer due to the variety of visualization types.
- Chart library selection (e.g., Recharts, Nivo, Victory) should be decided during implementation. Prioritize lightweight bundle size and React compatibility.
- The operation flow tree is consumed by transaction detail normal mode (task 0070).
- The invocation call tree is consumed by both transaction detail (task 0070) and contract detail (task 0075).
- Time-series charts are consumed by liquidity pool detail (task 0077).

## Implementation Notes

Branch `feat/0065_ui-tabs-charts-tree-viz`, implementation commit `9b27304`.

New files in `libs/ui/src/visualization/`:

- `Tabs.tsx` — controlled tab bar on the explorer-themed MUI Tabs; optional
  per-tab count badge; keyboard accessible.
- `useTabUrlState.ts` — `?tab=` URL-sync hook, mirrors `useTableUrlState`.
- `TimeSeriesChart.tsx` — `@mui/x-charts` line/area chart, `1D/7D/30D/1Y`
  range presets, line glow, loading/empty states.
- `OperationFlowTree.tsx` — unified flow tree: typed colour-coded node cards
  (account / contract / destination / result), labelled connectors, nested
  Soroban invocations, expand/collapse.
- `LazySection.tsx` + `useIntersectionObserver.ts` — viewport-gated render.
- `index.ts` — sub-barrel; re-exported from the `libs/ui` root barrel.

Other changes: `@mui/x-charts ^9.2.0` added to `libs/ui` dependencies;
`IdentifierDisplay` gained an additive opt-in `tone` prop.

Verification: `typecheck`, `lint`, `build` all green. No unit tests —
`libs/ui` has no test infra (task 0226). No visual verification yet — see
Acceptance Criteria note.

Docs (ADR 0032): `docs/architecture/frontend/frontend-overview.md` — N/A; it
already lists "tabs, charts, and graph/tree visualization primitives" as a
shared-UI category and the documented architecture shape is unchanged.

## Design Decisions

### From Plan

1. **`@mui/x-charts` for charts** — the spec deferred the library choice;
   picked `@mui/x-charts` for MUI-7 theme consistency (same Emotion/`sx`).
2. **Presentational / controlled components** — components emit callbacks;
   the consuming page owns data fetching (matches `ExplorerTable`).
3. **Single `LazySection` wrapper** — one IntersectionObserver wrapper rather
   than per-component, per task 0077's "Lazy-loaded via LazySection".

### Emerged

4. **Unified `OperationFlowTree`** — Figma renders the operation flow and the
   Soroban call tree as one tree; built a single component instead of the
   planned separate `InvocationCallTree` (Steps 3 & 4).
5. **Interval labels `1D/7D/30D/1Y`** — Figma shows range presets, not the
   spec's `1h/1d/1w` bucket granularity. The component takes a generic
   `intervals[]`; defaults follow Figma.
6. **Tabs count-badge** — Figma contract-detail tabs show a per-tab count
   pill; added an optional `count` to `TabDefinition` (not in Step 1).
7. **`IdentifierDisplay` `tone` prop** — `IdentifierDisplay` hardcoded
   `text.primary`, unreadable on the coloured flow-tree node cards. Added an
   opt-in `tone: 'inherit'` (additive, default unchanged) so `OperationFlowTree`
   reuses it instead of a duplicate identifier component.
8. **`useTabUrlState` as a separate hook** — the spec did not specify; split
   out to mirror the existing `useTableUrlState`.

### Spec ↔ design notes

- Notes line 122 ("invocation call tree consumed by 0075") is stale —
  contract detail uses a flat table; the tree is consumed by 0070 only. Left
  in the spec, recorded here.
- The Figma LP chart panel tabs read `TVL / Invocations / Events` while spec
  0077 and the Figma annotation table say TVL / volume / fee revenue — likely
  a copy-paste placeholder. Flagged for design / task 0077; does not affect
  0065 (its `Tabs` + `TimeSeriesChart` are generic).

## Issues Encountered

- The activation commit (`c7aa9dd`) renamed the task file backlog→active but
  the frontmatter `status` edit was unstaged at commit time, so it landed as a
  pure rename. Corrected forward in the completion commit (status + history).
- No `libs/ui` test infra and no consumer page exist yet — components compile
  and pass typecheck/lint/build but are not visually verified.

## Future Work

- Visual 1:1 verification and first real wiring happen in the already-planned
  consumer tasks 0070 (transaction detail), 0075 (contract detail) and 0077
  (liquidity pool detail). No new backlog task needed.
