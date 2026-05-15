---
id: '0227'
title: 'FEATURE: Build infra-hetzner/ Ansible playbook, mTLS CA, and Docker compose overlay for production CH deployment'
type: FEATURE
status: active
related_adr: ['0044', '0045']
related_tasks: ['0216']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    hetzner,
    ansible,
    mtls,
    deployment,
  ]
links: []
history:
  - date: '2026-05-15'
    status: active
    who: fmazur
    note: 'Spawned from task 0216 (high-level decisions). Builds the implementation artefacts.'
---

# FEATURE: Build `infra-hetzner/` deployment infrastructure

## Summary

Build the infrastructure-as-code artefacts that fulfil the high-level
decisions taken in task 0216 (Hetzner-hosted ClickHouse, mTLS-based
cross-cloud authentication, Ansible-driven provisioning). All code
lives in **`infra-hetzner/` at the repository root**.

## Context

Task 0216 captures the architectural decisions but does not produce
the runnable infrastructure code. This task delivers the playbook,
mTLS scaffolding, Caddy configuration, Docker compose overlay, and
ClickHouse configuration files that make the deployment executable.

The Hetzner box has already been provisioned with a fresh Ubuntu OS
on RAID 1 ext4. This task picks up from "fresh OS, SSH reachable" and
takes the box to "production stack running, mTLS-protected, backups
configured".

## Scope

The deliverable is a populated `infra-hetzner/` directory at repo
root containing:

### Ansible playbook (`infra-hetzner/ansible/`)

Idempotent playbook that takes a fresh Ubuntu box to the deployed
production stack. **Everything that is configurable post-order must
be declarative in this playbook — nothing managed via the Hetzner
Robot UI after server provisioning.** This includes:

**Hetzner-side (via `community.hrobot` collection, Robot REST API):**

- Stateless firewall rules at the network-switch level
- Reverse DNS for the server's IP
- Server display name / label
- SSH keys registered for rescue-mode access
- Storage Box authorized SSH keys for the backup destination

**OS-level (standard Ansible modules):**

- OS hardening (non-root deploy user, SSH key-only auth from GitHub
  public keys, root login disabled, `unattended-upgrades` with
  auto-reboot, `fail2ban` SSH jail, host firewall deny-incoming
  except `22/80/443`)
- Docker CE installation from the official apt repository
- Filesystem layout for the ClickHouse data directory and the local
  backup staging directory (correct ownership for the in-container
  ClickHouse user)
- Borg installation and the daily backup cron entry
- Repository checkout
- Public CA certificate copied to the Caddy mount path
- Docker compose stack started

**Constraint:** the only manual steps allowed are the initial Robot
UI order of the server and Storage Box (Hetzner does not provide
ordering APIs for the dedicated line). Everything else is reachable
from `ansible-playbook ...`.

### Self-signed mTLS CA tooling (`infra-hetzner/ca/`)

- One-time CA bootstrap script (run on a developer laptop)
- Client certificate issuance script (per AWS service, per developer)
- The public CA certificate (committed)
- The private CA key is **never committed** — it lives only in the
  team password manager

### Caddy reverse proxy configuration (`infra-hetzner/Caddyfile`)

- Server-side TLS termination using Let's Encrypt (automatic renewal)
- **Mutual TLS** — `require_and_verify` against the committed public
  CA, rejecting any handshake without a CA-signed client certificate
- Reverse proxy to the ClickHouse container over the Docker bridge

### Docker compose production overlay (repo root)

`docker-compose.prod.yml` overlay layered on top of the existing
local-development `docker-compose.yml`:

- Caddy service (TLS + mTLS)
- ClickHouse port binding restricted to loopback only
- Bind mounts for data, logs, and backup staging
- Sidecar overlay using production-flavoured environment
- Postgres service profile-excluded so it does not start in
  production

### ClickHouse configuration additions

- `crates/db-clickhouse/config.d/memory.xml` — memory tuning
- `crates/db-clickhouse/config.d/prometheus.xml` — native metrics
  endpoint on loopback
- `crates/db-clickhouse/users.d/dict.xml` — localhost-only,
  no-password user used by the Dictionary `SOURCE` clause
- Update `crates/db-clickhouse/schema/init.sql` to use the new user
  in the Dictionary `SOURCE`

### OS install template (`infra-hetzner/installimage.conf`)

Declarative Hetzner `installimage` configuration: Ubuntu 24.04 LTS,
mdadm RAID 1, single ext4 root, separate ext4 `/boot`, no swap.

### Runbook (`infra-hetzner/README.md`)

First-deploy procedure and disaster-recovery procedures.

## Security requirements

Mutual TLS is the primary cross-cloud authentication mechanism and
**must be enforced**:

- Caddy `tls.client_auth { mode require_and_verify }` directive set;
  any TLS handshake without a CA-signed client cert is rejected
  before any HTTP request reaches ClickHouse
- The public CA certificate (`infra-hetzner/ca/ca.crt`) is the only
  artefact in the repo; the private CA key never enters the repo
- ClickHouse remains bound to loopback only; the only public port is
  Caddy's `:443` (plus `:80` for the ACME http-01 challenge and
  redirect)
- Compose `ports: !override` is mandatory for every service the
  production overlay rebinds — required to prevent the base file's
  publicly-bound ports from being silently appended

## Acceptance Criteria

- [ ] Ansible playbook runs cleanly and idempotently against a fresh
      Ubuntu 24.04 box
- [ ] Docker compose stack (Caddy + ClickHouse + sidecar) starts
      successfully and survives a reboot
- [ ] Mutual TLS handshake works end-to-end with a CA-signed client
      certificate
- [ ] Synthetic negative test: connection without a client certificate
      is rejected at the TLS-handshake stage
- [ ] Borg backup script runs successfully against the BX21 Storage
      Box destination
- [ ] Schema applied via the sidecar on every boot, idempotent
- [ ] CA generation and client-cert issuance scripts produce a
      working certificate chain
- [ ] Runbook in `infra-hetzner/README.md` covers first-deploy and
      disaster-recovery procedures
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      updated only if architecture details described there change
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`

## Out of Scope

- Actual production deployment of the playbook against the live box
  (separate operational work; this task delivers the artefacts)
- AWS-side cutover (Lambda VPC removal, Galexie public subnet move,
  RDS decommissioning) — separate work in `infra/src/`
- Backup restore drill (separate operational task)
- Monitoring stack beyond the native ClickHouse Prometheus endpoint
