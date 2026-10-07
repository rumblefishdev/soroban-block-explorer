---
id: '0624'
title: 'Mainnet API gets the Cloudflare managed WAF; docs stop claiming protections that are off'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0553', '0277', '0302']
tags: ['security', 'cloudflare', 'docs', 'effort-small', 'priority-high']
links: []
history:
  - date: 2026-10-06
    status: backlog
    who: karolkow
    note: 'Spawned from 0553 launch: mainnet has run without a WAF since 2026-07-27.'
---

# Mainnet API gets the Cloudflare managed WAF; docs stop claiming protections that are off

## Summary

The mainnet API (`api-sorobanscan.rumblefishdev.com`) has had **no WAF** since
2026-07-27: task 0302 dropped AWS WAF on the premise that Cloudflare's managed
rules covered it, but the zone's `enable_managed_waf` flag was never switched
on after the nameserver move, and its ruleset id default was wrong. Testnet got
the Free Managed Ruleset on 2026-10-05; mainnet joins once testnet shows no
false positives. Separately, our docs claim the WAF and a Managed Challenge
that are not active.

## Stan teraz

- Done: Free Managed Ruleset live on the testnet API host only (zone
  Terraform, company `dns-cloudformation` repo); a Log4j-shaped probe gets
  403 on testnet, 401 on mainnet.
- Next: 2026-10-07 — read testnet's Security Events (managed rules, ~40 h);
  if no false positives, add the mainnet host to the WAF expression.
- In force: Managed Challenge stays **off** (an HTML challenge page on a JSON
  API breaks `fetch()` and API-key clients; Turnstile filters bots for the
  SPA). The Free plan allows one rate-limit rule per zone, so both API hosts
  share one per-IP budget.

## Implementation Plan

### Step 1: Mainnet WAF

In the zone repo (`cloudflare/security.tf`), widen the `managed_waf`
expression to both API hosts; plan shows one in-place update of
`cloudflare_ruleset.managed_waf[0]` and nothing else; the zone owner applies.

### Step 2: Docs say what is on

- `docs/scf/milestone-3-security-checklist.md:16` and `:35` — "managed WAF,
  rate-limit and Managed Challenge" → what is actually active.
- `docs/architecture/infrastructure/infrastructure-overview.md:346-347` — same.
- Note the shared rate-limit budget (Free plan, one rule).
- The 0302 claim inventory (`lore/1-tasks/archive/0302_*/notes/G-waf-claim-inventory.md`)
  is the list of places to re-check.

## Acceptance Criteria

- [ ] A Log4j-shaped probe to the mainnet API host answers 403 from Cloudflare.
- [ ] Testnet and mainnet Security Events show no managed-rule blocks of
      legitimate SPA or API-key traffic over the first 24 h after Step 1.
- [ ] No doc under `docs/**` claims a protection that is not active.
- [ ] **Docs updated** — `infrastructure-overview.md` (edge section) and the
      SCF checklist, per ADR 0032.
- [ ] **API types regenerated** — N/A, no API change.
