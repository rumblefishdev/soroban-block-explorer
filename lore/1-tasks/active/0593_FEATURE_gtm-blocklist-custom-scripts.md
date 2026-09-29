---
id: '0593'
title: 'FEATURE: block Custom HTML and Custom JS in GTM, so a published tag cannot read the portal API key'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0437', '0451', '0589']
tags: [frontend, security, analytics, priority-medium, effort-small]
links:
  - web/index.html
history:
  - date: '2026-09-29'
    status: backlog
    who: stkrolikiewicz
    note: >
      Opened after reviewing what the explorer's GTM can reach on the Prices API
      portal (stellar-prices-api task 0316, PR #362). The portal PR carries the
      same blocklist.
  - date: '2026-09-29'
    status: active
    who: stkrolikiewicz
    note: >
      Started. Same change as the portal's in stellar-prices-api #362: one
      dataLayer push before GTM, a test, and a browser check.
---

# FEATURE: block Custom HTML and Custom JS in GTM, so a published tag cannot read the portal API key

## Summary

Anyone who can publish in GTM container `GTM-TBF2GP5S` can run any
JavaScript on every page of `sorobanscan.rumblefish.dev`. No review, CI or
deploy stands in the way. That JavaScript can read the Prices API key of a
visitor who is signed in to the portal. A `gtm.blocklist` pushed before GTM
loads makes GTM refuse Custom HTML tags and Custom JavaScript variables,
whatever the container holds.

## Context

**The explorer and the portal are one trust zone.** Verified in
stellar-prices-api on 2026-09-29:

- The portal backend (`prices-api.sorobanscan.rumblefish.dev`) accepts
  credentialed calls from the origin `https://sorobanscan.rumblefish.dev`
  (`infra/envs/production.json`, `portalWebOrigin`). CORS checks the origin,
  not the path, so an explorer page counts the same as `/api/`.
- The `portal_session` cookie is `HttpOnly` and `SameSite=Lax`. Script cannot
  read it, but the two hosts are same-site, so the browser sends it on a
  `fetch` with `credentials: 'include'`.
- `GET /api/key` answers with the full key in `value`.

So a script on any explorer page can fetch a signed-in visitor's key. It can
also frame `/api/` (same origin) and read the rendered page.

**What the container holds today.** Published version 2 has exactly one
tag, the Google tag for GA4 `G-DFMXSJQ9DR` (`__googtag`). It has no Custom
HTML and no custom templates. The blocklist therefore breaks nothing today,
and it keeps the container from gaining arbitrary JavaScript later.

**Scope.** The blocklist is read by `gtm.js` on the page that sets it, so
every page that loads GTM needs it. The explorer has one `index.html`. The
portal's copy is in stellar-prices-api #362. HubSpot's scripts are outside
GTM, and this does not cover them.

## Implementation Plan

### Step 1: `web/index.html`

In the Consent Mode script that runs before the GTM snippet, push
`{'gtm.blocklist': ['customScripts']}` onto `dataLayer`. `customScripts`
covers Custom HTML tags and Custom JavaScript variables.

### Step 2: test

Extend `web/src/__tests__/consent-mode.test.ts`, or add a sibling test, to
assert that the blocklist is in `dataLayer` before the GTM loader runs.

### Step 3: browser check

On a `vite preview` build, check that `dataLayer` carries the blocklist and
that GA4 still sends a `page_view` after "Accept All".

## Acceptance Criteria

- [ ] `web/index.html` pushes `gtm.blocklist` with `customScripts` before
      the GTM snippet
- [ ] A test fails if the blocklist is removed or moved after GTM
- [ ] GA4 still works after consent (the Google tag is not blocked)
- [ ] **Docs updated** — N/A: `docs/architecture/**` does not describe the
      third-party tags.
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` is touched.

## Notes

- Also decide whether to add `nonGoogleScripts`. It blocks tags that inject
  non-Google scripts, including custom templates that do so. The container
  has none today.
- Outside the repo: keep the number of users with Publish permission on the
  container small, and require 2-step verification for publishing (GTM
  container settings).
