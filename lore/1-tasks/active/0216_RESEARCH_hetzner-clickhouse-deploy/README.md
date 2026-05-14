---
id: '0216'
title: 'RESEARCH: Hetzner production ClickHouse — server selection, provisioning, deploy mechanism'
type: RESEARCH
status: active
related_adr: ['0044', '0045']
related_tasks: []
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    hetzner,
    clickhouse,
    deployment,
    blocks-prod-deploy,
  ]
links:
  - https://www.hetzner.com/dedicated-rootserver
  - https://www.hetzner.com/cloud
  - https://docs.hetzner.com/
history:
  - date: '2026-05-13'
    status: backlog
    who: fmazur
    note: >
      Spawned during ADR 0045 discussion. ADR 0045 commits to a local-backfill
      → mirror-to-Hetzner path that assumes a production Hetzner ClickHouse
      box exists with the same Docker compose topology as local. None of that
      infrastructure has been chosen, provisioned, or scripted yet. This task
      answers every question needed to stand it up before the 11.5M-ledger
      backfill kicks off.
  - date: '2026-05-13'
    status: active
    who: fmazur
    note: 'Promoted to active — research starts now to unblock ADR 0045 production deploy path.'
---

# RESEARCH: Hetzner production ClickHouse — server selection, provisioning, deploy mechanism

## Summary

ADR 0045 commits to deploying the local Dockerised ClickHouse onto a Hetzner
server as the production target, but **no Hetzner infrastructure has been
chosen or stood up**. This research task answers every operational question
needed before the production box can exist — from "which Hetzner product
line" to "is the deploy `git push` triggered or `ssh root@…` manual" — and
produces a runbook ready to execute.

## Status: Backlog

Blocks the production deploy described in ADR 0045. Mirror via FREEZE +
rsync + ATTACH PART (the ADR 0045 plan) cannot start until this task ships
a reachable, schema-applied Hetzner CH endpoint.

## Context

The local development setup is `docker compose up clickhouse` against the
service defined in `/docker-compose.yml`, which uses
`clickhouse/clickhouse-server:26.3` with volumes for data and config
(`crates/db-clickhouse/config.d/timeouts.xml`,
`crates/db-clickhouse/users.d/timeouts.xml`) plus the
`db-clickhouse-init` sidecar that applies
`crates/db-clickhouse/schema/init.sql` on every boot. Production needs
the equivalent on a Hetzner box — same image, same schema, same config —
plus everything that makes a server actually-production (SSH, firewall,
TLS, backups, monitoring, secrets).

The team has no prior Hetzner deploy. The team is small (4 people, all
developers, no dedicated DevOps). Decisions should favour boring,
well-documented, low-ongoing-maintenance choices over clever ones.

## Research Questions

Each question below must end with a concrete recommendation, not a survey.

### Hardware / product line

- **Q1.** Which Hetzner product line: **Dedicated Root Server** (AX line,
  monthly billing, ≥1 Gbps unmetered) vs **Cloud Server** (CCX/CPX line,
  hourly billing, 20 TB/mo included)? Decision drivers: 800 GB on-disk
  CH footprint (and growing as backfill catches up to live), CPU
  characteristics for queries, ease of scaling later.
- **Q2.** Concrete SKU recommendation with specs justification. Floor:
  ≥1 TB NVMe (for the 800 GB + headroom + WAL + merges), ≥32 GB RAM
  (CH likes RAM for marks cache and query workspace), ≥6 cores
  (parallel query). Compare 2–3 candidates with monthly cost.
- **Q3.** Disk layout: single NVMe? RAID1 mirror across two NVMes
  (only on AX line)? Hetzner Volume? Implications for `ALTER TABLE …
FREEZE` hardlinks (must stay on same FS as `store/`).
- **Q4.** Hetzner Storage Box / Object Storage role — is it needed for
  backup destination, or is rsync-back-to-laptop / second-box sufficient?

### Provisioning model

- **Q5.** How is the box created: manual via web console, `hcloud` CLI,
  Terraform, Ansible, Pulumi? For a single-server deploy with no
  pre-existing IaC, what is the idiomatic minimum-overhead choice?
- **Q6.** OS image: Ubuntu LTS (22.04 / 24.04) vs Debian stable vs
  Hetzner's "Docker"-prebuilt image. Pick one and justify against
  Docker compatibility, security update cadence, team familiarity.
- **Q7.** Initial OS hardening checklist: SSH key-only auth, root login
  disabled, non-root deploy user, automatic security updates
  (`unattended-upgrades`), `ufw` / Hetzner Cloud firewall, fail2ban —
  what's the minimum viable set?

### Docker + ClickHouse stack on the box

- **Q8.** Docker installation: official `docker.io` apt repo, snap,
  Hetzner's pre-installed image, or rootless Docker / Podman? Trade-offs
  for a CH server (resource-hungry, expects normal cgroups behaviour).
- **Q9.** How does `docker-compose.yml` reach the server: `git pull`
  on the server + `docker compose up -d`, CI/CD push, GitOps, manual
  scp? Pick one with rationale.
- **Q10.** Config overlay: production needs different `CLICKHOUSE_PASSWORD`,
  different `CLICKHOUSE_HTTP_PORT` exposure, possibly different
  `timeouts.xml`. Strategy: `docker-compose.prod.yml` override file,
  `.env` file, Docker secrets, external secret store?
- **Q11.** Schema application: same `db-clickhouse-init` sidecar pattern?
  Or one-shot apply during deploy with `clickhouse-client --queries-file`?
  Idempotency story for re-runs after schema changes.
