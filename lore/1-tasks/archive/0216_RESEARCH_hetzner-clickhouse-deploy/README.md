---
id: '0216'
title: 'RESEARCH: Hetzner production ClickHouse — server selection, provisioning, deploy mechanism'
type: RESEARCH
status: completed
related_adr: ['0044', '0045']
related_tasks: ['0227']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    hetzner,
    clickhouse,
    deployment,
  ]
links: []
history:
  - date: '2026-05-13'
    status: backlog
    note: 'Spawned during ADR 0045 discussion to plan the Hetzner production deployment.'
  - date: '2026-05-13'
    status: active
    note: 'Promoted to active.'
  - date: '2026-05-14'
    status: active
    note: >
      Scope revision: Hetzner hosts only the data plane; the API stays
      on AWS. AWS-side topology change to remove dependency on the
      NAT Gateway.
  - date: '2026-05-15'
    status: active
    note: 'Hardware ordered and provisioned.'
  - date: '2026-07-01'
    status: completed
    note: >
      Archived as completed. Research + provisioning delivered: server selected,
      hardware ordered and provisioned, deploy mechanism decided (data plane on
      Hetzner, API stays on AWS). Implementation follow-ups 0227 (ansible), 0239
      (AWS cutover) and 0240 (CH RBAC) are all archived and prod ClickHouse is
      live on Hetzner. All acceptance criteria met.
---

# RESEARCH: Hetzner production ClickHouse

This task selects, provisions and documents the production deployment
of the project's ClickHouse data store on a Hetzner-hosted dedicated
server.

## Status

In flight. Initial provisioning is done; remaining configuration and
the AWS-side cutover are in progress.

## Decisions

The high-level architectural decisions for this task are recorded in
`notes/S-decisions.md`. In short:

- The production data store moves from AWS-hosted PostgreSQL to a
  Hetzner-hosted ClickHouse.
- The application API remains on AWS.
- AWS-side topology is restructured so that compute (Lambdas + the
  ingestion task) no longer requires a NAT Gateway. Lambdas exit the
  VPC; the long-running ingestion task moves to a public subnet with
  a public IP.
- Authentication between AWS-side workloads and the Hetzner-hosted
  database is based on cryptographic identity (mutual TLS).
- Provisioning model is infrastructure-as-code, with the hardware
  itself ordered manually via the Hetzner control panel.

## Acceptance Criteria

- [x] High-level decisions documented in `notes/S-decisions.md`.
- [x] `docs/architecture/infrastructure/infrastructure-overview.md`
      updated with a high-level reference to the Hetzner data plane.
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
