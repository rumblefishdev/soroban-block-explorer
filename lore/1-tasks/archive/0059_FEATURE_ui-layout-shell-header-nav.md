---
id: '0059'
title: 'UI lib: layout shell, header, navigation, network indicator'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0058']
tags: [priority-high, effort-medium, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-14
    status: active
    who: karolkow
    note: 'Task activated'
  - date: '2026-05-14'
    status: completed
    who: claude
    note: >
      Implemented 6 components in libs/ui (TopNav, SecondaryNav, NavButton,
      NetworkSwitcher, SearchInput, Footer) + web AppShell wiring.
      8 files created/modified. Key emerged decisions: split into two-row
      header (TopNav + SecondaryNav), AppShell lives in web not libs/ui,
      Figma-driven hover fix in NavButton, testnet banner condition
      corrected from build-mode to network-mode.
---

# UI lib: layout shell, header, navigation, network indicator

## Summary

Implements the persistent layout shell for the explorer frontend. Shell consists of two-row header (TopNav: stats/search/network switcher; SecondaryNav: logo + nav links), a `<main>` content outlet, and a Footer. AppShell composition lives in `web/src/router/`, UI primitives in `libs/ui/src/layout/`.

## Status: Completed

## Context

Explorer needed a stable shell that persists across route transitions — no white-screen reload, always-visible network indicator, global search entrypoint. Replaced the placeholder `AppShellStub` with the real implementation.

## Implementation Plan → Actual

Plan proposed a single `Header.tsx` + `Navigation.tsx` + `AppShell.tsx` all in `libs/ui`. Actual implementation deviated (see Design Decisions).

## Acceptance Criteria

- [x] AppShell renders header, navigation, footer, and content area using semantic HTML (`<header>`, `<nav>`, `<main>`, `<footer>`)
- [x] Header displays logo, search bar slot, and network indicator
- [x] Navigation contains links to all top-level routes with active state (6 routes; Tokens/Contracts list pages do not exist yet — nav links match actual routes)
- [x] Network indicator shows mainnet or testnet and is always visible (NetworkSwitcher in TopNav)
- [x] Environment banner renders for non-production environments (TestnetBanner renders when `network === 'testnet'`)
- [x] Footer renders copyright, links, and network badge
- [x] Route transitions update only the content area; shell does not unmount/remount
- [x] Navigation is keyboard-accessible with proper tab order (NavButton uses native `<button>`/`<a>`)
- [x] Components use MUI theme from task 0058
- [x] All components exported from `libs/ui`

## Implementation Notes

**libs/ui/src/layout/ — new files:**

- `NavButton.tsx` — nav link with active underline, badge slot, hover from Figma
- `NetworkSwitcher.tsx` — mainnet/testnet toggle pill
- `SearchInput.tsx` — search bar with clear button
- `TopNav.tsx` — top row: NetworkSwitcher + live stats + SearchInput (renders as `<header>`)
- `SecondaryNav.tsx` — second row: logo + NavButton list (renders as `<nav>`)
- `Footer.tsx` — logo, Explorer links, Resources links, status badge, network badge, legal (renders as `<footer>`)
- `index.ts` — barrel for all layout exports

**web/src/router/ — modified:**

- `AppShell.tsx` — new; composes TopNav + SecondaryNav + `<main><Outlet/>` + Footer; owns network state + search state + SPA navigation
- `AppShellStub.tsx` — deleted (moved to `.trash/`)
- `index.tsx` — swaps `AppShellStub` → `AppShell`, removes ui-demo route

## Issues Encountered

- **Vite HMR stale after file delete**: After deleting `libs/ui/src/layout/AppShell.tsx` (libs version, deemed redundant), Vite kept erroring on HMR. Required hard reload to clear.
- **Pre-commit typecheck from wrong directory**: Running nx from main project root resolved the main project's `libs/ui` stub, not the worktree's. Fix: run nx explicitly from the worktree directory.
- **NavButton hover flicker**: `borderRadius` was set on the element always, not just on hover. Browser applied radius at rest but `background-color` transition made the mismatch visible as a flash. Fix: move `borderRadius` to `'&:hover'` only, add `border-radius` to CSS transition.
- **Full page reload on nav click**: NavButton rendered as `<a href>` + onClick calling `navigate()`. Native anchor fired after onClick → hard navigation. Fix: `e.preventDefault()` when both `href` and `onClick` are present.
- **Wrong env banner condition**: Initially coded banner as `import.meta.env.DEV` (build-mode). Task says "TESTNET banner" = network condition. Corrected to `network === 'testnet'`.
- **Nested semantic HTML**: TopNav/SecondaryNav/Footer each own their semantic element (`component="header/nav/footer"`). AppShell initially wrapped them in matching `<Box component=...>`. Removed wrapper `component=` props — components own their semantics.

## Design Decisions

### From Plan

1. **Composition via props, not context**: Shell state (network, search) passed down as props to UI components. Context deferred until task 0066 (TanStack Query / API client).

2. **Search bar slot in TopNav**: SearchInput lives in `libs/ui`; AppShell wires value/onChange/onSubmit from outside. Task 0060 can replace SearchInput with a more capable version without touching AppShell.

3. **Footer network badge**: Optional `network` prop on Footer renders mainnet (green) / testnet (yellow) badge in bottom bar per task AC.

### Emerged

4. **Two-row header, not one**: Figma design has two distinct rows — stats+search+network-switcher on top, logo+nav on second. Plan assumed one `Header.tsx`. Implemented as `TopNav` + `SecondaryNav` for cleaner separation and independent reuse.

5. **AppShell lives in web, not libs/ui**: libs/ui AppShell (proposed in plan) would have needed React Router as a dependency. Kept libs/ui as pure presentational components; AppShell wiring (useNavigate, useLocation, routing) lives in web where react-router-dom is already a dep.

6. **NavButton SPA fix — `e.preventDefault()` approach**: NavButton keeps `href` for right-click / open-in-new-tab semantics, plus `onClick` for SPA navigation. `e.preventDefault()` on click when both present prevents hard navigation while preserving anchor semantics.

7. **MOCK_STATS placeholder**: Stats (TPS, ledger seq, accounts, contracts) are zeros pending task 0066 API integration. Explicit placeholder, not missing feature.

8. **6 nav routes, not 7**: Plan listed Tokens and Contracts as top-level routes. Neither exists in the router today. Nav links match actual routes to avoid broken links.

## Future Work

- Connect stats (TPS, ledger seq, accounts, contracts) to live API — task 0066
- Global search navigation: task 0060 (search bar component)
- Responsive nav (collapsible on mobile) — deferred, not in task scope
