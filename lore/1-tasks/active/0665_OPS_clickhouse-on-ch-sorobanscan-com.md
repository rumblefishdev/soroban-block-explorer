---
id: '0665'
title: 'Move the ClickHouse endpoint to ch.sorobanscan.com'
type: OPS
status: active
related_adr: []
related_tasks: ['0659', '0664', '0234', '0239', '0667']
tags: [infra, infra-hetzner, clickhouse, mtls, priority-low, effort-medium]
links: []
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Last step of the move.'
  - date: '2026-11-06'
    status: active
    who: stkrolikiewicz
    note: >
      Started: the DNS record (soroban-infrastructure#10) and Caddy serving
      both names (CH_DOMAIN_ALIASES).
---

# Move the ClickHouse endpoint to ch.sorobanscan.com

## Summary

Move the ClickHouse mTLS endpoint from `ch.sorobanscan.rumblefish.dev` to
`ch.sorobanscan.com` with no gap: Caddy serves both names while the clients
move one by one. The indexer writes through this host, so a mistake stops live
ingestion.

## Stan teraz

- Done (2026-11-06):
  - Step 1: soroban-infrastructure#10, `ch.sorobanscan.com` A `168.119.73.161`,
    DNS-only.
  - Step 2: #721, then `--tags app` (stkrolikiewicz, 12:38 UTC) with
    `CH_DOMAIN=ch.sorobanscan.com` and
    `CH_DOMAIN_ALIASES=ch.sorobanscan.rumblefish.dev`. Both names answer `/ping`
    with a client certificate (`dev_shared`) and refuse the handshake without
    one.
  - The run recreated the whole stack (a changed `.env` does that): 4 + 1
    indexer write failures at 12:43 UTC, redelivered. Four alarms
    (`*-indexer-*failures`, `*-ingestion-backlog-age`) were OK again by 12:58.
  - Step 3, AWS clients:
    - Testnet: #722, `make deploy-testnet` at 13:13 UTC.
    - Production: #723 and release #724. Compute and Ingestion were deployed
      from a laptop at 13:25–13:32 UTC, from `a1fae52a`.
    - stellar-prices-api: Compute, then EventBridge with `--force` (its
      template doesn't change: the stacks read the name through an SSM-typed
      parameter). Deployed 14:14–14:17 UTC from `bc315639`. All 12 Prices
      Lambdas are on the new name, with 0 errors.
  - The Galexie restart (task definition rev 11) paused ingestion 13:28–14:51
    UTC. Ledgers stayed contiguous in S3 from `65337130`.
    - 13:29–13:56: captive core rebuilt state from 38 buckets.
    - 14:02–14:51: catch-up ran one checkpoint per round, at 5–10 min a round.
    - Cause: the ECS security group lacks egress on 80, so every fetch from
      the SDF archives (`http://history.stellar.org`) timed out (`exit 28`,
      19 times). Follow-up 0667.
- Next: operators, defaults and docs (`local.rs`, backfill-runner tests,
  stellar-prices-api `reingest_0286.py`, runbooks), then Step 4.
- In force:
  - Both names stay valid until the last client has moved.
  - Every Ansible run keeps `CH_DOMAIN=ch.sorobanscan.com` and
    `CH_DOMAIN_ALIASES=ch.sorobanscan.rumblefish.dev` until Step 4. An old
    `CH_DOMAIN` alone drops the new name from Caddy.
- Found:
  - Prices reads `/platform/production/ch-domain`, which
    Explorer-production-Compute writes from `chDomainName`. So Prices follows
    the explorer's production deploy, not the other way round.
  - Production builds `HetznerDnsStack` from `chDomainName` in the Route 53
    zone `sorobanscan.rumblefish.dev`. The new name is outside it, so
    production switches with `provisionChDns: false`. The stack stays deployed
    and keeps the old record until Step 4.

## Context

- `infra-hetzner/Caddyfile` serves `{$CH_DOMAIN}` with a Let's Encrypt server
  certificate and verifies client certificates against the team CA. Client
  certificates carry no server name, so none is re-issued.
- The record must be DNS-only: Cloudflare's proxy terminates TLS, and the
  client certificate would never reach Caddy.
- The name lives in:
  - `chDomainName` in `infra/envs/*.json`
  - `CH_DOMAIN` on the Lambdas (`compute-stack.ts`, `ingestion-stack.ts`)
  - Ansible `ch_domain` and `hetzner_reverse_dns`
  - `HetznerDnsStack` (the Route 53 A record)
  - defaults in `crates/api/src/bin/local.rs` and `crates/backfill-runner/tests/*`
  - runbooks in this repo and in stellar-prices-api, which reads the same host
- The Route 53 zone `sorobanscan.rumblefish.dev` stays while 0664's redirects
  live in it.

## Implementation Plan

### Step 1: DNS

DNS-only A record `ch.sorobanscan.com` → the server IP, in soroban-infrastructure.

### Step 2: Caddy serves both names

Both names in the site address (Ansible var). Let's Encrypt needs the DNS in
place first (`infra-hetzner/README.md`).

### Step 3: Clients

Explorer Lambdas (`chDomainName`, one CDK deploy), the Prices API,
backfill-runner and operators. With both names valid each switch is safe.

### Step 4: Cleanup

Drop the old name from Caddy, point reverse DNS at the new one, remove
`HetznerDnsStack`, and update the defaults and runbooks.

## Acceptance Criteria

- [x] `https://ch.sorobanscan.com/ping` answers with a client certificate and
      refuses the handshake without one (2026-11-06).
- [ ] Every client is on the new name; ingestion lag stays normal.
- [ ] The old name is gone from Caddy; `HetznerDnsStack` removed.
- [ ] **Docs updated** — `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`,
      `docs/architecture/infrastructure/infrastructure-overview.md`,
      `infra-hetzner/README.md`, the runbooks naming the host.
- [ ] **API types regenerated** — run it: `crates/api/src/bin/local.rs`
      changes; expect no diff.
