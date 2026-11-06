---
id: '0665'
title: 'Move the ClickHouse endpoint to ch.sorobanscan.com'
type: OPS
status: active
related_adr: []
related_tasks: ['0659', '0664', '0234', '0239']
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

- Done: nothing live yet.
- In progress (2026-11-06):
  - Step 1: soroban-infrastructure#10, `ch.sorobanscan.com` A `168.119.73.161`,
    DNS-only.
  - Step 2: Caddy's site address takes optional extra names from
    `CH_DOMAIN_ALIASES` (Ansible `ch_domain_aliases`), empty by default.
- Next: apply #10, then `--tags app` with `CH_DOMAIN=ch.sorobanscan.com` and
  `CH_DOMAIN_ALIASES=ch.sorobanscan.rumblefish.dev`; check both names.
- In force: both names stay valid until the last client has moved.

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

- [ ] `https://ch.sorobanscan.com/ping` answers with a client certificate and
      refuses the handshake without one.
- [ ] Every client is on the new name; ingestion lag stays normal.
- [ ] The old name is gone from Caddy; `HetznerDnsStack` removed.
- [ ] **Docs updated** — `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`,
      `docs/architecture/infrastructure/infrastructure-overview.md`,
      `infra-hetzner/README.md`, the runbooks naming the host.
- [ ] **API types regenerated** — run it: `crates/api/src/bin/local.rs`
      changes; expect no diff.
