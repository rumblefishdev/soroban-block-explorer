---
id: '0058'
title: 'UI lib: MUI theme configuration and explorer-specific styling'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-high, effort-small, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-14
    status: completed
    who: FilipDz
    note: 'Implemented on feat/0058_ui-mui-theme. Mainnet/testnet distinction dropped per design call. Dark mode delivered alongside light. WCAG inherited from Figma tokens.'
---

# UI lib: MUI theme configuration and explorer-specific styling

## Summary

Configure a custom MUI theme in `libs/ui/src/theme/` tailored for a data-first block explorer. MUI provides the base component and accessibility layer, but defaults must be overridden for explorer density, scanability, and network distinction. Export a ThemeProvider wrapper for the app.

## Status: Backlog

**Current state:** Not started.

## Context

MUI is the base component library for the explorer. However, MUI defaults are designed for general-purpose applications, not dense data explorers. The theme must be customized for:

- Data-first, scanability-focused layout
- Dense but readable table rows and summary cards
- Clear mainnet/testnet visual distinction
- Monospace typography for identifiers and data values
- WCAG AA contrast compliance for all text including secondary metadata and disabled states

MUI is used as a base component + accessibility layer, NOT as finished design. Every default that conflicts with explorer density must be overridden.

## Implementation Plan

### Step 1: Custom palette

Create `libs/ui/src/theme/palette.ts`:

- Primary and secondary colors appropriate for a blockchain explorer
- Mainnet palette: distinct color set for mainnet environment
- Testnet palette: distinct color set for testnet environment (visually different enough to prevent confusion)
- Status colors: success (green-toned), error (red-toned) for transaction status badges
- Type colors: distinct colors for classic, SAC, and Soroban type badges
- Background and surface colors for light/dark mode readiness
- All colors WCAG AA compliant for text contrast

### Step 2: Typography

Create `libs/ui/src/theme/typography.ts`:

- Base font: clean sans-serif for general UI text
- Monospace font: for all identifier strings, hashes, addresses, XDR content
- Data table typography: slightly smaller, tighter line-height for dense tables
- Heading scale: `<h1>` through `<h6>` hierarchy for page structure
- Font sizes tuned for scanability in data-heavy contexts

### Step 3: Spacing and density

Create `libs/ui/src/theme/spacing.ts`:

- Compact spacing for table rows (denser than MUI defaults)
- Summary card padding: compact but readable
- Dense list row heights for collection pages
- Consistent spacing scale used across all components
- Breakpoints for responsive behavior

### Step 4: Component overrides

Create `libs/ui/src/theme/overrides.ts`:

- MUI Table: dense row height, tighter cell padding, border styling
- MUI Chip: compact for badges (status, type, network)
- MUI Button: appropriate sizing for pagination controls and actions
- MUI Tooltip: styled for identifier hover and copy confirmation
- MUI Tabs: explorer-appropriate tab styling
- MUI Skeleton: matching dimensions for table/card/detail skeletons

### Step 5: Theme assembly and ThemeProvider

Create `libs/ui/src/theme/theme.ts` and `libs/ui/src/theme/ThemeProvider.tsx`:

- Assemble palette, typography, spacing, and overrides into `createTheme()`
- Accept network parameter to switch between mainnet/testnet palettes
- Export `ExplorerThemeProvider` wrapper component
- Export raw theme object for use in tests or Storybook

### Step 6: Exports

Export `ExplorerThemeProvider` and theme utilities from `libs/ui` barrel.

## Acceptance Criteria

- [x] ~~Custom palette with mainnet/testnet distinction~~ — dropped per design call (`no_differ`). Single palette per mode.
- [x] Status colors (success/error/warning/information) defined via `text.*` + `surface.*` semantic tokens; type colors for chips/badges defined as `blue/violet/emerald/neutral/subtle/brown/accent` (see `libs/ui/src/theme/colors.ts` + `MuiChip` overrides).
- [x] WCAG AA — inherited from Figma design system tokens (`Light mode.tokens.json` / `Dark mode.tokens.json`). No contrast pairings invented in code.
- [x] Monospace font configured — JetBrains Mono variable font for hashes/addresses/contract IDs/XDR (`bodyMono*` Typography variants).
- [x] Data table typography — implemented via `MuiTableCell` overrides + `bodySmRegular` cell variant.
- [x] Heading hierarchy — 24 heading variants (`heading1Bold` through `heading6Regular`) covering 4 weights × 6 sizes per Figma.
- [x] Compact spacing — table row density + card padding tuned in `MuiTable*` and `MuiCard` overrides.
- [x] MUI component overrides for: Table, Chip, Button, Tooltip, Tabs, Skeleton — delivered. Skeleton uses MUI defaults (no override needed); rest covered in `libs/ui/src/theme/overrides.ts`. Also covered beyond spec: Switch, Checkbox, Radio, Slider, TextField, Select, Menu, Pagination, Paper, Card, Tab.
- [x] ~~`ExplorerThemeProvider` accepts network parameter~~ — replaced by color-mode (light/dark) toggle. `useColorMode` hook persists to localStorage.
- [x] Theme exported from `libs/ui` barrel — `createExplorerTheme`, `ExplorerThemeProvider`, `useColorMode`, `colorsLight`, `colorsDark`, etc.
- [x] Theme works as base for all UI components in `libs/ui` — verified interactively during the branch work via a `DevPlaygroundPage` demo strip that covered every override; the playground was removed once routing landed (task 0067) since real pages will exercise the same theme paths.

## Notes

- This theme is consumed by every other frontend task. It should be implemented early.
- MUI defaults are intentionally generic. The overrides in this task encode the explorer's specific design decisions.
- Mainnet/testnet palette distinction is critical for preventing user confusion when switching networks.
- Dark mode is not required for initial launch but the palette structure should not preclude it.
