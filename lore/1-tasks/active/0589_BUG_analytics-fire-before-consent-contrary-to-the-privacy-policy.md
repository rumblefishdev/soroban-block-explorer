---
id: '0589'
title: 'BUG: GA fires and sets _ga before consent, while the privacy policy says analytics wait for consent'
type: BUG
status: active
related_adr: []
related_tasks: ['0437', '0451', '0577']
tags: [frontend, privacy, analytics, priority-high, effort-small]
links:
  - web/index.html
  - web/src/pages/PrivacyPolicyPage.tsx
history:
  - date: '2026-09-28'
    status: backlog
    who: stkrolikiewicz
    note: >
      Measured while adding the same GTM container to the Prices API portal
      (stellar-prices-api task 0316). The portal copies whatever this repo
      does, so the fix is made here first and then repeated there.
  - date: '2026-09-28'
    status: active
    who: stkrolikiewicz
    note: >
      Started. Option 1 (Consent Mode default + HubSpot listener in
      web/index.html) is the fix in this repo.
---

# BUG: GA fires and sets \_ga before consent, while the privacy policy says analytics wait for consent

## Summary

The HubSpot banner governs HubSpot's own cookies only. GTM `GTM-TBF2GP5S`
loads GA4 whatever the visitor chooses. The privacy policy published in
[[0577]] states the opposite, so the live site and its policy disagree.

## Context

**Measured on 2026-09-28** in a browser in Poland. Cookies were cleared,
`https://sorobanscan.rumblefish.dev/` was reloaded, and the banner was left
visible and untouched:

- GA4 sent a `page_view` to `region1.google-analytics.com/g/collect` with
  `tid=G-DFMXSJQ9DR`, and set `_ga` and `_ga_DFMXSJQ9DR`.
- HubSpot sent `track.hubspot.com/__ptq.gif`.
- The `dataLayer` holds no `consent default` entry, and `gcd=13l3l3l2l1l1`
  means Google's Consent Mode receives no signal at all.

**Why it is so.** [[0451]] (decision 7) left GTM ungated on consent. At
that point the footer linked the corporate policy, so the site made no
promise of its own about analytics.

**Why it is now a defect.** [[0577]] published the site's own policy.
`PrivacyPolicyPage.tsx` §3 says that where the law requires it, "analytics
technologies are activated only after you provide consent". The site does not
behave that way, and a visitor who declines is still tracked by GA.

## Options

1. **In this repo.** In `web/index.html`, before the GTM snippet, add
   `gtag('consent', 'default', …denied)`. Add a HubSpot
   `addPrivacyConsentListener` that sends `gtag('consent', 'update', …)` from
   `consent.categories`. About 15 lines, reviewable, and it can be checked
   in a browser.
2. **In the vendors' settings.** Turn on Google Consent Mode in HubSpot's
   banner settings, and make the GA tag in the GTM container require
   `analytics_storage`. This needs no code, but it happens outside review
   and needs someone with access to both accounts.
3. **Change the policy instead.** Describe the actual behaviour. This is
   weak under GDPR and ePrivacy, because analytics cookies need prior
   consent in the EU.

Options 1 and 2 together also stop the cookieless pings that Consent Mode's
"advanced" setup still sends.

## Acceptance Criteria

- [x] With cookies cleared and the banner untouched, there is no `_ga` cookie,
      and either no `g/collect` request or only one with a denied `gcs`.
      Verified: no cookies at all, not even HubSpot's. GA sends cookieless
      pings, `page_view` and `scroll`, both `gcs=G100`. Only option 2 stops
      them.
- [x] After "Accept All", `analytics_storage` is granted and a `page_view` is
      sent. After "Decline All", it stays denied.
- [x] A consent choice stored on an earlier visit is applied on load, without
      a flash of granted state. Stored accept: `default denied` → `update
    granted`, one `page_view` with `gcs=G111` (`wait_for_update` held it).
      Stored decline: `update denied`, `page_view` with `G100`, no `_ga`.
- [ ] The Prices API portal (`/api/*`, stellar-prices-api 0316) repeats the
      same change. It lives in the other repo; tracked there.
- [x] **Docs updated.** N/A: `docs/architecture/**` does not describe the
      third-party tags.
- [x] **API types regenerated.** N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` is touched.

## Implementation Notes

- `web/index.html`:
  - Before GTM: `gtag('consent', 'default')` with `analytics_storage`,
    `ad_storage`, `ad_user_data` and `ad_personalization` all `denied`, and
    `wait_for_update: 500`.
  - After the HubSpot loader: an `addPrivacyConsentListener` that maps
    `categories.analytics` to `analytics_storage`, and
    `categories.advertisement` to the three `ad_*` flags.
- `web/src/__tests__/consent-mode.test.ts`: runs the real inline scripts from
  `index.html` in jsdom. It checks that the default is denied and sits before
  GTM, and that the listener maps categories to flags. A mutation of the
  default to `granted` makes it fail.
- Verified in a browser on a production build (`vite preview`) against the
  live GTM container and HubSpot portal. Checked: a fresh visitor, "Accept
  All", "Decline All", reload with a stored choice, and the footer's Cookie
  Settings with analytics only (`analytics_storage` granted, `ad_*` denied,
  `_ga` set, no `_gcl_au`).

## Design Decisions

### From Plan

1. **Option 1: Consent Mode signals in code.** The fix is reviewable and
   versioned, and the browser can check it. Option 2 stays optional. It is
   the only way to stop the cookieless `G100` pings, and it needs access to
   the GTM account.

### Emerged

2. **Deny everywhere, not only in the EEA.** A region-scoped default saves
   nothing worth the extra condition, and the policy does not promise more
   anywhere else.
3. **The `ad_*` flags follow the advertisement category.** GTM already sets
   `_gcl_au` (Google Ads) on "Accept All", so ads consent is real, not only
   analytics.
4. **`wait_for_update: 500`.** A returning visitor's stored choice arrives
   through HubSpot, which loads after GTM. Measured: 500 ms was enough for
   the stored accept to reach the first `page_view`.
5. **GTM still loads before consent.** Loading GTM only after consent would
   also stop the pings, but it moves the loader away from the vendor's
   snippet and puts every page view behind HubSpot's load. Consent Mode is the
   mechanism the vendors support.

## Issues Encountered

- **Resource Timing does not show GA hits under `vite serve`.** The dev
  server loads over 250 modules and fills the default buffer, so later
  entries (`g/collect`) are dropped. Checked on a `vite preview` build
  instead.
- **Cookies are shared across `localhost` ports.** A test on `:4201` leaked
  its consent into `:4202`. Cookies were cleared from a script-free page
  (`/favicon-32.png`) before the fresh-visitor run.
