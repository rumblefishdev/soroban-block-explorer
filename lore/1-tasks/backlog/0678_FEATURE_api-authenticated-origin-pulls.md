---
id: '0678'
title: 'FEATURE: Authenticated Origin Pulls between Cloudflare and the API'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0677', '0277']
tags: [phase-future, effort-medium, priority-low, layer-infra, security]
links: []
history:
  - date: 2026-11-19
    status: backlog
    who: karolkow
    note: 'Spawned from 0677 grilling (W6 B): a separate task for later, not part of the session cookie.'
---

# FEATURE: Authenticated Origin Pulls between Cloudflare and the API

## Summary

Today API Gateway accepts a request only when it carries the secret header
that Cloudflare adds (`crates/api/src/common/edge_lock.rs`). Anyone who learns
the secret can reach API Gateway directly, skip Cloudflare's rate limit and
use up the stage throttle (50 rps, shared), so real visitors get 429. Data
still needs a session or an API key. Authenticated Origin Pulls (AOP) makes
Cloudflare present a client certificate at the TLS handshake instead; API
Gateway refuses a connection without it.

## Context

- ADR 0048 Decision 3 chose zone-level AOP with our own client certificate;
  it was never built.
- The CDK already has a dormant truststore and mTLS
  (`infra/src/lib/stacks/api-gateway-stack.ts`, `provisionApiMtlsTruststore`,
  `enableApiMtls`, both `false`). `enableApiMtls` requires
  `enableCloudflareApiDomain` (`infra/src/lib/types.ts:708`), which is off:
  the live domain is `api.sorobanscan.com` / `api-testnet.sorobanscan.com`.
- The known hole: a regional API Gateway IP with SNI `api.sorobanscan.com`
  answers `/health` 200, data 403 (ADR 0048, note 2026-11-17).
- Research of 2026-11-19: task 0677
  `notes/R-facts-and-peers.md` (F11) and
  `notes/R-peers-standards-recon.md` (AOP row).

## Open points (grill before planning)

- Own certificate per hostname or zone-level; peers found (Lychee ltbase,
  truvity/grun) trust Cloudflare's global CA, which proves only "from
  Cloudflare", not "from our account".
- Direct `/health` probes and the origin-lock canary would be refused at
  the handshake.
- Renewal: a lapsed certificate stops every request; monitoring and a
  rotation runbook.
- Cloudflare side lives in the `soroban-infrastructure` repo (`cloudflare/`).
- Whether a handshake-refused request counts against the throttle or the
  bill: not established.

## Acceptance Criteria

- [ ] A direct request to API Gateway without Cloudflare's certificate is
      refused on testnet and mainnet; traffic through Cloudflare is unchanged.
- [ ] A renewal runbook and an alarm before the certificate expires.
- [ ] **Docs updated** — ADR 0048 and `docs/architecture/infrastructure/**`.
- [ ] **API types regenerated** — N/A, no `crates/api` change expected.

## Notes

Estimate from the 0677 research: 4–6 h first rollout, ~2 h per rotation.
