---
id: '0059'
title: 'UI lib: layout shell, header, navigation, network indicator'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0058', '0060', '0062']
tags: [priority-high, effort-medium, layer-frontend-shared]
milestone: 2
links:
  - 'https://www.figma.com/design/siumLgKOc9LLepEfbimyp3/Design-System---Stellar-Block-Explorer?node-id=8946-1330'
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-13
    status: active
    who: karolkow
    note: 'Activated. Branching off feat/0062 (carries 0058 theme scaffold + color tokens). Scope reconciled with Figma Navigation bar (node 8946:1330) + 2nd navbar (node 8947:1926).'
---

# UI lib: layout shell, header, navigation, network indicator

## Summary

Implement the persistent layout shell for the explorer frontend in `libs/ui/src/layout/`. Two stacked bars sit above the content area:

1. **Top bar** — interactive Mainnet/Testnet **switcher** (left), live network stats inline (TPS, Ledger, Accounts, Contracts), global search input slot (right, `CTRL+F` hint).
2. **2nd navbar** — Rumblefish logo (left, links to `/`), entity nav links (right).

Shell stays stable across route transitions — only content area updates.

## Status: Active

## Context

The explorer frontend needs a consistent, always-visible layout shell that frames every page. The shell provides orientation (where am I, what network), quick access to all entity categories, and the global search entrypoint. Without a stable shell, route transitions cause white-screen reloads and users lose navigation context.

Design philosophy from the architecture docs:

- Data-first, explorer-oriented layout. Scanability over marketing.
- Collection screens are list-heavy; detail screens show concise summary first.
- Route transitions must preserve shell rendering -- no white-screen reload. Route changes update the content area while header/nav remain stable.
- Network indicator always visible to prevent mainnet/testnet confusion.

## Implementation Plan

### Step 1: Layout shell wrapper

Create `libs/ui/src/layout/AppShell.tsx` providing the outer frame: top bar region, 2nd navbar region, and `<main>` content outlet. Use semantic HTML (`<header>`, `<nav>`, `<main>`). Content area accepts `children` (React Router `<Outlet>`).

### Step 2: Top bar component (`TopBar.tsx`)

Layout left → right:

1. **`NetworkSwitcher`** (left) — interactive Mainnet/Testnet pill toggle (NOT a passive indicator). States: Default, Hover, Active. Persists choice (URL param or localStorage). On change, propagates network to app context. Figma: `Mainnet / Testnet tabs` (node 8946:1168).
2. **`HeaderStats`** (centre) — inline live counters: `TPS`, `Ledger`, `Accounts`, `Contracts`. Values dimmed in search-active mode. Data source out-of-scope here (stub with `network.stats` props; populated by `useNetworkStats` hook from `web/src/api/`, task 0066). Figma shows label-value pairs (e.g. `TPS 142.3`).
3. **`SearchSlot`** (right) — composition slot for global search input (task 0060). Renders `CTRL+F` keyboard shortcut hint inside the input. Search-active state expands input + collapses stats.

Testnet styling = top bar accent (orange underline + tinted text), driven by theme `network` param from task 0058. No separate "TESTNET" banner element — colour of the bar IS the banner.

Four visual variants (per Figma `Mainnet / testnet nav-bar` 8946:1221):

- Mainnet light
- Testnet light (orange accent)
- Search-active dark
- Search-active light

### Step 3: 2nd navbar component (`MainNav.tsx`)

Layout left → right:

1. **Logo** (left) — Rumblefish logo, renders as `<Link to="/">`. Acts as Home link; no separate "Home" nav entry.
2. **Nav links** (right-aligned cluster) — in order:
   - Transactions → `/transactions`
   - Accounts → `/accounts` _(see §Notes — confirm w/ fmazur; absent from `docs/architecture/frontend/frontend-overview.md` §7, present in Figma `2nd navbar` 8947:1926)_
   - Ledgers → `/ledgers`
   - Assets → `/assets` _(Figma label is "Tokens" — stale post `tokens→assets` rename; route + label both follow docs)_
   - Contracts → `/contracts`
   - NFTs → `/nfts`
   - Liquidity Pools → `/liquidity-pools`
3. Active link = underline + bold (Figma variants `Active page=…`).
4. Route param naming per `docs/architecture/frontend/frontend-overview.md` §6.1: `:accountId`, `:contractId`, `:sequence`, `:hash`, `:id`.
5. Responsive: collapse to hamburger menu on small screens.

### Step 4: NetworkSwitcher (`NetworkSwitcher.tsx`)

Standalone component used by `TopBar`:

- Two-segment pill toggle (Mainnet / Testnet)
- Reads + writes app-level network context
- States: Default, Hover, Active (per Figma `Toggle buttons` 8965:2601)
- Keyboard accessible (Tab + Enter/Space, ArrowLeft/Right between segments)
- ARIA: `role="radiogroup"`, segments `role="radio"` with `aria-checked`

### Step 5: HeaderStats (`HeaderStats.tsx`)

Stub component:

- Props: `{ tps, ledger, accounts, contracts }`, each optional
- Renders label-value inline pairs with separator dots
- Skeleton state for undefined values
- Real data wiring deferred to task 0066 (TanStack Query hooks)

### Step 6: Integration and exports

Export all layout components from `libs/ui` barrel. Shell composes with React Router `<Outlet>` for content area.

## Acceptance Criteria

- [ ] AppShell renders top bar, 2nd navbar, and content area using semantic HTML (`<header>`, `<nav>`, `<main>`)
- [ ] Top bar left→right: `NetworkSwitcher`, `HeaderStats` (TPS/Ledger/Accounts/Contracts), `SearchSlot` (with `CTRL+F` hint)
- [ ] `NetworkSwitcher` is interactive (toggle), not a passive indicator; persists selection; updates app network context
- [ ] Testnet variant tints the top bar (orange accent) — no separate banner element
- [ ] 2nd navbar: Rumblefish logo links to `/`; nav cluster contains Transactions, Accounts, Ledgers, Assets, Contracts, NFTs, Liquidity Pools
- [ ] Active nav link rendered with underline + bold matching Figma `Active page=…` variants
- [ ] Route transitions update only the content area; shell does not unmount/remount
- [ ] Keyboard accessibility: switcher (radiogroup semantics), nav links (Tab order), search slot focusable
- [ ] Components consume MUI theme from task 0058 (`network` + `mode` params drive variant)
- [ ] All components exported from `libs/ui` barrel

## Notes

- Global search bar implementation = task 0060. This task provides only the slot.
- MUI theme = task 0058. This task consumes it via `ExplorerThemeProvider` (`network` + `mode` params).
- `HeaderStats` values wired to live data in task 0066 (TanStack Query). Stub props here.
- **Open question — `Accounts` in nav**: Figma `2nd navbar` 8947:1926 includes `Accounts`, docs §7 omit it (account = detail page only). Implementing per Figma; flag for fmazur to confirm or update Figma.
- **Open question — Figma "Tokens" label**: Figma still shows `Tokens` in 2nd navbar; after `tokens → assets` rename (commit 7f2dcee) UI label + route both follow docs (`Assets` / `/assets`). Figma needs update — flag for fmazur.
- Nav link list should remain easy to extend if new entity types are added.

## Figma references

- Navigation bar frame: `8946:1330`
- Top bar full strip variants: `8946:1221` (`Mainnet / testnet nav-bar`)
- Network switcher: `8946:1168` + `8946:1190`
- Toggle button states: `8965:2601`
- 2nd navbar with active states: `8947:1926`
- Nav buttons (MD/LG sizes): `8947:1725`
