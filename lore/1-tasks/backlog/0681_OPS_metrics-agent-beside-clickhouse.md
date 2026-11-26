---
id: '0681'
title: 'OPS: explorer metrics agent beside ClickHouse, its rules on the monitoring VM'
type: OPS
status: backlog
related_adr: ['0062', '0054']
related_tasks: ['0682', '0683', '0476', '0237']
tags:
  [
    priority-medium,
    effort-large,
    layer-infrastructure,
    monitoring,
    prometheus,
    ansible,
  ]
links:
  - https://github.com/rumblefishdev/soroban-block-explorer/pull/762
history:
  - date: '2026-11-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from ADR 0062 (PR #762) as the foundation for ClickHouse and host alarms on the
      monitoring VM. Starts once the ADR merges as proposed: Step 1 settles its agreement gates,
      and the drills here and in 0682–0683 tick the rest before it is accepted.
---

# OPS: explorer metrics agent beside ClickHouse, its rules on the monitoring VM

## Summary

ADR 0062 sends ClickHouse and host facts to Prometheus on the shared monitoring VM
(soroban-infrastructure `monitoring/`). This task builds the path every later check uses: a
Prometheus agent next to ClickHouse that sends to the VM, the explorer's rules file checked in CI
and loaded on the VM at a pin, the agent's own silence rules, and the channel its alerts reach.
The data-check exporter (0682) and the textfile exporter (0683) plug into it.

## Stan teraz

- Done: nothing yet; ADR 0062 is proposed in PR #762.
- Next: Step 1 once PR #762 merges.
- In force: ADR 0062, decisions 2 and 3, and ADR 0054's rules 2–5 as restated there.

## Context

- The VM keeps 90 days (20 GB cap). Its Alertmanager has one Slack webhook, to
  `#stellar-x402-bot`. Ingest is at `metrics.sorobanscan.com` over mTLS, with writers allowed by
  CN in `metrics_writer_cns`; today that is only x402's agent.
- x402's agent is the model: `stellar-x402/deploy/metrics/` (config, digest-pinned images,
  `read_only`, `cap_drop: [ALL]`), its rules in `deploy/metrics/rules/x402-production.yml`, its
  CI `promtool` job, and its pins in soroban-infrastructure (`x402_rules_commit`,
  `x402_rules_sha256`).
- ClickHouse's endpoint is `127.0.0.1:9363` inside the container
  (`crates/db-clickhouse/config.d/prometheus.xml`), with all four families on.
- On ch-prod-01 the Compose project is `app` (`/srv/app`): the container is `app-clickhouse-1`,
  and every command passes both `docker-compose.yml` and `docker-compose.prod.yml`.

## Implementation Plan

### Step 1: Agreements before code (ADR 0062, Open)

- **The channel:** the explorer's operators read `#stellar-x402-bot`, or an explorer route with
  its own webhook in soroban-infrastructure's `alertmanager.yml`.
- **Reminders:** the shared repeat (24 h `warning`, 1 h `critical`) accepted for the explorer, or
  an explorer route with a longer one. 0237's "Why no cooldown" relies on it.
- **A second writer on the VM**, agreed with stellar-x402. A PR there changes ADR 0011's sentence
  on its agent scraping ClickHouse.
- **`x402-disk-root` seen in the channel once**, agreed with stellar-x402: the explorer relies on
  it for ClickHouse's disk.
- **The series budget:** read the VM's head series
  (`ssh monitoring 'curl -s localhost:9090/api/v1/status/tsdb'`) and size ClickHouse's allowlist
  against the CPX12's ~8,000 series.

### Step 2: The agent

- **A `metrics-agent` service** in `docker-compose.prod.yml`: Prometheus in agent mode, image
  pinned by digest, `read_only: true`, `cap_drop: [ALL]`, no published port,
  `network_mode: "service:clickhouse"`.
- **Its config** (e.g. `infra-hetzner/metrics/prometheus-agent.yml`):
  - `external_labels` `project: explorer` and `host: ch-prod-01`;
  - jobs `agent` (`localhost:9090`) and `clickhouse` (`localhost:9363`, with
    `metric_relabel_configs` keeping only the allowlist);
  - `remote_write` to `https://metrics.sorobanscan.com/api/v1/write` with the client certificate;
  - applied on change: the rsync notifies nothing and `recreate: auto` ignores bind-mounted files,
    so the `app` role records the config's checksum and a handler recreates the agent, as
    `Record Caddyfile checksum` does for Caddy.
