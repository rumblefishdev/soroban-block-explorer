---
id: '0437'
title: 'FEATURE: Google Tag Manager analytics + footer Privacy Policy link'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0407']
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

- [ ] GTM loads on the live site (Network shows `gtm.js?id=GTM-TBF2GP5S`)
- [ ] Footer shows a `Privacy Policy` link (external, new tab)
- [ ] Hits register in GA
- [ ] FE typecheck / lint green

## Design Decisions

### From Plan

1. **GTM in `index.html`, not a React component**: it is a page-level
   third-party tag; the standard placement is the HTML shell. No new dep.

### Emerged

_(fill during implementation)_

## Future Work

- **SPA route tracking**: GTM/GA only fires a pageview on the initial HTML
  load. Client-side route changes (this is an SPA) won't register unless a
  History-Change trigger is configured, or route changes are pushed to
  `dataLayer`. Confirm whether per-route pageviews are needed; if so, spawn
  a follow-up to emit `dataLayer` events on navigation.
