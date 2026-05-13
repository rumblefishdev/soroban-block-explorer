---
id: '0062'
title: 'UI lib: identifier display, copy button, linked identifiers'
type: FEATURE
status: active
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
  - date: 2026-05-13
    status: active
    who: karolkow
    note: 'Activated — implementing from Figma DS (siumLgKOc9LLepEfbimyp3): Copy button (8938:982) + Table cell (8938:1082)'
---

# UI lib: identifier display, copy button, linked identifiers

## Summary

Implement identifier display components in `libs/ui/src/identifiers/` that provide consistent rendering, truncation, copy-to-clipboard, and deep linking for all entity identifiers across the explorer. Every hash, account ID, contract ID, token ID, pool ID, and ledger sequence in the app must look and behave identically.

## Status: Active

**Current state:** Implementation done; tooltips removed per design lead decision (see Emerged below). Pending review + commit.

## Context

Identifiers are the primary navigation anchors in a block explorer. Users constantly scan, copy, and click identifiers to move between entities. Visual consistency is critical -- the same identifier must look the same whether it appears in a table row, a detail page header, or a search result.

Linkable entity types and their routes:

- Transaction hash: `/transactions/:hash`
- Account ID: `/accounts/:id`
- Contract ID: `/contracts/:id`
- Token ID: `/tokens/:id`
- Pool ID: `/liquidity-pools/:id`
- Ledger sequence: `/ledgers/:seq`

Display requirements:

- Visually identical everywhere: same truncation rules, font, hover behavior, link styling
- Copy confirmation: brief non-intrusive tooltip "Copied!" for 1-2 seconds
- Each component accepts entity type to determine link target
- Dependency on `libs/domain` for identifier format validation utilities

## Implementation Plan

### Step 1: Identifier display component

Create `libs/ui/src/identifiers/IdentifierDisplay.tsx`:

- Props: `value` (full identifier string), `type` (entity type enum), `truncate` (boolean, default true), `linked` (boolean, default true)
- Truncation: shows first N and last M characters with ellipsis (e.g., "GABC...XYZ1")
- Full value shown on hover via tooltip
- Monospace font for all identifiers
- When `linked=true`, renders as a React Router `<Link>` to the appropriate detail page based on `type`

### Step 2: Copy button component

Create `libs/ui/src/identifiers/CopyButton.tsx`:

- Small icon button adjacent to identifier
- On click: copies full identifier value to clipboard
- Shows brief "Copied!" tooltip for 1-2 seconds, then reverts
- Non-intrusive: does not displace layout or obscure content
- Accessible: aria-label "Copy to clipboard", announces copy success

### Step 3: Composed identifier with copy

Create `libs/ui/src/identifiers/IdentifierWithCopy.tsx`:

- Composes `IdentifierDisplay` + `CopyButton` inline
- For full-length display contexts (detail page headers): show full value + copy button
- For table/list contexts: show truncated value + copy on hover/focus

### Step 4: Entity type routing map

Create `libs/ui/src/identifiers/identifierRoutes.ts`:

- Maps entity type enum to route pattern
- Used by `IdentifierDisplay` to generate correct `<Link>` target
- Types: transaction, account, contract, token, pool, ledger, nft

### Step 5: Identifier format validation (libs/domain)

Add or verify identifier format validation utilities in `libs/domain`:

- Transaction hash: 64-character hex
- Account ID: G... format (Stellar public key)
- Contract ID: C... format
- Ledger sequence: positive integer
- Token ID, Pool ID, NFT ID: string validation

### Step 6: Exports

Export all identifier components from `libs/ui` barrel.

## Acceptance Criteria

- [x] IdentifierDisplay renders with consistent truncation, font, and hover behavior everywhere
- [ ] Truncated identifiers show full value on hover via tooltip — **deviated, see Emerged #4** (Figma `Table cell` identifier has no such tooltip; matched Figma)
- [x] Linked identifiers navigate to the correct detail page based on entity type (`getIdentifierHref`)
- [x] CopyButton copies full value to clipboard and shows "Copied!" tooltip for 1-2 seconds (1.5s)
- [x] Copy confirmation is non-intrusive (tooltip per Figma DS `8956:2020`, above the button)
- [x] IdentifierWithCopy composes display and copy for both full and truncated contexts
- [x] Entity type routing map covers: transaction, account, contract, token, pool, ledger, nft
- [x] Monospace font used for all identifier strings (`monoFontFamily` exported from theme)
- [x] Keyboard accessible: copy button focusable and activatable via Enter/Space (MUI IconButton)
- [x] All components exported from `libs/ui`