- **The client certificate** (CN `explorer-metrics-agent-production`):
  - the key and its CSR are made on ch-prod-01 outside `/srv/app`, whose sync deletes and re-owns
    files (e.g. `/etc/soroban-metrics/`); the key is owned by 65534 with mode 0400, since Compose
    file secrets keep the host's owner;
  - the CA holder signs the CSR with `openssl x509 -req`, as soroban-infrastructure's
    `monitoring/README.md` ("The AWS identity") does; `infra-hetzner/ca/issue-client-cert.sh`
    makes its own key and takes no CSR;
  - the certificate is committed outside the git-ignored `ca/out/` (e.g. `infra-hetzner/metrics/`),
    and the role checks it matches the key, as x402's `check-agent-certificate.sh` does;
  - the CN is added to `metrics_writer_cns` in soroban-infrastructure.
- **Lifecycle** (ADR 0062, "Its lifecycle"):
  - every documented restart or recreate of ClickHouse alone names the agent and the data-check
    exporter too: `infra-hetzner/README.md`, `docs/runbooks/backfill_derived_table_reparse_hetzner.md`
    and the `app` role's "a restart is enough to pick up an XML edit" comment;
  - the runbook step for a stranded agent, in `/srv/app`, with 0682's exporter named too:
    `docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --no-deps --force-recreate metrics-agent`.

### Step 3: Rules, CI and pins

- **`infra-hetzner/metrics/rules/explorer-production.yml`.** Every rule carries `project`,
  `check`, `severity` and `shape`, a description with its runbook, and ADR 0062's comments (its
  re-arm answer for a level rule, what covers its silence). Every selector has
  `project="explorer"`. The first rules:
  - `explorer-agent-silent`: `absent_over_time(up{project="explorer",job="agent"}[5m])`, its
    description naming both causes (the agent or its namespace, or the whole host);
  - `explorer-agent-behind`, as `x402-agent-behind`, with a `promtool test rules` scenario;
  - `explorer-target-down`: `up{project="explorer"} == 0` for 5 minutes, as `x402-target-down`.
    A target that fails its scrape leaves every series it serves stale, so the per-check rules
    see nothing;
  - a first ClickHouse rule on the allowlist, e.g. failed queries through `increase()`.
- **CI:** `promtool check rules` and `promtool test rules` in `ci.yml`, as stellar-x402 runs them.
- **soroban-infrastructure:**
  - `explorer_rules_commit` / `explorer_rules_sha256`, with the download and `promtool` tasks
    beside x402's; every later rule change is re-pinned there before its drill;
  - a Grafana folder with a first ClickHouse dashboard (`explorer_dashboard_commit` /
    `explorer_dashboard_sha256`).

### Step 4: Drills (ADR 0062, rule 5)

- The VM lists the explorer's group at the pin (`/api/v1/rules`).
- `explorer-agent-silent`: stop the agent, see the alert in the channel, start the agent, see it
  resolve.
- The first ClickHouse rule: run a failing query (e.g. `SELECT throwIf(1)`) until it fires, and
  see it resolve once its `increase()` window passes.
- In a quiet window, `docker restart app-clickhouse-1`: check whether the agent is stranded, and
  record the answer in the runbook.
- `explorer-agent-behind` passes its `promtool` scenario; `explorer-target-down` is 0683's drill.

## Acceptance Criteria

- [ ] Step 1's agreements reached, stellar-x402's ADR 0011 sentence changed, and `x402-disk-root`
      seen in the channel once.
- [ ] The agent sends from ch-prod-01, its series carry `project="explorer"`, only ClickHouse's
      allowlist arrives, and a changed config reaches the running agent on the next deploy.
- [ ] The rules file is checked in CI and loaded on the VM at a pin, and Grafana's explorer folder
      shows the dashboard.
- [ ] Every Step 4 drill done and recorded in this task.
- [ ] The certificate committed and its CN on `metrics_writer_cns`. Its expiry rule is 0683's
      `explorer-agent-certificate`; this task closes once that rule has fired.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md` (the agent,
      the VM path) and `infra-hetzner/README.md` (deploy and recovery), linking ADR 0062.
- [ ] **API types regenerated** — N/A — nothing under `crates/api/**`, `Cargo.{toml,lock}` or
      `libs/api-types/**` changes.
