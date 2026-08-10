---
id: '0467'
title: 'BUG: light mode is unfinished — brand logos and the accent yellow are drawn for dark only'
type: BUG
status: active
related_adr: []
related_tasks: ['0058', '0257']
tags:
  [priority-medium, effort-medium, layer-frontend-web, layer-frontend-shared]
links: []
history:
  - date: 2026-08-10
    status: active
    who: stkrolikiewicz
    note: 'Task created after a light-mode visual + code audit'
---

# BUG: light mode is unfinished — brand logos and the accent yellow are drawn for dark only

## Summary

Light mode is reachable in production (`readInitialMode` follows the stored
preference / `prefers-color-scheme`, and there is a toggle in the top nav), but
two brand-level things were only ever designed against the dark background: the
SorobanScan and RumbleFish logo artwork, and the accent yellow `#fdda24`. In
light mode the nav logo's "Scan" wordmark is white-on-white, the footer
RumbleFish lockup is a ghost, and the hero headline sits at **1.26:1** contrast.
Two component families additionally bind to a hardcoded palette object instead
of the theme, so they are wrong in whichever mode they were not written for.

## Status: Active

**Current state:** Audited, not started. Findings below are reproduced on a
local dev server in light mode plus a code sweep; contrast ratios are computed,
not eyeballed.

## Context

`0058` delivered light + dark theming; `0257` (frontend audit) flagged the
palette-coupling class of bug as F-RR-16 but left it 🟡 and never spawned a
task. Nobody has since done a deliberate light-mode pass, so the gaps
accumulated in the most visible surfaces — nav, footer, hero.

## Findings

### F1 — Logo assets are theme-blind (the two screenshots)

[`AppShell.tsx:71`](../../../web/src/router/AppShell.tsx) picks the image by
**brand variant**, never by color mode:

```tsx
src={isSoroban ? '/soroban-logo.webp' : '/rumblefish-logo.webp'}
```

Both files (`web/public/*.webp`) are drawn for a dark background — light glyphs
on transparency. Consequences in light mode:

- **Top nav** — the "Scan" wordmark and the circular mark are white on white
  (effectively invisible); only the yellow "Soroban" survives, and badly (F2).
- **Footer** — the whole RumbleFish lockup renders as a faint grey ghost.

Fix needs a design input: either per-mode artwork (`-light` / `-dark`) selected
on `theme.palette.mode` inside `HomeLogo`, or — better and cheaper to maintain —
SVG wordmarks whose glyphs use `currentColor` so a single asset serves both
modes. `web/public/favicon.svg` already exists, so an SVG pipeline is not new
ground.

### F2 — `text.accent` is the same yellow in both modes

[`colors.ts:102`](../../../libs/ui/src/theme/colors.ts) (light) and
[`colors.ts:149`](../../../libs/ui/src/theme/colors.ts) (dark) are both
`#fdda24`. Measured contrast:

| accent `#fdda24` vs     | ratio      | WCAG AA                           |
| ----------------------- | ---------- | --------------------------------- |
| dark bg `#212121`       | 11.68:1    | pass                              |
| light bg `#f5f5f5`      | **1.26:1** | fail (needs 4.5 body / 3.0 large) |
| light surface `#ffffff` | **1.38:1** | fail                              |

Affected consumers: hero headline "Soroban" / "Stellar"
([`HomeHero.tsx:39,47`](../../../web/src/pages/home/HomeHero.tsx)), the active
nav underline, `stroke.action`, `overrides.ts:120,562`, and everything using
`color="accent"`.

The light palette **already ships a usable value**: `surface.primaryMainAlt:
'#a36905'` measures **4.21:1** on `#f5f5f5` — passes AA for large text, close
for body. So this is likely a one-value split, not a re-design — but confirm
with design that the darker amber is the intended light-mode accent rather than
an unrelated token.

Out of scope unless design says otherwise: `HomeHeroGlow` uses
`surface.primaryMain` at `opacity: 0.28` behind `blur(75px)`. It is decorative
(`aria-hidden`), reads as a warm wash in light mode, and is not a contrast
target.

### F3 — `NftEventBadge` hardcodes `colorsDark.*`

