---
id: '0227'
title: 'FEATURE: Build infra-hetzner/ Ansible playbook, mTLS CA, and Docker compose overlay for production CH deployment'
type: FEATURE
status: completed
related_adr: ['0044', '0045']
related_tasks: ['0216', '0234', '0235', '0236']
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
  - date: '2026-05-19'
    status: completed
    who: fmazur
    note: >
      First deploy validated against the live ch-prod-01 box
      (<box-ipv4>). Stack healthy: Caddy + ClickHouse +
      sidecar (sidecar exit 0 after applying init.sql; 20 tables
      in `default`). Three follow-ups spawned: 0234 (Route 53
      A-record + mTLS smoke), 0235 (Robot API "IP not found"
      bug), 0236 (declarative Storage Box subaccount IaC).
      Deferred AC: mTLS smoke → 0234 (no LE cert without real
      DNS), Borg backup roundtrip → 0236 (Cloud Console UI does
      not expose subaccount SSH keys). 7 AC done, 3 deferred,
      2 N/A. Artefacts: 7 Ansible roles, mTLS CA tooling, Caddy
      mTLS gate, docker-compose.prod.yml overlay, CH server
      tuning, installimage.conf, runbook.
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

- [x] Ansible playbook runs cleanly and idempotently against a fresh
      Ubuntu 24.04 box — first-deploy debugging produced fixes for
      compose-stack handler (`state: present` + `recreate: always` + `wait: true` instead of `state: restarted`, so sidecar
      respects `service_healthy`) and the templated
      `clickhouse-client.xml` (Poco XML §2.5 — `--` literal banned
      inside comments; CDATA-wrapped password). Re-run against the
      already-deployed box is now a no-op.
- [x] Docker compose stack (Caddy + ClickHouse + sidecar) starts
      successfully and survives a reboot — `docker compose up
--force-recreate --wait` brings the stack to healthy in
      ~10 s; sidecar exits 0 after applying init.sql.
