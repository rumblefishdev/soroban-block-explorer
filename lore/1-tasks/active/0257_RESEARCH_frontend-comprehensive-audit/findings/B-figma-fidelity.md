# B — Figma Fidelity (Wave 6 / 2.1)

## Status: BLOCKED — no Figma URL surface provided for this session

Per Wave 6 dispatch instructions:
> "Phase 3 spawn candidate: dedicated Figma comparison task once URL available."

## What was checked

### Figma URLs ARE catalogued in archived FE tasks

`grep -rn "figma.com" lore/1-tasks/archive/` returns design URLs for:

| Task | Figma node-id refs |
|---|---|
| 0073 (account detail) | nodes 154-12747, 157-22153, 360-1812 (Design System) |
| 0074 (assets list/detail) | nodes 188-25012, 206-15608, 360-1812 |
| 0077 (liquidity-pools) | nodes 266-35969, 267-59942, 325-7098, 325-24354 |
| 0246 (LP backend) | nodes 266-35969, 267-59942, 325-7098, 325-24354 |

File key (extracted from URLs): `n1p6WCMVd4iinbuvOA2WjP` (Designs) and `siumLgKOc9LLepEfbimyp3` (Design System — Stellar Block Explorer).

### Pixel-perfect comparison NOT attempted in this session

Reasons:
1. **No node-id provided in Wave 6 dispatch.** Per `figma-use` skill conventions, every `mcp__figma__get_screenshot` call requires `(fileKey, nodeId)`. Picking node-ids from archived tasks without confirming they match the *current* live FE state means audit findings could be against an outdated design.
2. **Figma mainline may have moved since 0073/0074/0077 archive dates.** Audit value of "1:1 fidelity" depends on a known-current design baseline; comparing live code to potentially-superseded design produces noise.
3. **User instruction in this session:** "If user hasn't shared Figma file URL, document as 'BLOCKED — need Figma URL' and proceed to other sub-phases."

## Recommendation for Phase 3

Spawn dedicated task `XXXX_RESEARCH_figma-fidelity-audit-1to1` (Phase 3) with prerequisite:
- User confirms current/canonical file + node-id mapping per route
  - or
- Pull the most recent Figma file via `mcp__figma__get_metadata` to enumerate node IDs by name and let agent select per-route matches

Effort estimate: 6-8h per task README; depends on number of design variants per route (Wave 6 plan assumed pixel-perfect on every view = no time-box).

## What Wave 6 *can* report from cross-sub-phase observation

Even without a Figma compare, several visual divergences from "likely Figma intent" surfaced during the Playwright pass:

| Observation | Likely Figma intent | Cross-cite |
|---|---|---|
| Two filter slots above `/assets` table render with zero-width placeholders | Figma mockup almost certainly shows labeled filters with placeholder copy | F-W6-E7-1 |
| Four filter slots above `/nfts` table all unlabeled | Same — likely labeled in Figma | F-W6-E10-1 |
| "?" question-mark glyph for assets with no icon | Figma likely specifies a neutral token-icon fallback | F-W6-E7-2 |
| `33.3333333333333333%` for fractional shares % | Figma would not show 16 significant digits | F-W6-E13-1 |
| Mobile layout missing hamburger menu | Figma mobile spec presumably has one | F-W6-E0-3 |
| NotFound page lacks h1 (4 of 5 detail routes) | Figma NotFound mockup likely has heading | F-W6-NOTFOUND-1 |
| Footer Resources + Terms/Privacy/Cookies render as plain spans | Figma footer surely uses link styling | CA-1 + CA-2 |
| LIVE badge always-on regardless of freshness | Figma likely shows badge only when fresh | DM-1 |

## Output

This file = BLOCKED status + the cross-reference table above. No new severity-tagged findings in this file — all visual divergences caught are already filed elsewhere.

**Recommended for Phase 3:** spawn `XXXX_RESEARCH_figma-fidelity-audit-1to1` and bundle it with the visual-polish batch (`XXXX_REFACTOR_format-truncate-unification`, etc.) per Gate B Phase-3 spawn plan.
