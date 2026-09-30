---
id: '0474'
title: 'Inferara logo in the Code-tab attribution line'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0465']
tags: ['effort-small', 'frontend', 'cooperation-inferara']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/374']
history:
  - date: 2026-08-12
    status: active
    who: stkrolikiewicz
    note: >
      Post-deploy follow-up to 0465. Inferara (Dominik) confirmed the live
      integration and the report-issue flow work as expected, and asked for
      one branding addition: a tiny Inferara logo in the attribution line.
      The copy stays as-is (their words: "current copy is perfect").
  - date: 2026-08-13
    status: active
    who: stkrolikiewicz
    note: >
      Merged in #394 (master, 08-12), riding release #402 — but NOT yet
      deployed: the prod SPA still serves the pre-#394 bundle (the
      ContractDetailPage chunk carries the attribution line but no
      `#810F0C` mark fill; checked 08-13). Stays active per the
      archive-at-deploy convention. To close: deploy the SPA, eyeball
      the mark in both themes on prod, optionally let Inferara confirm
      on issue #374, then archive.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      Live on production (#394). Closing check below; not eyeballed in both themes.
---

# Inferara logo in the Code-tab attribution line

## Summary

Add a tiny Inferara logo to the attribution line under the Code-tab viewer
("WASM decompilation provided by Inferara soroban-ret · inferara.com").
Requested by Inferara after the 0465 deploy; copy unchanged.

## Context

The mark is the full Inferara glyph (maze + red dot), vectored in their own
`inferara.com/assets/images/logo.svg`. The asset Inferara sent was the WHITE
variant of that glyph — on a white background only the red dot shows, which
sent the first implementation down a wrong path (a lone red dot; their
favicon happens to be just the dot too). Their wordmark SVG
(`inference-logo-outlined.svg`) is a different brand ("Inference") — not
usable here.

## Implementation

`InferaraMark` component (`web/src/pages/contracts/InferaraMark.tsx`): the
14 paths from their logo.svg with the maze switched to `currentColor` (the
same trick their own wordmark SVG uses) so it follows the attribution text
colour in both themes; the dot keeps the brand red `#810F0C`. Rendered
15×15 px, `vertical-align: text-bottom`, at the END of the attribution
line after the `inferara.com` link (a leading mark read as a list bullet).
No raster asset: their PNG has a baked white background (glows in dark
mode), while the inline vector is crisp at any scale.

## Acceptance Criteria

- [x] Full Inferara mark renders at the end of the attribution line,
      bottom-aligned, correct in both light and dark mode (deployed bundle; see below)
- [x] Copy and links unchanged.
- [x] **Docs updated** — N/A (no architecture-shape change).
- [x] **API types regenerated** — N/A (frontend only).

## Closing check (2026-09-30, read-only)

- The deployed `ContractDetailPage` chunk renders "WASM decompilation provided
  by Inferara · soroban-ret · inferara.com" followed by the `InferaraMark`
  component (`web/src/pages/contracts/ContractCode.tsx:583`), whose maze uses
  `currentColor`, so it follows the theme.
- Not eyeballed in both themes on production: the Code tab needs the API,
  which sits behind the browser challenge.
