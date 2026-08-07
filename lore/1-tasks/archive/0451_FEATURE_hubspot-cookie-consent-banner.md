---
id: '0451'
title: 'FEATURE: HubSpot cookie consent banner + footer Cookie Settings control'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0437', '0454']
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
  - date: 2026-07-29
    status: completed
    who: karolkow
    note: >
      Shipped and verified live. 3 files (web/index.html, Footer.tsx, and a new
      Footer.test.tsx), 2 commits, 76 + 119 tests green. Deployed as part of the
      Compute stack release (API + indexer + enrichment) followed by the SPA;
      post-deploy checks confirmed the loader in the served HTML, the Turnstile
      key present in the shipped bundle, the banner rendering for a fresh
      visitor (opt-in with categories) and the footer control in place.
      Boundary recorded explicitly: the banner governs the vendor's own cookies
      only — gating the analytics container on consent was costed and declined
      as out of scope. Deploy verification also surfaced a 19-minute ingestion
      outage, unrelated to this change and ended by the deploy itself; fully
      investigated and spawned as [[0454]].
---

# FEATURE: HubSpot cookie consent banner + footer Cookie Settings control

## Summary

Embed the HubSpot tracking code (portal `8102665`) so the opt-in cookie
consent banner configured in the HubSpot portal renders on the site, and add
a `Cookie Settings` entry to the footer that re-opens the banner.

## Status: Completed

**Current state:** shipped and verified live on production.

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

- [x] Tracking code loads on production — the served `index.html` carries the
      loader, and the live page pulls `js.hs-scripts.com`, `js.hs-banner.com`
      and `js.hs-analytics.net`.
- [x] HubSpot domain verification passes — implied and confirmed by the banner
      actually being served; HubSpot only serves it once the tracking code is
      verified at the URL.
- [x] Consent banner renders for a fresh visitor — verified live in a clean
      session: `#hs-eu-cookie-confirmation` present, with `Accept All`,
      `Decline All` and `Cookies settings` (opt-in with categories).
- [x] Footer shows `Cookie Settings` — present in the served bundle and in the
      live DOM as the last `Resources` entry. The click path is unit-tested;
      re-opening after a stored preference was not exercised end-to-end.
- [x] FE typecheck / lint / tests green — 76 in `libs/ui` (+1 new), 119 in `web`.
- [x] **Docs updated** — N/A, no change to the shape of the system.
- [x] **API types regenerated** — N/A, nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` is touched.

## Implementation Notes

Three files, one commit each for code and test:

- `web/index.html` — loader before `</body>`, byte-identical to the supplied
  snippet apart from prettier line wrapping. The GTM snippet stays in `<head>`.
- `libs/ui/src/layout/Footer.tsx` — `Cookie Settings` appended to `RESOURCES`,
  plus a `Window._hsp` declaration.
- `libs/ui/src/layout/Footer.test.tsx` — new; asserts the control is still a
  link (so it keeps focus and hover) and that clicking queues `showBanner`.

Deployed with the Compute stack (API + indexer + enrichment) and then the SPA.
Post-deploy verification covered: the loader in the served HTML, the Turnstile
site key present in the shipped bundle (the un-armed-bundle trap from [[0437]]),
the banner rendering live, and the absence of the withdrawn net-settled column.

## Design Decisions

### From Plan

1. **Loader in `index.html`, before `</body>`** — per the vendor install guide;
   it is a page-level third-party tag, not a React concern. Same placement
   rationale as [[0437]].

2. **Footer entry instead of the supplied `<button>`** — the portal markup
   carries hardcoded colours and an inline handler, neither of which matches the
   design system. The vendor documents a link as a supported control variant, so
   this is within their contract, not around it.

### Emerged

3. **`href="#"` + `preventDefault()` rather than a bare `onClick` span** —
   `FooterLink` renders a `<span>` when there is no `href`, which would lose
   keyboard focus and hover styling. The anchor form is the idiom the footer
   already uses (`handleFooterNavClick` in `AppShell`).

4. **A unit test, verified by mutation** — the control is the only way to
   withdraw consent once given, and a refactor of `FooterLink` could silently
   break it. The test was confirmed to fail when the `href` is removed.

5. **`declare global { Window._hsp }` inside `Footer.tsx`** — one optional
   field, created on demand, so a click is a no-op when the script is blocked
   (ad blockers block this vendor by default; the banner is absent then too, so
   the behaviour stays consistent).

6. **Banner appearance left portal-side** — the banner is vendor-rendered and
   themable only in their editor. It has a single static look while the site has
   a light/dark toggle, so it cannot follow the theme. Overriding their CSS from
   our side was rejected: the element lives in the normal DOM and could be
   restyled, but it would break silently on any vendor update.

7. **GTM stays ungated on consent** — the banner governs the vendor's own
   cookies. Wiring the analytics container to the consent state
   (`addPrivacyConsentListener`) was raised, costed and declined by the owner as
   out of scope. Recorded here so the boundary is explicit rather than assumed:
   a visitor who declines is not opted out of the analytics container.

## Issues Encountered

- **Pre-commit typecheck false-failed in the worktree.** `node_modules` is a
  symlink to the main checkout, so `@rumblefish/*` workspace packages resolved to
  whatever branch that checkout sat on, and `web` typechecked against a different
  library. Fixed by shadowing the two packages under `web/node_modules/`
  (gitignored) — not by bypassing the hook. Side effect worth knowing: before the
  fix, `web`'s typecheck was not exercising this task's own change at all.

- **A stale `index.html` showed a blank page right after the deploy.** Browser
  cache pointing at a bundle the sync had just deleted. Bounded to minutes by the
  short-TTL cache policy from [[0106]]; no action needed.

- **A 19-minute ingestion outage overlapped the Compute deploy** and was found
  while verifying it. Not caused by this task — the deploy in fact ended it.
  Investigated in full and spawned as [[0454]].

## Future Work

- Banner colours to be matched to the dark theme in the vendor portal — external,
  no repository change, no task.
- Future CSP work must allow `js.hs-scripts.com`, `js.hs-banner.com`,
  `js.usemessages.com`, `js.hscollectedforms.net`.

## Notes

- `showBanner` applies only to opt-in and cookies-by-category policies. The
  banner is configured opt-in, so the footer control is meaningful.
- Deploy: production is manual. Build with `--skip-nx-cache` and verify
  `/auth/session` returns 200 — the un-armed-bundle trap from [[0437]].
- Future CSP work must allow `js.hs-scripts.com`, `js.hs-banner.com`,
  `js.usemessages.com`, `js.hscollectedforms.net`.
