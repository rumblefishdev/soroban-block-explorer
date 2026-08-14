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
  - date: 2026-08-13
    status: active
    who: stkrolikiewicz
    note: >
      Status sync: PR #386 (steps 1, 2, part of 4) merged 08-10, rode
      release #389 (08-11) and is deployed — today's prod bundle is
      post-#389 (established in passing during the 0474 deploy check),
      so the per-mode logo lockups and the graphite light-mode accent
      are live. Untouched and still open: steps 3 and 5 plus the rest
      of the F5 triage (NftEventBadge colorsDark binding, ~12
      common.black/white call sites, the assetColor.ts import check).
---

# BUG: light mode is unfinished — brand logos and the accent yellow are drawn for dark only

## Summary

Light mode is reachable in production — there is a toggle in the top nav and
`readInitialMode` restores whatever the user last chose — but two brand-level
things were only ever designed against the dark background: the
SorobanScan and RumbleFish logo artwork, and the accent yellow `#fdda24`. In
light mode the nav logo's "Scan" wordmark is white-on-white, the footer
RumbleFish lockup is a ghost, and the hero headline sits at **1.26:1** contrast.
Two component families additionally bind to a hardcoded palette object instead
of the theme, so they are wrong in whichever mode they were not written for.

## Status: Active

**Current state:** Steps 1, 2 and part of 4 merged in PR #386 (2026-08-10),
released in #389 and deployed — the per-mode logo lockups and the graphite
light-mode accent are live on prod. Design settled the accent question the
other way from the guess in F2: light mode goes **graphite**
(`scales.gray[700]`), not a darker amber, and the brand yellow stays reserved
for filled surfaces, glows and markers. Steps 3 and 5 and the rest of the F5
triage are untouched.

Findings below are reproduced on a local dev server in light mode plus a code
sweep; contrast ratios are computed, not eyeballed.

## Context

`0058` delivered light + dark theming; `0257` (frontend audit) flagged the
palette-coupling class of bug as F-RR-16 but left it 🟡 and never spawned a
task. Nobody has since done a deliberate light-mode pass, so the gaps
accumulated in the most visible surfaces — nav, footer, hero.

## Findings

**Chip palette — `Fungible` fails AA** (found in the 0472 review, 2026-08-13,
measured in the live DOM): emerald chip renders `#009966` on `#D0FAE5` =
**3.22:1** at 14px/500, against the 4.5:1 minimum. Task 0472 promoted that
chip to the contract page header, where it is the only link, so the failure
is now load-bearing. The sibling colours pass — brown 7.90:1, accent ~13:1,
neutral 7.06:1 — so this is one token, not the whole palette. Same override
block as the rest of this task (`libs/ui/src/theme/overrides.ts`, the
`props: { color: 'emerald' }` variant).

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

- **Colour-mode resolution, for the record.** Dark is the product default
  (`ThemeProvider.tsx`, `defaultMode = 'dark'`) and OS `prefers-color-scheme`
  is **deliberately ignored** — only an explicit toggle, stored under
  `soroban-explorer.color-mode`, overrides it. An earlier draft of this task
  said `readInitialMode` follows the OS preference; that was carried over from
  a stale 0257 note and is wrong. PR #386 additionally moved the persistence
  out of a mount effect and onto the toggle, so a first-time visitor no longer
  gets today's default frozen into their storage.
- Reported from two user screenshots (nav + footer in light mode), then
  reproduced locally and extended by a code sweep.
- Contrast numbers computed with the WCAG relative-luminance formula against
  the actual token values in `colors.ts`, not sampled from a screenshot.
- Steps 1 and 2 both need a design decision before code — worth pulling design
  in once, for both, rather than twice.
