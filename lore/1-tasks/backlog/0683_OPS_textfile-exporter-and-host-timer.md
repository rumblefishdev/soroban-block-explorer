---
id: '0683'
title: 'OPS: textfile exporter and host timer for host facts on ch-prod-01'
type: OPS
status: backlog
related_adr: ['0062']
related_tasks: ['0681', '0237']
tags:
  [
    priority-medium,
    effort-medium,
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
      Created from ADR 0062 (PR #762). Builds the mechanism for host facts and the agent
      certificate's days left; 0237's reboot flag is the next fact. Starts once 0681's agent and
      certificate exist.
---

# OPS: textfile exporter and host timer for host facts on ch-prod-01

## Summary

ADR 0062 reads host facts as textfiles. A host timer writes them, a node_exporter of the
explorer's own with only the textfile collector serves them, and the explorer's agent (0681)
scrapes it. This task builds that, with the first fact the foundation needs, the agent
certificate's days left, and the rules every textfile gets. 0237's reboot flag is the next fact.

## Stan teraz

- Done: nothing yet.
- Next: after 0681's agent and certificate exist (its Step 2).
- In force: ADR 0062, decision 2 ("What it scrapes", "Every textfile", "The host").

## Context

- **x402's host probe is the model.** `stellar-x402/deploy/libexec/x402-host-probe` writes its
  textfiles through a temp file and a rename, writes -1 days when it can't read the certificate,
  and `x402-textfile-stale` watches every file's age (stellar-x402 ADR 0011, "Host metrics"). The
  collector keeps serving a file nobody writes.
- **The host's disk and memory stay x402's** (its node_exporter, `x402-disk-root`,
  `x402-memory`). This exporter serves only the explorer's files, and x402's series of the same
  names differ only by `project`.

## Implementation Plan

### Step 1: The textfile exporter

- `node_exporter` in `docker-compose.prod.yml`: image pinned by digest, `read_only: true`,
  `cap_drop: [ALL]`, no published port.
- On the Compose network, not in ClickHouse's namespace: the agent reaches it by service name on
  port 9100, and a ClickHouse restart doesn't strand it.
- Flags: `--collector.disable-defaults`, `--collector.textfile` and
  `--collector.textfile.directory=/textfile`.
- The host directory (e.g. `/var/lib/explorer-metrics`) is mounted read-only. The agent's
  `textfile` job scrapes it.

### Step 2: The host timer

- A systemd service and timer from the Ansible roles (`app`, or a new `metrics` role), every
  minute, writing each `.prom` file through a temp file and a rename.
- The first fact, in `certificate.prom`: `certificate_days_left{certificate="metrics-agent"}`,
  from 0681's committed certificate. When it can't read the certificate, the timer writes -1, so
  the expiry rule fires instead of the series vanishing from a fresh file.

### Step 3: Rules

In the explorer's rules file (0681), every selector with `project="explorer"`, re-pinned in
soroban-infrastructure before the drills:

- **`explorer-textfile-stale`** fires on any of:

  - a file older than 360 s (the one-minute period plus five minutes, ADR 0062):
    `time() - node_textfile_mtime_seconds{project="explorer"} > 360`;
  - `node_textfile_scrape_error{project="explorer"} > 0`;
  - an expected file absent while the exporter is up, one expression per file. The `file` label
    is the path inside the container, matched as `x402-textfile-stale` matches it:

    ```promql
    absent(node_textfile_mtime_seconds{project="explorer",file=~"(.*/)?certificate\\.prom"})
      and on() up{project="explorer",job="textfile"} == 1
    ```

- **`explorer-agent-certificate`:**
  `certificate_days_left{project="explorer",certificate="metrics-agent"} < 30`.
- The exporter itself being down is 0681's `explorer-target-down`.

### Step 4: Drills

- Stop the timer: `explorer-textfile-stale` fires; start it: the alert resolves.
- Stop the exporter's container: 0681's `explorer-target-down` fires; start it: it resolves.
- ADR 0062's rule-5 drill series: a drill file with
  `certificate_days_left{certificate="metrics-agent",network="drill"} 5`, and the same `# HELP`
  and `# TYPE` lines as the timer's file, fires `explorer-agent-certificate`; removing it resolves
  the alert. Remove it within five minutes, or its own age fires `explorer-textfile-stale`.

## Acceptance Criteria

- [ ] The exporter serves only the explorer's textfiles, and the agent scrapes it.
- [ ] The timer writes the certificate's days left every minute, atomically, and -1 when it can't
      read the certificate.
- [ ] `explorer-textfile-stale`, `explorer-target-down` and `explorer-agent-certificate` have each
      fired once into the channel and resolved, the last from the drill series.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md` (the
      exporter and the timer) and `infra-hetzner/README.md`, linking ADR 0062.
- [ ] **API types regenerated** — N/A — nothing under `crates/api/**`, `Cargo.{toml,lock}` or
      `libs/api-types/**` changes.