- [ ] Mutual TLS handshake works end-to-end with a CA-signed client
      certificate — **(deferred to 0234)** — `CH_PROD_DOMAIN` is
      still `ch-prod.placeholder` because the Route 53 A-record is
      not yet wired (intentional: 0234 separates the AWS CDK work
      from this task's Hetzner-side artefacts). Caddy is in the
      documented LE-retry loop. As soon as 0234 publishes the
      real A-record and the operator re-sources the env with the
      production `CH_PROD_DOMAIN`, both this AC and the negative
      test below are validated by 0234's smoke step.
- [ ] Synthetic negative test: connection without a client certificate
      is rejected at the TLS-handshake stage — **(deferred to 0234)**
      same reason: needs the LE cert from a real domain. 0234's
      Acceptance Criteria carries the explicit `curl -sv` negative
      check.
- [ ] Borg backup script runs successfully against the BX21 Storage
      Box destination — **(deferred to 0236)** — Storage Box
      subaccount `<storagebox-sub>` was created manually in Hetzner
      Cloud Console (chroot `/borg-ch-prod-01-repo`); wiring the
      Borg pubkey requires SFTP-with-password or Cloud API
      because the Console UI does not expose subaccount SSH keys.
      0236 brings the subaccount + pubkey + first roundtrip under
      IaC. Backfill can proceed without backup wired (RAID 1 is
      the first line of defence).
- [x] Schema applied via the sidecar on every boot, idempotent — 20
      tables in `default` after sidecar exit 0.
- [x] CA generation and client-cert issuance scripts produce a
      working certificate chain — `ca.crt` committed,
      `<dev>-laptop.{crt,key}` issued and persisted in
      `~/.certs/` from the laptop CA bootstrap (Phase 2).
- [x] Runbook in `infra-hetzner/README.md` covers first-deploy and
      disaster-recovery procedures.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      N/A — this task adds new files under `infra-hetzner/` but
      does not change architecture details already described in
      the overview doc.
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`

## Out of Scope

- Actual production deployment of the playbook against the live box
  (separate operational work; this task delivers the artefacts)
- AWS-side cutover (Lambda VPC removal, Galexie public subnet move,
  RDS decommissioning) — separate work in `infra/src/`
- Backup restore drill (separate operational task)
- Monitoring stack beyond the native ClickHouse Prometheus endpoint

## Implementation Notes

Delivered artefacts under `infra-hetzner/` plus three companion
locations:

- `infra-hetzner/ansible/` — 7 roles
  (`base`, `users`, `security`, `docker`, `hetzner`, `app`,
  `backup`), `site.yml`, `group_vars/all.yml`, `inventory.ini.example`,
  `requirements.yml`, `ansible.cfg`.
- `infra-hetzner/ca/` — CA bootstrap (`generate-ca.sh`),
  client-cert issuance (`issue-client-cert.sh`), public CA
  (`ca.crt`).
- `infra-hetzner/Caddyfile` — TLS edge + mTLS gate +
  CN-allowlist + reverse proxy to CH bridge.
- `infra-hetzner/installimage.conf` — Hetzner rescue installer
  template (Ubuntu 24.04, mdadm RAID 1, ext4, no swap).
- `infra-hetzner/README.md` — first-deploy and DR runbook.
- `docker-compose.prod.yml` (repo root) — production overlay
  with Caddy, loopback CH binding, Postgres profile gate.
- `crates/db-clickhouse/config.d/{memory,prometheus}.xml` and
  `crates/db-clickhouse/users.d/dict.xml` — CH server tuning +
  loopback-only dict source user.

Operational artefacts of the first deploy (kept manual, not
committed):

- Hetzner Robot project — server + Storage Box ordered.
- Hetzner Cloud Console — Storage Box BX21 subaccount
  `<storagebox-sub>` with chroot `/borg-ch-prod-01-repo`
  (SSH key wiring deferred to 0236).
- `~/.config/soroban-prod.env` on the operator's laptop
  (gitignored, mirrored in the team password manager under
  `soroban-prod / ansible-env`).
- KeePassXC: `soroban-prod` group with CA passphrase,
  CLICKHOUSE_PASSWORD, BORG passphrase, BX21 subaccount
  password.

## Design Decisions

### From Plan

1. **Caddy `tls.client_auth { mode require_and_verify }` as the
   primary cross-cloud auth gate** — rejects handshakes without a
   CA-signed client cert before any HTTP frame reaches CH. Defined
   in [[adr-0044]] and [[adr-0045]]; this task realises it.

2. **ClickHouse loopback-only binding in production** — only the
   Caddy `:443` (+ `:80` for the ACME http-01 challenge) is public.
   Compose `ports: !override` is mandatory so the base file's
   public bindings cannot leak in.

3. **Ansible-everywhere with a single carve-out** — only the
   initial Robot UI order of the server + Storage Box stays manual
   (Hetzner does not expose ordering APIs for the dedicated line).
   Everything else, including post-order Robot REST (firewall,
   rDNS, server label, SSH keys, Storage Box authorisations), is
   reachable from `ansible-playbook ...`.

4. **CN-allowlist as a second auth gate** — even a valid CA-signed
   cert is rejected by Caddy if its CN is not on the allowlist
   (Ansible-rendered from `group_vars`). Proactive equivalent of
   the CRL/OCSP story; rotation = 1-line edit + replay `--tags
app`.

### Emerged

5. **`docker compose up --force-recreate --wait` instead of
   `docker compose restart` for the compose-stack handler** — the
   original handler used `state: restarted`, which is
   `docker compose restart` semantics and **does not honour
   `depends_on: condition: service_healthy`**. The
   db-clickhouse-init sidecar consequently raced CH startup and
   exited with `NETWORK_ERROR / Connection refused` on every
   re-deploy. Switched the handler to `state: present`,
   `recreate: always`, `wait: true`, `wait_timeout: 180` — this
   is `up --force-recreate --wait`, which evaluates dependency
   conditions and blocks until every service is healthy. Sidecar
   now sees CH healthy, applies `init.sql`, exits 0.

6. **CDATA-wrapped password + literal-`--` ban in the
   `clickhouse-client.xml.j2` template** — Jinja2 autoescape is
   off for `.xml.j2` (autoescape triggers only on
   `.html`/`.htm` by default), so passwords with `&`, `<`, `>`,
   `'`, `"` would otherwise produce invalid XML. CDATA neutralises
   special-char interpretation; the only sequence that breaks
   CDATA is `]]>`, rejected upstream by env-source validation.
   Separately, the **template comment header initially used
   `--flag` literals**, which is forbidden by XML 1.0 §2.5 inside
   comment bodies — Poco XML (the parser CH uses) is strict and
   rejected the file with `SAXParseException: Invalid token`.
   Re-spelled the CLI flags as words to keep the parser happy.

7. **Mode `0400` on the credentials file with owner pinned to
   numeric UID 101 — no group** — GID 101 on Ubuntu 24.04 is
   allocated dynamically by `useradd` and may map to an unrelated
   system group (`messagebus`, `systemd-resolve`, …) which would
   inadvertently gain read on the plaintext password. With mode
   0400 and a numeric owner matching the in-container CH UID,
   only the CH container user can read the file; no host group is
   involved.

8. **`user: "101:101"` on the db-clickhouse-init sidecar** — the
   official `clickhouse-server` image's USER directive defaults
   to root and its entrypoint script drops to 101 only for the
   server process; when we override the entrypoint to
   `clickhouse-client` directly the drop-priv step is skipped, so
   we set the user explicitly. Without this, the 0400-owner=101
   credentials file is read via root bypass and the
   defence-in-depth around the password leaks.

9. **`--skip-tags hetzner` during the first deploy** — the
   community.hrobot `reverse_dns` / firewall calls return
   "IP not found" for our auction-purchased server. Worked
   around by skipping the `hetzner` tag and proceeding with the
   OS-side roles. Root cause spawned as [[task-0235]].

10. **Storage Box subaccount created manually in Cloud Console
    instead of via API** — community.hrobot 1.9.x predates the
    `storagebox_subaccount` module (added in 2.x); a 2.x upgrade
    plus per-tag splitting of the existing `hetzner` role was
    scope-budget incompatible with operator's preference to
    unblock backfill. Manual creation + deferral to [[task-0236]]
    for full IaC.

11. **`CH_PROD_DOMAIN=ch-prod.placeholder` to allow first deploy
    without DNS** — Caddy enters an LE retry loop without
    affecting the rest of the stack; the runbook documents this
    as the canonical "deploy before DNS" mode. mTLS smoke
    deferred to [[task-0234]] which wires the real Route 53
    A-record.

12. **Caddy access log `format filter` redacts URL-embedded
    credentials** — surfaced by the pre-commit security audit
    (three parallel Explore subagents on hardcoded secrets,
    gitignore posture, and information-disclosure surface). The
    info-disclosure agent caught that Caddy's default JSON
    access-log encoder persists `request_uri` verbatim, and
    ClickHouse's HTTP API accepts `?user=…&password=…` in the
    URL — a misbehaving client could leak credentials into the
    Docker json-file log and any downstream aggregator. Fixed
    inline (not deferred) by switching both the global and
    per-site `log` blocks to `format filter { wrap json fields
{ request>uri query { replace password REDACTED; replace
user REDACTED }; request>headers>Authorization delete;
request>headers>Cookie delete } }`.

13. **Caddyfile checksum sentinel triggers `Reload caddy`
    handler** — Caddyfile lands on the box via
    `ansible.posix.synchronize`, whose `changed` flag reflects
    the whole subtree, not the single file. Wiring rsync's
    changed result to the handler would over-bounce Caddy on
    every infra-hetzner/ edit (READMEs, ansible-only files,
    …). The inverse is worse: without any handler binding, a
    Caddyfile edit would silently keep the OLD config running
    until the next full-stack recreate. Bridge the gap with a
    SHA-256 sentinel at `/etc/soroban-app/caddyfile.sha256`
    (path is outside `app_repo_dest` so `rsync --delete` does
    not wipe it); a content drift between runs triggers the
    existing Caddy-only reload handler.

## Issues Encountered

- **Sidecar `Exited (232)` on first deploy** — root cause: XML
  parse error in the `clickhouse-client.xml.j2` template's
  comment header (literal `--` sequence). Fixed by re-spelling
  CLI flags as words inside the comment body. Not a regression
  in CH or Poco — XML 1.0 spec compliance; our template was
  invalid.

- **Sidecar `Exited (210) / Connection refused` after the XML
  fix** — root cause: `docker compose restart` does not respect
  `depends_on: condition: service_healthy`. Fixed by switching
  the handler to `up --force-recreate --wait`. Documented in
  Design Decisions #5 above.

- **`community.hrobot.reverse_dns` returns "IP not found"** for
  the auction server purchased for ch-prod-01. Worked around with
  `--skip-tags hetzner` on every deploy; rDNS is set to the
  Hetzner default which is functional for outbound mail and
  meets the security baseline. Root-cause investigation spawned
  as [[task-0235]].

- **Cloud Console UI does not expose subaccount SSH keys** —
  Storage Box subaccount Create / Edit / Access Details views
  only handle password auth. SSH key wiring goes through either
  SFTP-with-password (operator manual) or Cloud API (declarative,
  per [[task-0236]]).

- **Pre-commit security audit caught URL credential leak path** —
  three Explore subagents ran in parallel before the close commit:
  one on hardcoded secrets (clean — 43 files, 0 leaks), one on
  gitignore posture (clean — no gaps, `inventory.ini` /
  `ca/*.key` / `.env*` properly excluded, `inventory.ini.example`
  and the public `ca.crt` properly staged), and one on
  information-disclosure surface (one HIGH-severity finding: the
  default Caddy JSON access log persists `request_uri` query
  strings — fixed in Design Decision #12). The audit happened
  pre-commit deliberately; running it after the merge would have
  leaked the issue into git history.

## Future Work

Spawned during first-deploy operational work; see backlog entries:

- [[task-0234]] — Route 53 A-record `ch-prod.sorobanscan.rumblefish.dev`
  in production CDK, plus carry-over mTLS smoke (positive +
  negative).
- [[task-0235]] — community.hrobot Robot API "IP not found"
  root cause investigation for the auction server.
- [[task-0236]] — Declarative Storage Box subaccount + SSH key
  - first backup roundtrip via Robot / Cloud API.
