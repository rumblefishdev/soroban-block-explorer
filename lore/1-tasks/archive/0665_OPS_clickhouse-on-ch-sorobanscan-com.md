---
id: '0665'
title: 'Move the ClickHouse endpoint to ch.sorobanscan.com'
type: OPS
status: completed
related_adr: []
related_tasks: ['0659', '0664', '0234', '0239', '0667', '0668']
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
  - date: '2026-11-06'
    status: completed
    who: stkrolikiewicz
    note: >
      Done in one day, in 4 steps. Explorer PRs #721–#727, soroban-infrastructure
      #10, stellar-prices-api #415. Every client is on ch.sorobanscan.com, and the
      old name is gone from Caddy and Route 53. Spawned 0667 (Galexie egress 80)
      and 0668 (Robot firewall rules).
---

# Move the ClickHouse endpoint to ch.sorobanscan.com

## Summary

The ClickHouse mTLS endpoint moved from `ch.sorobanscan.rumblefish.dev` to
`ch.sorobanscan.com` with no gap: Caddy served both names while the clients
moved one by one.

## Stan teraz

- Done (2026-11-06), all times UTC:
  - Step 1: soroban-infrastructure#10, `ch.sorobanscan.com` A `168.119.73.161`,
    DNS-only.
  - Step 2: #721 plus `--tags app` at 12:38 with `CH_DOMAIN_ALIASES` set to
    the old name. Caddy served both names.
  - Step 3, clients, in this order:
    - testnet (#722, 13:13);
    - production Compute and Ingestion (#723, release #724, 13:25–13:32);
    - stellar-prices-api Compute, then EventBridge with `--force`
      (14:14–14:17);
    - defaults and docs (#726, stellar-prices-api #415).
  - Step 4:
    - #727 removed `HetznerDnsStack` and `provisionChDns`.
    - Caddy's log then showed 0 requests to the old name against 49,071 to
      the new one since 14:20.
    - `Explorer-production-HetznerDns` was deleted at 15:43, taking the old
      Route 53 record.
    - `--tags app` with `CH_DOMAIN_ALIASES` empty ran at 15:54. Caddy serves
      only the new name, and the old one's handshake is refused.
    - Reverse DNS is `ch.sorobanscan.com`.
- In force: `CH_DOMAIN=ch.sorobanscan.com`, with `CH_DOMAIN_ALIASES` empty or
  unset. The variable stays for a future rename.

## Acceptance Criteria

- [x] `https://ch.sorobanscan.com/ping` answers with a client certificate and
      refuses the handshake without one.
- [x] Every client is on the new name; ingestion lag stays normal. Apart from
      the Galexie pause below, it did.
- [x] The old name is gone from Caddy; `HetznerDnsStack` removed.
- [x] **Docs updated** — the overviews, `infra-hetzner/README.md`,
      `infra/README.md`, `docs/deployment.md` and the runbooks here and in
      stellar-prices-api. Dated SCF evidence stays as submitted.
- [x] **API types regenerated** — CI's "API types freshness" passed on #726
      (`local.rs` changed only an env default).

## Design Decisions

### From Plan

1. **Both names in Caddy during the move**, a DNS-only record, and clients one
   at a time. Each switch was safe on its own.

### Emerged

2. **`CH_DOMAIN_ALIASES`, not a second site block.** Caddy substitutes
   `{$VAR}` before parsing, so one optional variable adds names and is empty
   by default.
3. **The IP is in `clickhouse.tf`, not tfvars.** It's a fixed server, and a
   stale tfvars could plan the record away.
4. **Prices after the explorer.** Prices reads
   `/platform/production/ch-domain`, which Explorer-production-Compute
   writes. A manual `put-parameter` would have drifted that stack.
5. **`provisionChDns: false`, then removal.** The new name is outside the
   Route 53 zone, so production stopped synthesizing the stack first. The
   deployed stack was deleted with `aws cloudformation delete-stack` once
   Caddy's log showed no old-name traffic.

## Issues Encountered

- **A changed `/srv/app/.env` recreates the whole stack**, ClickHouse too.
  Two such runs (12:43, 15:54) each cost about 20 s of write failures, all
  redelivered. Four alarms (`*-indexer-*failures`, `*-ingestion-backlog-age`)
  went off for 6–15 minutes.
- **The Galexie restart paused production ingestion 13:28–14:51.** Ledgers
  stayed contiguous from `65337130`.
  - The new task's captive core rebuilt state from 38 buckets.
  - Catch-up then ran one checkpoint per round. The ECS security group lacks
    egress on 80, so every fetch from the SDF archives timed out (`exit 28`,
    19 times). That is 0667.
- **The `hetzner` play fails on "Apply Robot stateless firewall rules".**
  `community.hrobot.firewall` now requires `ip_version` on rules that use
  `protocol`. The playbook stopped after reverse DNS, before `app`; a second
  run with `--tags app` alone finished the job. Nothing reached the Robot API.
  That is 0668.
- **`soroban-prod.env` drives every run.** `CH_DOMAIN` in it moved with each
  step. Operators' copies must say `ch.sorobanscan.com`.

## Future Work

- 0667: Galexie egress on 80 for the SDF history archives.
- 0668: `ip_version` on the Robot firewall rules.