## Implementation Notes

Files created under `libs/ui/src/identifiers/`:

- `types.ts` — `EntityType` union + `TruncationConfig`
- `routes.ts` — `getIdentifierHref(type, id)` with `encodeURIComponent`
- `truncate.ts` — `truncateMiddle` + per-type defaults (tx 12+8, acc/contract/token/pool/nft 6+4, ledger 0+0)
- `validators.ts` — regex validators (`isTransactionHash`, `isAccountId`, `isContractId`, `isLedgerSequence`, `isValidIdentifier`)
- `CopyButton.tsx` — MUI `IconButton` with three visual states (default / hover / copied) sourced from DS variables (`Text/Primary`, `Surface/Gray/Hoover`, `Black`, `Surface/Primary/Main`)
- `IdentifierDisplay.tsx` — `Box`-based polymorphic render (`<a>` when linked, `<span>` otherwise)
- `IdentifierWithCopy.tsx` — inline-flex composition
- `index.ts` — barrel

Theme touched: added `monoFontFamily` export in `libs/ui/src/theme/typography.ts` + re-export through `theme/index.ts` and root `libs/ui/src/index.ts`.

Build / verify:

- `nx typecheck` ✓
- `nx lint` ✓
- `nx build` ✓ (rollup, ~215 kB raw / ~57 kB gz)
- Visual verification in `web/` dev server (demo + alias used temporarily, reverted before commit)

## Design Decisions

### From Plan

1. **Truncation defaults per entity type** — task spec called for "sensible defaults (6+4)". Picked 12+8 for transaction hashes (matches Figma `Table cell` 8938:1082 visual), 6+4 for Stellar account/contract/token/pool/nft, 0+0 (no truncate) for ledger sequences.
2. **Pills-shaped copy button (radius 9999)** — direct from Figma `Copy button` 8938:982 (`Corner Radius/pills`).
3. **Linked color = `text.accent`** — Figma `Table cell` linked variant matches `#fdda24` (primary accent).

### Emerged

4. **Tooltip with full value on hover (IdentifierDisplay) — removed.** Plan called for tooltip with full value on hover. Figma `Table cell` identifier (`8938:878`) is a plain `<p>` with no tooltip — implementation now matches Figma exactly. Trade-off: users who need full value must click through to detail page or use copy button. Deviates from plan, aligns with design.
5. **Identifier color: black at rest, gold on hover (linked).** Initial implementation used `text.accent` (yellow) for linked identifiers at rest. Figma `Table cell` Hash variants show: default → `text/primary` (black), Hover=True → `surface/primary/main-alt` (#a36905 light / #fdda24 dark). Fixed to match Figma color states. No underline (Figma uses color-only hover signal).
6. **Pressed icon color hardcoded `#000000`.** Early version used `text.secondary` which in dark mode resolves to `#d3d3d3` (near-white) → invisible on yellow. Fix mirrors Figma `8938:903` (`Black` token).
7. **Validators live in `libs/ui/src/identifiers/validators.ts`, not `libs/domain`.** Plan step 5 referenced `libs/domain` which does not exist in this repo yet. Kept validators inline; migration to a future `libs/domain` is a follow-up (see Future Work).
8. **Routing is `href`-based, not React Router `<Link>`.** Plan referenced `<Link>`; the repo has no router setup yet and `libs/ui` should remain router-agnostic. `IdentifierDisplay` renders `<a href>` (works under any router via interception). Migration to router-aware component is a follow-up.
9. **`monoFontFamily` exported from theme.** Figma uses `JetBrains Mono Medium` for identifier text (`Body/Mono/Small/500`). Centralized in `libs/ui/src/theme/typography.ts` with system mono fallbacks for portability.

## Future Work

- Migrate validators from `libs/ui/src/identifiers/validators.ts` to `libs/domain/...` when the domain lib is created. Consumers should swap imports; barrel can transparently re-export for a transition window.
- Once the React Router setup lands (task 0066 area), audit whether `IdentifierDisplay` should render a router `<Link>` for SPA navigation instead of plain `<a>`. Likely a thin wrapper variant rather than coupling `libs/ui` to a router.

## Notes

- This component set is used by virtually every page in the explorer. Consistency is paramount.
- Identifier format validation in `libs/domain` may already be partially implemented from tasks 0009-0012.
- The truncation algorithm should be configurable but have sensible defaults (e.g., 6 chars + ... + 4 chars).