[`NftEventBadge.tsx:14-21`](../../../web/src/pages/nft-detail/NftEventBadge.tsx)
imports `colorsDark` and binds it unconditionally. This is 0257's F-RR-16. Its
own note says the "Mint" pill still reads acceptably in light, so this is
cosmetic — but it is the same defect class as F4 and should die in the same
sweep.

### F4 — `assetColor.ts` hardcodes `colorsLight.*`

[`assetColor.ts`](../../../web/src/pages/assets/assetColor.ts) does the inverse:
~30 bindings to `colorsLight.{blue,emerald,violet,green,red,yellow,secondary,gray}`,
so the asset-type badges are keyed to the light scale in **dark** mode too.
Worth checking whether that is actually wrong — the `scales` object is shared
between `colorsLight` and `colorsDark`, so these may resolve identically today
and the bug is only that the import advertises a mode it does not mean.

### F5 — `common.black` / `common.white` used as foreground/background

`theme.palette.common.*` does not flip with mode. ~12 call sites bind to it:
`SearchResultsTabs.tsx:72,113,122`, `ViewAllLink.tsx:48`,
`NftMediaPreview.tsx:71`, `Tabs.tsx:53`, `NavButton.tsx:104`,
`PageGridBackdrop.tsx:30,33`, `CopyButton.tsx:16`, `ExplorerTable.tsx:123`,
`overrides.ts:51,119,138,287,302,317,462,840`. Some are deliberate (black text
on a yellow chip is correct in both modes); some are latent. Needs a
site-by-site verdict, not a blanket replace.

### Verified clean

List/table pages render correctly in light mode (checked `/transactions`:
headers, hashes, status chips, op-type pills, pagination all fine). No `rgba()`
literals and no stray hex in components outside the one comment in
`HomeHeroGlow.tsx`. Whatever is broken is concentrated in brand + accent, not
spread through the component library.

## Implementation Plan

### Step 1: Logo artwork per mode

Get light-background lockups (or `currentColor` SVGs) for SorobanScan and
RumbleFish from design. Teach `HomeLogo` to select on `theme.palette.mode`, or
drop the selection entirely if SVG + `currentColor` lands.

### Step 2: Split `text.accent` per mode

Give `colorsLight.text.accent` a light-safe value (start from `#a36905`), keep
dark at `#fdda24`. Then walk every accent consumer — the token also feeds
`palette.primary.main` (`palette.ts:21`), so the blast radius is wider than
the hero.

### Step 3: De-hardcode F3 and F4

Move `NftEventBadge` and `assetColor` onto theme-aware lookups. Consider making
the two palette objects non-exported from `libs/ui/src/index.ts` (currently
exported at lines 7-8) so this cannot regress — a lint rule or just removing
the export.

### Step 4: Triage `common.black` / `common.white`

One pass over the ~12 sites in F5, each either confirmed-intentional (with a
comment) or moved to a mode-aware token.

### Step 5: Visual pass in light mode

Home, each list page, each detail page, NFT detail, search results, empty and
error states. Capture before/after.

## Acceptance Criteria

- [ ] Both logos are legible in light mode, in nav and in footer
- [ ] Hero headline accent reaches at least AA-large (3:1); document the ratio
- [ ] `text.accent` differs between `colorsLight` and `colorsDark`
- [ ] `NftEventBadge` and `assetColor` no longer import `colorsDark`/`colorsLight`
- [ ] Every `common.black`/`common.white` site is either commented as intentional or converted
- [ ] Light-mode visual pass done over the page list in Step 5
- [ ] **Docs updated** — `N/A — theming/asset change, does not alter the shape of the system described in `docs/architecture/\*\*``
- [ ] **API types regenerated** — `N/A — frontend only, no `crates/api/**`or`libs/api-types/**` change`

## Notes

- Reported from two user screenshots (nav + footer in light mode), then
  reproduced locally and extended by a code sweep.
- Contrast numbers computed with the WCAG relative-luminance formula against
  the actual token values in `colors.ts`, not sampled from a screenshot.
- Steps 1 and 2 both need a design decision before code — worth pulling design
  in once, for both, rather than twice.
