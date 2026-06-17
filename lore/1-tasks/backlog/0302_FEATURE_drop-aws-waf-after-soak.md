---
id: '0302'
title: 'FEATURE: Drop AWS WAF after Cloudflare soak'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags:
  [phase-future, effort-small, priority-medium, security, waf, cloudflare, cost]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Renumbered 0283 → 0302 to resolve a shared-ID collision — '0283' was also
      claimed (same day) by the active contract-type-rebuild task (older,
      milestone-1, with child tasks 0294-0297, and the namesake of branch
      fix/0283). Content unchanged; only id + filename. Heads-up @fmazur.
---

# Drop AWS WAF after Cloudflare soak

## Summary

Cloudflare edge is live + verified (0277). After an agreed soak window, drop BOTH AWS WAF WebACLs
to reach the flat-cost goal (AWS WAF bills $0.60/M per request).

## Context

0277 armed the Cloudflare edge but kept `enableWaf:true` to avoid a same-deploy WAF teardown.
Edge protection now lives in Cloudflare.

## Implementation

- Soak: monitor Cloudflare challenge/block + API GW 5xx for the agreed window.
- `enableWaf:false` → drops REGIONAL (API) + CLOUDFRONT WebACLs (`waf-web-acl.ts`).
- WAF log groups are `RemovalPolicy.DESTROY` → re-enable is a fresh deploy, not a toggle.

## Acceptance Criteria

- [ ] Soak passed (no regressions)
- [ ] `enableWaf:false` deployed; both WebACLs gone; cost drop confirmed
