---
id: '0661'
title: 'Prepare sorobanscan.com: ACM certificates, API edge rules, Turnstile'
type: OPS
status: completed
related_adr: ['0048']
related_tasks: ['0659', '0662', '0663', '0277', '0658']
tags: [infra, cloudflare, acm, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/4'
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/5'
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/6'
  - 'https://github.com/rumblefishdev/dns-cloudformation/pull/5'
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
  - date: '2026-11-05'
    status: completed
    who: stkrolikiewicz
    note: >
      Step 3 by moving the Turnstile widget to soroban-infrastructure (#6,
      imported, sitekey and secret kept) instead of editing it in
      dns-cloudformation, which forgets it in its #5. The widget lists the
      sorobanscan.com hosts beside the old ones; 0663 shipped on them.
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
- Done: Step 2. soroban-infrastructure#4 applied for both API hosts
  (2026-11-04), token given Transform Rules and Zone WAF Edit.
- Done: Step 3 (2026-11-05). The widget lives in soroban-infrastructure's
  `cloudflare/turnstile.tf` (#6), imported with its sitekey and secret, and
  lists `sorobanscan.com`, `testnet.sorobanscan.com` and the two
  `rumblefish.dev` hosts. dns-cloudformation#5 drops it from that repo with a
  `removed` block (`destroy = false`).
- In force: the zone, its rulesets and the Turnstile widget belong to
  soroban-infrastructure. Its token needs Account: Turnstile: Edit.
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

- [x] Both certificates `ISSUED`; their ARNs recorded here.
- [x] `terraform plan` in soroban-infrastructure adds the explorer rules and
      changes nothing of x402's (soroban-infrastructure#4, applied 2026-11-04).
- [x] The Turnstile widget lists the new hostnames (soroban-infrastructure#6,
      applied 2026-11-05).
- [x] **Docs updated** — `docs/deployment.md` says where the widget's hostnames
      live now.
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`.

## Design Decisions

### From Plan

1. **The explorer's Cloudflare config goes to soroban-infrastructure**, not
   `infra/cloudflare/`: a zone has one ruleset per phase, and `x402.tf` already
   owns one of them.

### Emerged

2. **SAN certificate for the old SPA names** (2026-11-04): the old alias stays
   on the distribution until 0664, so the us-east-1 certificate carries
   `sorobanscan.rumblefish.dev` and its wildcard too.
3. **Turnstile moved instead of edited** (2026-11-05). We had read-only access
   to dns-cloudformation, and Marcin's direction is that Soroban's Cloudflare
   config lives in soroban-infrastructure. Both routes needed one PR there, so
   the widget went whole: an `import` block in soroban-infrastructure
   (`<account_id>/<sitekey>`) and a `removed` block with `destroy = false` in
   dns-cloudformation. The import keeps the sitekey and secret; the imported
   secret was compared by hash with `soroban/{production,testnet}/auth/turnstile-secret`
   before and after the apply. Either repo's apply can run first, but until
   dns-cloudformation#5 is applied, an apply there from the old code resets
   `domains` to the two old hosts.

## Issues Encountered

- **No write on dns-cloudformation.** A dry-run push answered 403. Marcin gave
  maintainer access on 2026-11-05.
- **The import's plan answered 403** on `GET …/challenges/widgets/…`: the
  soroban-infrastructure token had no Turnstile permission. Account: Turnstile:
  Edit added in the dashboard; editing a token's permissions keeps its value.
- **The plan marks `secret` as changed** (a sensitive value re-read after the
  update), and `bot_fight_mode`, `clearance_level`, `ephemeral_id` as known after
  apply. An update does not rotate the secret; the hash check after the apply
  matched.
