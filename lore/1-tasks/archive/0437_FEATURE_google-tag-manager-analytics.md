---
id: '0437'
title: 'FEATURE: Google Tag Manager analytics + footer Privacy Policy link'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0407', '0390']
tags: [priority-medium, effort-small, layer-frontend, phase-launch]
milestone: 3
links:
  - web/index.html
  - libs/ui/src/layout/Footer.tsx
history:
  - date: 2026-07-23
    status: active
    who: karolkow
    note: >
      Embed the Google Tag Manager container so basic visitor analytics
      (traffic, referrers) can be collected via GA. GTM/GA container and
      tag configuration are owned outside this repo; the code change is
      only the page snippet plus a footer Privacy Policy link. Follows
      [[0407]] as the next soft-launch frontend item.
  - date: 2026-07-23
    status: completed
    who: karolkow
    note: >
      Shipped and verified live on production. 2 files (web/index.html GTM
      snippet + Footer.tsx Privacy Policy link), committed clean on develop.
      Live checks: gtm.js and gtag/js both load, dataLayer initialised,
      /auth/session 200, footer Privacy Policy + header Report a bug present,
      dark default. AC "hits register in GA" is external (owner confirms in
      Analytics). The isolated prod deploy surfaced an un-armed-bundle 401
      trap → deploy hardening captured in [[0390]] scope.
---

# FEATURE: Google Tag Manager analytics + footer Privacy Policy link

## Summary

Add the Google Tag Manager container (`GTM-TBF2GP5S`) to the SPA so basic
visitor analytics (traffic, referrers) can be collected via GA, and add a
`Privacy Policy` link to the footer.

## Context

The GTM/GA container and its tags are configured outside this repository;
the frontend change is limited to embedding the standard snippet and
surfacing a Privacy Policy link. Follows [[0407]] (footer/header work).

## Implementation

1. **`web/index.html`** — the standard GTM snippet:
   - `<head>`: loader `<script>` placed as high as possible (right after
     `<meta charset>`), per GTM guidance.
   - `<body>`: `<noscript>` iframe fallback right after `<body>` open.
2. **`libs/ui/src/layout/Footer.tsx`** — add `Privacy Policy` →
   `https://www.rumblefish.dev/privacy-policy/` to `RESOURCES`.

## Acceptance Criteria

- [x] GTM loads on the live site — verified: `gtm.js` and `gtag/js` both
      load, `dataLayer` initialised.
- [x] Footer shows a `Privacy Policy` link (external, new tab).
- [ ] Hits register in GA — **external**: the GA/GTM owner confirms in
      Analytics; nothing left to do in-repo.
- [x] FE typecheck / lint green (pre-commit passed on the commit).

## Implementation Notes

- Shipped as one clean commit on `develop` (2 files: `web/index.html`,
  `Footer.tsx`). Live on production and verified end-to-end.
- The GTM snippet is functionally identical to the supplied one — only
  reformatted by prettier (repo code style); behaviour unchanged.

## Design Decisions

### From Plan

1. **GTM in `index.html`, not a React component**: it is a page-level
   third-party tag; the standard placement is the HTML shell. No new dep.

### Emerged

2. **Snippet pretty-printed, not minified**: pasted the loader expanded to
   match repo formatting (prettier). Functionally identical to the supplied
   minified one — same code, same container id.

## Issues Encountered

- **Un-armed-bundle 401 (prod)**: an isolated prod deploy shipped a bundle
  built without `VITE_TURNSTILE_SITE_KEY` (Nx cache did not hash the env
  var), so the SPA attached no session token and every API call 401'd. Not
  caused by this task's code — a deploy-tooling gap. Rolled back, then
  redeployed an armed build. Hardening captured in [[0390]] scope.

## Future Work

- **SPA route tracking** (not scheduled): GTM/GA fires a pageview only on the
  initial HTML load; SPA client-side route changes need a GTM History-Change
  trigger (no code) or a `dataLayer` push on navigation (code). Only pursue if
  the analytics owner wants per-route pageviews, not just visitor/traffic
  counts — the current setup already covers the latter.
