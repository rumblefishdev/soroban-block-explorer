---
id: '0589'
title: 'BUG: GA fires and sets _ga before consent, while the privacy policy says analytics wait for consent'
type: BUG
status: backlog
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

**Why it is so.** [[0451]] decision 7 recorded that gating GTM on consent
was "raised, costed and declined by the owner as out of scope". At that
point the footer linked the corporate policy.

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

- [ ] With cookies cleared and the banner untouched, there is no `_ga` cookie,
      and either no `g/collect` request or only one with a denied `gcs`
- [ ] After "Accept All", `analytics_storage` is granted and a `page_view` is
      sent. After "Decline All", it stays denied.
- [ ] A consent choice stored on an earlier visit is applied on load, without
      a flash of granted state
- [ ] The Prices API portal (`/api/*`, stellar-prices-api 0316) repeats the
      same change