- **Q12.** Volume strategy: bind mount to `/srv/clickhouse-data` on the
  host (visible, easy to back up) vs named Docker volume (encapsulated,
  harder to inspect from host). Lean towards bind-mount on a single-box
  deploy for `FREEZE`+rsync workflow visibility.

### Network exposure & security

- **Q13.** What's the public surface of the production CH: 8123 (HTTP)
  / 9000 (native) exposed to the world, reverse-proxied behind nginx
  with TLS termination, VPN-only (WireGuard / Tailscale), or
  SSH-tunnel-only? Decision driver: who consumes — `crates/api` on
  the same box, or external client too?
- **Q14.** Co-location of `crates/api` (Rust API server) on the same
  Hetzner box vs separate boxes. Latency, deploy independence, blast
  radius trade-offs.
- **Q15.** TLS: Let's Encrypt via Caddy/nginx-acme-companion, or CH
  native TLS, or off (intra-host only)? Cert renewal automation.
- **Q16.** Secrets handling for `CLICKHOUSE_PASSWORD`, future
  external API keys (Oskar price API): `.env` file with restricted
  perms, Docker secrets, age/sops-encrypted in repo, external
  secret store? What's proportionate for this team size?

### Operations

- **Q17.** Backup story: how often, where to, automated how, retention
  policy. Should it be `FREEZE`+rsync-to-Storage-Box on a cron, native
  `BACKUP` statement, or LVM/ZFS snapshots? Cost projection.
- **Q18.** Monitoring: Hetzner's built-in basic metrics, `node_exporter`
  - Prometheus + Grafana stack, `clickhouse-exporter`, or roll something
    smaller (CH `system.metric_log` + Grafana CH datasource)? What signals
    matter (disk, replication lag if HA later, query latency, merger
    backlog).
- **Q19.** Log shipping: stay in `journald` / Docker logs on the box, or
  ship to Loki / Datadog / similar? Volume estimation for a
  production-traffic CH.
- **Q20.** Update / patch cadence: how do CH minor version upgrades
  happen? Rebuild box from scratch, in-place `docker compose pull` +
  restart, or blue-green with second box?

### Disaster recovery

- **Q21.** Restore drill: if the production box dies, what is the
  recovery procedure and ETA? Where does the data come from — most
  recent backup, re-run backfill from S3, or rsync from the
  intentionally-kept local CH (per ADR 0045)?
- **Q22.** Where do the **provisioning scripts / IaC / runbook** live
  in the repo? `infra/hetzner/` directory? Top-level `ops/`? Inline
  in `crates/db-clickhouse/`? Make a final call so the deliverable
  has a home.

### Cost

- **Q23.** Total monthly cost projection: server (Q2) + bandwidth
  (likely free for CH workload but check Cloud-line 20 TB cap) +
  Storage Box for backups (if chosen in Q4) + any IaC tooling that
  isn't free. Compare against the alternative of running on AWS /
  GCP for sanity.

## Deliverables

This task is "done" when the following exist and are reviewed by the team:

1. **One concrete recommendation per Q1–Q23** documented under
   `notes/` (Q-prefix one per question or grouped synthesis notes with
   S-prefix conclusions).
2. **`docs/architecture/infrastructure/infrastructure-overview.md`
   updated** with the chosen Hetzner topology (server SKU, network
   exposure, backup destination, related-services co-location).
3. **`infra/hetzner/`** directory created with whatever artefacts the
   provisioning-model decision (Q5) implies — at minimum a `README.md`
   describing the manual-provision steps, ideally Terraform/Ansible
   scripts if that path is chosen.
4. **`docker-compose.prod.yml`** (or equivalent overlay) added to the
   repo root, ready to deploy on the chosen box.
5. **Runbook** for first deploy of the production CH (provision box
   → harden OS → install Docker → clone repo → start compose stack
   → verify CH responds → ready for ATTACH PART from local mirror).
6. **Promotion of the next task**: a FEATURE task spawned for the
   actual execution of the runbook (provisioning + deploy), once
   this research lands.

## Acceptance Criteria

- [ ] Q1–Q23 each have a written recommendation in `notes/` with
      rationale (not just a yes/no).
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md`
      updated with the production Hetzner topology.
- [ ] `infra/hetzner/README.md` exists with manual-provision steps at
      minimum.
- [ ] `docker-compose.prod.yml` (or chosen overlay name) committed
      and verified to start CH locally with prod-flavoured config.
- [ ] First-deploy runbook reviewed by at least one team member other
      than the implementer.
- [ ] Follow-up FEATURE task spawned for actual provisioning + deploy
      execution; this RESEARCH task moves to `archive/`.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      is the primary doc affected; updated in the same PR as the
      runbook lands.
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Out of Scope

- Standing up an HA / replicated CH cluster — this task targets a
  single-box deploy. HA is a separate decision that, if taken, will
  trigger an ADR-level revisit of the schema (Replicated\* engines).
- Migrating `crates/api` to Hetzner — Q14 decides co-location, but
  the actual API deploy is a separate task.
- Choosing a CI/CD platform — if Q9 lands on "CI/CD pipeline", the
  pipeline implementation is a follow-up task.

## Notes

Lean towards **boring, single-box, well-documented, low-ongoing-maintenance**
choices over clever ones. The team is small and has no Hetzner experience
to draw on. The goal is a production box that can be re-provisioned from
runbook by anyone on the team in a single afternoon if it dies.
