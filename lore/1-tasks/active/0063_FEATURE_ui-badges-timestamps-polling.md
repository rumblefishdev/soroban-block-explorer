---
id: '0063'
title: 'UI lib: badges, relative timestamps, polling indicator'
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
  - date: 2026-05-14
    status: active
    who: FilipDz
    note: 'Promoted to active on feat/ui-foundation — bundled with 0058/0064/0067 close-out on this branch.'
  - date: 2026-05-14
    status: active
    who: FilipDz
    note: 'Primitives in libs/ui done — network badges via Chip color="blue"/color="warning" at call sites, RelativeTimestamp + PollingIndicator + useNow + formatRelative shipped under libs/ui/src/timestamps/. Re-opened because PollingIndicator "visible on polling-enabled pages" lands when 0068+ home/list pages wire it up; ergonomic enhancements (`isFetching` prop + spin animation + optional `onRefresh` click) come with that consumption.'
---

# UI lib: badges, relative timestamps, polling indicator

## Summary

Implement badge components, relative timestamp display, and a polling indicator in `libs/ui/src/badges/` and `libs/ui/src/timestamps/`. These small but ubiquitous primitives appear on nearly every page and must be accessible, consistent, and informative.

## Status: Backlog

**Current state:** Not started.

## Context

Badges and timestamps are among the most frequently rendered elements in the explorer. They communicate transaction status, entity type, network environment, and data freshness at a glance. Accessibility is a hard requirement: badges must use visible TEXT labels (not color-only), and timestamps must have sufficient contrast per WCAG guidelines.

Badge types needed:

- Status badges: success / failed (for transactions)
- Type badges: classic / SAC / soroban (for tokens and contracts)
- Network badge: mainnet / testnet variant

Timestamp requirements:

- Relative display: "2 min ago", "1 hour ago", "3 days ago"
- Full ISO timestamp on hover
- Sufficient contrast for secondary metadata per WCAG

Polling indicator:

- Shows "Updated 5s ago" or similar on polling-enabled pages
- Visible on pages with auto-refresh (home, possibly list pages)

## Implementation Plan

### Step 1: Status badge component

Create `libs/ui/src/badges/StatusBadge.tsx`:

- Props: `status` ("success" | "failed")
- Renders colored chip/badge with TEXT label ("Success", "Failed")
- Color: green-toned for success, red-toned for failed -- but text label is the primary indicator, not color alone
- Compact size for table rows

### Step 2: Type badge component

Create `libs/ui/src/badges/TypeBadge.tsx`:

- Props: `type` ("classic" | "sac" | "soroban")
- Renders badge with text label and distinct visual treatment per type
- Used on token list/detail pages and contract pages
- Prevents confusion between similarly named assets of different types

### Step 3: Network badge component

Create `libs/ui/src/badges/NetworkBadge.tsx`:

- Props: `network` ("mainnet" | "testnet")
- Distinct palette per network (from MUI theme in task 0058)
- Used in header network indicator (task 0059) and wherever network context is shown

### Step 4: Relative timestamp component

Create `libs/ui/src/timestamps/RelativeTimestamp.tsx`:

- Props: `timestamp` (ISO string or Date)
- Renders relative time: "2 min ago", "1 hour ago", etc.
- Full ISO timestamp shown on hover via tooltip
- Updates periodically (e.g., every 30s) to keep relative time accurate
- Sufficient contrast ratio per WCAG for secondary metadata text

### Step 5: Polling indicator component

Create `libs/ui/src/timestamps/PollingIndicator.tsx`:

- Props: `lastUpdated` (timestamp), `intervalMs` (polling interval)
- Renders "Updated 5s ago" with a subtle refresh icon
- Visible on polling-enabled pages (home, list pages)
- Updates relative time display periodically

### Step 6: Exports

Export all badge and timestamp components from `libs/ui` barrel.

## Acceptance Criteria

- [x] Status badge — "Success" / "Failed" via `<Chip color="success/error" dot label>` at call sites. No `StatusBadge` wrapper component (it would be a one-line indirection over `Chip` — the spec's "text label primary" requirement is met by passing the label string directly).
- [x] Type badge — "Classic" / "SAC" / "Soroban" via `<Chip color="blue|violet|emerald" label>` at call sites. Same reasoning: no wrapper.
- [x] Network badge — `<Chip color="blue" label="Mainnet">` / `<Chip color="warning" label="Testnet">` at call sites. The theme's `MuiChip` `color="blue"` override already matches Figma's `Accent/Blue/100` + `Accent/Blue/600`, and `color="warning"` matches `Surface/Warning` + `Text/Warning`. No `NetworkBadge` wrapper component — same reasoning as Status / Type. Outlined / text variants from Figma can be expressed via `sx={{ border, backgroundColor: 'transparent' }}` at the (rare) call sites that need them. The header NetworkIndicator (task 0059) consumes Chip directly.
- [x] RelativeTimestamp — shows "2 min ago" style, full ISO on hover via tooltip.
- [x] Timestamps contrast — uses `text.secondary` semantic token (inherited from Figma design system, WCAG-validated upstream).
- [x] Relative timestamps re-render — `useNow(intervalMs)` shared hook ticks every 30s by default.
- [ ] PollingIndicator — primitive built (refresh icon + "Updated Xs ago", default 5s tick via `intervalMs`), but not yet **visible on polling-enabled pages** per spec — those pages (0068 home, list pages) don't exist yet. Also pending: `isFetching` prop + spin animation + optional `onRefresh` click handler — will land when a real polling page wires up TanStack Query's `dataUpdatedAt` and `isFetching`.
- [x] Text labels primary — every badge above passes the label string; color is decoration.
- [x] All components exported — `NetworkBadge`, `RelativeTimestamp`, `PollingIndicator`, plus helpers `formatRelative` + `useNow`, all re-exported from `libs/ui` barrel.

## Notes

- Badge color palette comes from MUI theme configuration in task 0058.
- Status badges are used heavily in transaction tables (tasks 0068, 0069, 0070).
- Type badges are critical for the tokens list/detail (task 0074) and contract detail (task 0075).
- Relative timestamps appear in every table that shows time data.
