---
id: '0474'
title: 'Inferara logo in the Code-tab attribution line'
type: FEATURE
status: active
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
---

# Inferara logo in the Code-tab attribution line

## Summary

Add a tiny Inferara logo to the attribution line under the Code-tab viewer
("WASM decompilation provided by Inferara soroban-ret · inferara.com").
Requested by Inferara after the 0465 deploy; copy unchanged.

## Context

The mark: Inferara's own favicon / app icon (512px, alpha) is a plain red
dot, `#DD2E44` (sampled from their `android-chrome-512x512.png`) — the same
asset Dominik sent. Their full GitHub-avatar mark (navy maze + dot) is a
different, larger lockup; the dot alone is the sanctioned tiny form.
Their wordmark SVG (`inference-logo-outlined.svg`) is a different brand
("Inference") — not usable here.

## Implementation

Inline SVG circle (`<circle r=5 fill="#DD2E44">`, ~10px, `aria-hidden`)
before the attribution text in `ContractCode.tsx`. No raster asset: the
PNG has a baked white background (glows in dark mode), while an inline
vector is crisp at any scale and adds nothing to the bundle.

## Acceptance Criteria

- [ ] Red-dot logo renders before the attribution line, vertically centered,
      correct in both light and dark mode.
- [ ] Copy and links unchanged.
- [ ] **Docs updated** — N/A (no architecture-shape change).
- [ ] **API types regenerated** — N/A (frontend only).
