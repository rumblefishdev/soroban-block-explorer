---
id: '0667'
title: 'Let Galexie reach the SDF history archives over http (egress 80)'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0665']
tags: [infra, galexie, networking, priority-medium, effort-small]
links: []
history:
  - date: '2026-11-06'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0665. A Galexie restart paused production ingestion for 83
      minutes; the slow catch-up traced to a blocked port 80.
---

# Let Galexie reach the SDF history archives over http (egress 80)

## Summary

The ECS security group in `network-stack.ts` (`EcsSg`, production
`sg-0fdd7da30d4ceba0c`) allows egress on TCP 443, 53 and 11625 only. Galexie's
captive core uses the default pubnet archive list (`network = "pubnet"` in its
`config.toml`), which reaches the three SDF archives at
`http://history.stellar.org/...`. Every such fetch hangs until curl's connect
timeout. After a restart, catch-up runs one checkpoint per round. A round that
picks SDF takes 5–10 minutes instead of under one, so the node trails the
network for a long time.

## Context

- 2026-11-06, restart for 0665 (task definition rev 11): ingestion paused
  13:28–14:51 UTC.
- From 13:29 on, 19 SDF fetches ended in `exit 28` (timeout). Other archives
  failed too: Franklin Templeton `exit 6` (DNS), creit.tech and publicnode
  `exit 22` (newest files missing). Only Lobstr, OBSRVR and similar served.
- Earlier restarts took 21 min (2026-10-28) and 33 min (2026-11-06 00:14 UTC),
  probably for the same reason.

## Implementation

- `ecsSg.addEgressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(80), …)`, with a
  comment naming the SDF archives.
- `cdk diff` of `Explorer-production-Network` should show only the new egress
  rule. Security group rules apply to the running task's ENI, so Galexie needs
  no restart.
- Alternative, not chosen: an explicit https archive list in
  `[stellar_core_config]`. It means owning a captive core config and restarting
  Galexie.

## Acceptance Criteria

- [ ] The production ECS security group allows egress TCP 80.
- [ ] On the next Galexie restart, no `exit 28` from `history.stellar.org`, and
      catch-up finishes well under 20 minutes.
- [ ] **Docs updated** — the comment in `network-stack.ts`;
      `docs/architecture/infrastructure/infrastructure-overview.md` if it lists
      the ECS egress.
- [ ] **API types regenerated** — N/A.
