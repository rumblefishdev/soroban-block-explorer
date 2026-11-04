---
id: '0661'
title: 'Prepare sorobanscan.com: ACM certificates, API edge rules, Turnstile'
type: OPS
status: active
related_adr: ['0048']
related_tasks: ['0659', '0662', '0663', '0277', '0658']
tags: [infra, cloudflare, acm, priority-medium, effort-medium]
links: []
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659.'
  - date: '2026-11-04'
    status: active
    who: stkrolikiewicz
    note: >
      Started after 0660 shipped. Decided: the old SPA alias stays on the
      distribution until 0664, so the us-east-1 certificate carries the old
      names as SANs.
---

# Prepare sorobanscan.com: ACM certificates, API edge rules, Turnstile

## Summary

Before any host moves, the new zone needs certificates, the explorer API's edge
rules and the Turnstile hostnames. Without the `X-Edge-Secret` Transform Rule
the API answers `403` behind its new host.

## Stan teraz

- Done: Step 1. Both certificates `ISSUED` 2026-11-04, validated through
  soroban-infrastructure#5's record: us-east-1 `0bcafe74-2ec1-4624-8277-5a51112da65a`,
  eu-central-1 `c937aeef-20b3-4e94-ba90-ade97c7d00ec`.
- Next: Step 2 (soroban-infrastructure#4 plus the token's permissions) and
  Step 3 (Turnstile, with dns-cloudformation's owner).
- In force: the zone and its rulesets belong to soroban-infrastructure.
- Decided (2026-11-04): the old SPA alias stays until 0664. us-east-1:
  `sorobanscan.com`, `*.sorobanscan.com`, `sorobanscan.rumblefish.dev`,
  `*.sorobanscan.rumblefish.dev` (the old names validate through the Route 53
  record the current certificate uses). eu-central-1: `sorobanscan.com`,
  `*.sorobanscan.com`.

## Context

- The zone is soroban-infrastructure's `cloudflare/` (ssl strict, always HTTPS,
  TLS ≥ 1.2). A zone has one ruleset per phase and `x402.tf` already owns
  `http_config_settings`, so the explorer's records and rules go there too
  (its `versions.tf` says so), not into `infra/cloudflare/`.
- Production and testnet run `enableEdgeSecretLock: true`: the API Lambda
  rejects any request without `X-Edge-Secret` (`crates/api/src/common/edge_lock.rs`).
  Today a Transform Rule in the `rumblefishdev.com` zone (dns-cloudformation)
  injects it, next to the WAF / rate-limit / Managed Challenge rules.
- `enableAuthLayer: true`: `/auth/session` verifies Turnstile, and the widget
  (`turnstileSiteKey`, the same key on both networks) only accepts the
  hostnames on its list.

## Implementation Plan

### Step 1: ACM

`sorobanscan.com` + `*.sorobanscan.com` in us-east-1 (CloudFront, prod and
testnet; testnet already shares the prod CloudFront certificate) and in
eu-central-1 (API Gateway custom domains). DNS validation records in the zone,
DNS-only. If 0663 keeps the old SPA alias during the move, the us-east-1
certificate also needs the old names as SANs.

### Step 2: Edge rules in soroban-infrastructure

A new `cloudflare/explorer.tf`: the `X-Edge-Secret` Transform Rule for
`api.sorobanscan.com` and `api-testnet.sorobanscan.com`, with the value per
environment as a sensitive variable read from Secrets Manager, never in git.
Copy the WAF / rate-limit / challenge expressions from dns-cloudformation,
scoped by `http.host`. Check the token's permissions in `providers.tf`.

### Step 3: Turnstile

Add `sorobanscan.com` and `testnet.sorobanscan.com` to the widget.

## Acceptance Criteria

- [ ] Both certificates `ISSUED`; their ARNs recorded here.
- [ ] `terraform plan` in soroban-infrastructure adds the explorer rules and
      changes nothing of x402's.
- [ ] The Turnstile widget lists the new hostnames.
- [ ] **Docs updated** — N/A: nothing serves the new names until 0662.
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
