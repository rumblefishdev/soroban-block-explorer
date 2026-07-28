---
id: '0451'
title: 'FEATURE: HubSpot cookie consent banner + footer Cookie Settings control'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0437']
tags: [priority-medium, effort-small, layer-frontend, phase-launch]
milestone: 3
links:
  - web/index.html
  - libs/ui/src/layout/Footer.tsx
history:
  - date: 2026-07-28
    status: active
    who: karolkow
    note: >
      Embed the HubSpot tracking code so the cookie consent banner configured
      in the HubSpot portal renders on the site, and add a footer control that
      re-opens it. Banner copy, policy type and domain verification are owned
      outside this repo. Follows [[0437]].
---

# FEATURE: HubSpot cookie consent banner + footer Cookie Settings control

## Summary

Embed the HubSpot tracking code (portal `8102665`) so the opt-in cookie
consent banner configured in the HubSpot portal renders on the site, and add
a `Cookie Settings` entry to the footer that re-opens the banner.

## Status: Active

**Current state:** not started — two files, no dependencies left open.

## Context

The banner itself is not code: it is configured portal-side under
Settings → Privacy & Consent → Cookies, with an opt-in policy, and
`sorobanscan.rumblefish.dev` registered there as an external domain. HubSpot
verifies that its tracking code is present at that URL before it will serve
the banner, so nothing renders until this ships.

Scope boundary: the banner governs HubSpot's own cookies. The Google Tag
Manager container embedded in [[0437]] loads independently of it and is not
gated on consent — wiring GTM to the consent state (HubSpot's
`addPrivacyConsentListener`) is out of scope here.

## Implementation Plan

### Step 1: `web/index.html`

Loader `<script>` before the closing `</body>` tag, per HubSpot's install
guide. The GTM snippet stays where it is, in `<head>`.

### Step 2: `libs/ui/src/layout/Footer.tsx`

Add `Cookie Settings` to `RESOURCES` as an `onClick` entry with no `href`,
calling `_hsp.push(['showBanner'])`. Reuses the existing `FooterNavItem`
`onClick` path; the portal-supplied `<button>` markup is not used, because
its inline styles and inline handler do not match the design system.

## Acceptance Criteria

- [ ] Tracking code loads on production (`js.hs-scripts.com/8102665.js`).
- [ ] HubSpot domain verification passes for `sorobanscan.rumblefish.dev`.
- [ ] Consent banner renders for a fresh visitor.
- [ ] Footer shows `Cookie Settings`; clicking it re-opens the banner.
- [ ] FE typecheck / lint green.
- [ ] **Docs updated** — N/A, no change to the shape of the system.
- [ ] **API types regenerated** — N/A, nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` is touched.

## Notes

- `showBanner` applies only to opt-in and cookies-by-category policies. The
  banner is configured opt-in, so the footer control is meaningful.
- Deploy: production is manual. Build with `--skip-nx-cache` and verify
  `/auth/session` returns 200 — the un-armed-bundle trap from [[0437]].
- Future CSP work must allow `js.hs-scripts.com`, `js.hs-banner.com`,
  `js.usemessages.com`, `js.hscollectedforms.net`.
