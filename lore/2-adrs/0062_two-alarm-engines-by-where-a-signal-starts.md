---
id: '0062'
title: "Two alarm engines by where a signal starts, with ADR 0054's rules for both"
status: proposed
deciders: [karolkow, stkrolikiewicz]
related_tasks:
  [
    '0681',
    '0682',
    '0683',
    '0237',
    '0476',
    '0455',
    '0671',
    '0672',
    '0673',
    '0674',
  ]
related_adrs: ['0054', '0032']
tags: [observability, ops, infrastructure, policy, prometheus]
links:
  - lore/2-adrs/0054_one-alarm-engine-and-three-rules-for-alarms.md
  - infra/src/lib/stacks/cloudwatch-stack.ts
  - infra/src/lib/stacks/ingestion-alarms.ts
  - crates/db-clickhouse/config.d/prometheus.xml
  - crates/db-clickhouse/users.d/services.xml
  - docker-compose.prod.yml
history:
  - date: 2026-11-26
    status: proposed
    who: stkrolikiewicz
    note: >
      Drafted to supersede ADR 0054 (`0054_one-alarm-engine-and-three-rules-for-alarms.md`) now
      that a monitoring VM runs outside ch-prod-01 (stellar-x402 ADR 0011, task 0139). A
      proposal before implementation, as ADR 0054 was before its gates. Tasks 0681–0683
      implement it once it merges as proposed, and their drills tick the gates below before it is
      accepted. Draft for Karol's review.
---

# ADR 0062: Two alarm engines by where a signal starts, with ADR 0054's rules for both

**Related:**

- [ADR 0054: one alarm engine and three rules](./0054_one-alarm-engine-and-three-rules-for-alarms.md)
  (not `0054_concurrent-clickhouse-reads.md`, which shares its id): its rule 1 is replaced; its
  rules 2–5 are restated below and hold for both engines
- The implementation, in three tasks:
  [0681: the metrics agent beside ClickHouse](../1-tasks/backlog/0681_OPS_metrics-agent-beside-clickhouse.md),
  [0682: the data-check exporter](../1-tasks/backlog/0682_OPS_data-check-exporter-beside-clickhouse.md),
  [0683: the textfile exporter and host timer](../1-tasks/backlog/0683_OPS_textfile-exporter-and-host-timer.md)
- [Task 0237: reboot-required alert](../1-tasks/backlog/0237_FEATURE_reboot-required-notification.md)
- [Task 0476: alert on a stalled `balance_aggregates_mv` refresh](../1-tasks/backlog/0476_OPS_balance-aggregates-refresh-alert.md)
- Tasks 0671–0674: CloudWatch-side work under ADR 0054's rules; their "In force" lines point here
  once this ADR is accepted
- stellar-x402 ADR 0011 ("Logs, metrics and alerts") and task 0139; soroban-infrastructure
  `monitoring/README.md`

---

## Context

ADR 0054 made CloudWatch the alarm engine for AWS-side signals and left the database host
without one, on purpose. A CloudWatch path would have put an AWS credential on the host to carry
one boolean, and Prometheus on the host itself was rejected as "partially inside the system it
observes" (its Alternative 1b). Its Negative consequences say so: "Signals that originate outside
AWS have no ruling here, so each still needs its own decision."

Since stellar-x402 ADR 0011 the host has an AWS identity, but it is x402's: its CloudWatch agent,
through IAM Roles Anywhere, whose role writes only x402's logs and metrics, and which ADR 0011
names the server's only AWS credential. It reads nothing of ClickHouse and turns no host fact into
an explorer alarm. So these signals are still waiting:

- **0237:** `/var/run/reboot-required` on ch-prod-01, a host fact. The task still proposes its own
  Slack webhook and a cooldown file.
- **0476:** a stalled `balance_aggregates_mv` refresh, a ClickHouse fact. The check exists only
  as a manual runbook query.
- **ClickHouse itself:** merges, failed queries, refreshes. Its Prometheus endpoint is on
  (`crates/db-clickhouse/config.d/prometheus.xml`, mounted by `docker-compose.prod.yml`), bound
  to the container's loopback for "a future Prometheus scraper". Nothing scrapes it.

What changed between 2026-11-17 and 2026-11-25 (stellar-x402 ADR 0011, task 0139):

- **A monitoring VM**, `monitoring-hel1-01`, a Hetzner Cloud CPX12 in Helsinki, in another
  location than ch-prod-01 in Falkenstein. It runs Prometheus (90 days, capped at 20 GB),
  Alertmanager and Grafana for every project on ch-prod-01, provisioned from
  soroban-infrastructure `monitoring/`. Each project keeps its rules and dashboards in its own
  public repository; the VM downloads each file at a pinned commit, refuses it unless its SHA-256
  matches, and checks the rules with its own `promtool`. Today only x402's are pinned.
- **One Prometheus agent on ch-prod-01, x402's**, today the VM's only writer (its CN
  `x402-metrics-agent-production` is the one entry in `metrics_writer_cns`). It scrapes the
  facilitator, the host's node_exporter and itself, and sends with `remote_write` over mTLS, with
  a client certificate from this project's CA. It evaluates nothing, and its write-ahead log
  holds a few hours of samples while the VM can't be reached. ADR 0011 planned that one agent to
  scrape ClickHouse too; its config doesn't.
- **A witness for the VM.** An always-firing Watchdog goes to an SNS topic every minute, and a
  CloudWatch alarm with missing data breaching fires when it stops. That is a dead-man's switch
  for the VM, judged by CloudWatch. It is not the one ADR 0054 named, a service outside the
  CloudWatch → Slack chain that would also witness CloudWatch; that one is still not adopted. A
  failed Slack send is posted through SNS, a failed SNS send through Slack.
- **One Slack webhook.** Alertmanager posts through one incoming webhook, to x402's alarm channel
  `#stellar-x402-bot`. The explorer's CloudWatch alarms post to its own channels, one per
  environment.
- **Grafana** shows dashboards only, its alerting off, behind Cloudflare Access for every
  @rumblefish.pl address.
- **x402 moved its metric alarms there** on 2026-11-25 and drilled them the same day, except
  `agent-behind`, `disk-root`, `memory` and `agent-certificate`, which weren't fired live (task
  0139). `x402-watch`, its uptime probe and the heartbeat stay in CloudWatch.

ADR 0054's revisit conditions for its Alternatives 1 and 1b are not met, and this ADR doesn't
claim them: two people are on call, and the retention 0054 meant was Lambda logs, which stay in
CloudWatch. This ADR answers 0054's Negative consequences instead: signals that start outside AWS
need their own decision, and the stack 1b rejected now runs off the box and is already operated
for x402.

---

## Decision

**1. Each signal has one engine, chosen by where it starts.** This replaces ADR 0054's rule 1.

- **Components emit metrics, or a host timer writes a textfile; none notifies a human.** No other
  notifier (a webhook, an email relay, a cron that posts) is added for any signal. 0237's webhook
  and cooldown file become a textfile and a rule.
- **AWS-side signals stay in CloudWatch**, as ADR 0054 decided: metrics an AWS resource in this
  account emits (SQS, Lambda, API Gateway, ECS), and what our Lambdas already measure as part of
  their own work (`IngestionLagSeconds`, the `IndexerFailures` log filter). Delivery stays SNS →
  Chatbot (Amazon Q Developer) → Slack, and thresholds stay in CloudWatchStack
  (`cloudwatch-stack.ts`, `ingestion-alarms.ts`).
- **ClickHouse and host facts go to Prometheus on the monitoring VM.** They are read on the host:
  host facts as textfiles (0237), ClickHouse's own metrics, and data checks, SQL against
  ClickHouse whose answer is a number (0476). No new Lambda is written to read one. The rules live
  in this repository and are checked with `promtool` in CI.
- **No signal goes through both.**

**2. The explorer runs its own agent next to ClickHouse, a second writer on the VM**, where
stellar-x402 ADR 0011 planned one agent for the whole server.

- **Where it runs.** In `docker-compose.prod.yml`, with ClickHouse's network namespace
  (`network_mode: "service:clickhouse"`), the shape `prometheus.xml` reserved, so ClickHouse's
  endpoint stays on the container's loopback.
- **Its lifecycle.** Compose resolves `service:clickhouse` to that container's ID when it creates
  the agent, so when `clickhouse` is recreated or restarted (a deploy, a crash, an OOM kill,
  `docker restart`), the agent and the exporter are left without network and send nothing until
  they are recreated too. A `docker compose up -d` of the whole project recreates them in the same
  run, since the new ID changes their configuration. `depends_on` doesn't, and its `restart: true`
  acts only on Compose operations. A command that names only `clickhouse`, such as
  `up -d --force-recreate clickhouse`, leaves them behind, so that procedure names them too. After
  any other restart the agent's silence rule (below) pages, and its runbook step recreates the
  agent and the exporter.
- **Its reach.** Anything in that namespace reaches ClickHouse's loopback ports, where the
  `<no_password/>` users of `users.d/services.xml` accept it, `dev_shared` (admin) among them. The
  exporter's read-only user guards against our own query mistakes, not a compromised sidecar. So
  the agent, the exporter and the textfile exporter run digest-pinned images with
  `read_only: true`, `cap_drop: [ALL]` and no published port, as x402's agent does.
- **What it scrapes:**
  - itself (`job="agent"`, `localhost:9090`), for its `remote_write` backlog and failures;
  - an allowlist of ClickHouse's metric names, not all four families;
  - a data-check exporter in the same namespace;
  - a node_exporter of its own with only the textfile collector on
    (`--collector.disable-defaults --collector.textfile --collector.textfile.directory=<dir>`),
    reading a directory a host timer writes: 0237's flag and the agent certificate's days left.
    It sits on the Compose network, outside ClickHouse's namespace, so a ClickHouse restart
    doesn't strand it.
- **The exporter.** Its queries, its series names and the rules that read them live in this
  repository and change in one commit; it is built from this repository or runs a digest-pinned
  image (the implementation task chooses). It reads with a read-only ClickHouse user granted only
  what its checks read, the `system.*` tables included, under a statement timeout. The queries
  reach ch-prod-01 with the explorer's deploy and the rules reach the VM with soroban-infrastructure's
  pin, so each check also has a rule on its series being absent while the exporter is up: a
  series renamed at one end pages instead of going quiet.
- **Every check exports:**
  - its value; one it can't compute (`last_success_time` NULL with no `exception`, after a
    ClickHouse restart) is no value, never 0, while a non-empty `exception` is a failing value;
  - the time it last succeeded, so a check that can't run stops advancing it and a rule fires on
    its age;
  - how many of the objects it checks it can see. A read the grants filter returns no rows, not
    an error, and would read as healthy; a rule fires when the count is 0. stellar-prices-api's
    probe, on the CloudWatch path, has `MvDriftUnreadable` and `MvRefreshUnreadable` for this
    reason.
- **Every textfile** has a rule on its age, as x402's `textfile-stale` does:
  `time() - node_textfile_mtime_seconds{project="explorer"}` beyond the timer's period plus five
  minutes, a scrape error, or the file absent while the node_exporter is up. The collector keeps
  serving a file nobody writes.
- **Rules on ClickHouse's `events` and `errors` counters use `increase()`**, since a restart
  resets them.
- **Labels.** Every series carries `project="explorer"`. A data check's series also carries
  `network`: `stellar:pubnet` for `default.*`, `stellar:testnet` for `testnet.*`, since both
  databases are in this ClickHouse and Alertmanager groups by `network`. A testnet check is a
  `warning` at most.
- **Its own witness.** The explorer's rules watch its agent: `absent_over_time(up{project="explorer"}[5m])`
  for the agent and its namespace, and a rule on its `remote_write` falling behind, as x402's
  `agent-behind` does, tested with `promtool test rules`, since its series reach the VM only
  through the sends it measures. x402's silence rule watches x402's agent only.
- **Its certificate** has a CN of its own on the VM's `metrics_writer_cns`, and a rule fires 30
  days before it expires, on the days-left textfile the host timer writes, as x402's host probe
  does.
- **The host.** x402 keeps the host's metrics: its node_exporter, and its rules on the root
  filesystem (`x402-disk-root`, 80%) and on memory (`x402-memory`); there is no CPU rule. The
  host's base setup stays with this repository's `infra-hetzner`, the timer that writes the
  explorer's textfiles included. ClickHouse's data (`/srv/clickhouse-data`) is on the root
  filesystem, so `x402-disk-root` covers it; the explorer relies on that rule and doesn't alarm on
  the disk twice. Neither project's rules read the other's series or files.

**3. ADR 0054's rules 2–5 hold for both engines.** Restated here, so this ADR stands on its own,
with what each means on the VM:

- **Rule 2: alarm on change, not on level, wherever a condition can persist.** A standing
  condition belongs on a dashboard, an event on the pager. Carve-out: a level alarm is correct
  where policy forces the steady state to zero (a DLQ, 5xx), because it is drained and re-armed.
  In Alertmanager an alert instance is its label set, so a new cause (a new `check`, a new table)
  posts while another fires. A standing alert repeats at the shared route's interval, a `warning`
  every 24 hours and a `critical` every hour (stellar-x402 task 0139, decisions 1 and 2). That is
  a deliberate change, for the VM's path, to ADR 0054's rule 2, which "is not repeated reminders"
  and under which "a standing condition never re-pages"; CloudWatch's path keeps it as it was.
- **Rule 3: absence is breaching only where nothing else witnesses the same absence.** Paging
  twice for one fault is how alarms get muted. The explorer agent's silence rule witnesses the
  agent, its namespace and its network, so a per-signal absence rule is written only for what it
  can't see: one exporter or check that stops while the agent runs. Two absences are knowingly
  witnessed twice: a whole-host or network outage posts x402's `server-silent` and the explorer's
  agent rule, one per project, since neither reads the other's series; and a stopped `clickhouse`
  container posts CloudWatch's indexer alarms and the agent rule, since the agent shares its
  namespace. The agent rule's description names both causes. An inhibition across projects would
  make the explorer's witness depend on x402's rule again, so none is added. Any inhibition goes in
  soroban-infrastructure's shared `alertmanager.yml`, scoped by `project="explorer"` matchers and
  reviewed there.
- **Rule 4: a page caused by planned work the operator just performed is cheap; suppression
  logic is expensive. Don't build it.** One knowing page per planned pause is the accepted cost.
  Prefer the bare signal; revisit only if knowing pages become genuinely noisy, with a count, not
  a feeling. An Alertmanager silence expires on its own, so ADR 0054's objection to disabling
  actions (a forgotten re-enable leaves a mute alarm) doesn't apply to it, but no procedure depends
  on one.
- **Rule 5: the delivery path is verified on every change to it, before the change is called
  done.** A message that arrives counts, not a diff that looks right.
  - CloudWatch's path, as ADR 0054 had it: every deploy of the alarm stack, including one that
    doesn't appear to touch delivery, is done only when one message has travelled CloudWatch → SNS
    → Chatbot → Slack and been seen in the channel.
  - The VM's path, for a change to the explorer's rules: done when the VM lists the explorer's
    group at the new pin (`/api/v1/rules`) and each new or changed rule has fired once into the
    channel from a real or a drill series (`network="drill"`, as x402's 0139 drill did).
    `amtool alert add` tests only Alertmanager onward.
  - A change to `alertmanager.yml` (routes, receivers, inhibitions) is soroban-infrastructure's,
    verified there after every playbook run with a test alert carrying the route's matchers (its
    README). The shared routes match `severity` and `shape`, not `project`.
  - Between changes, the Watchdog and the cross-posting failed-notification rule watch the VM's
    path.
- **In every rule:** the labels the shared Alertmanager routes and groups on, `project="explorer"`,
  `check`, `severity` (`critical` only for what would page on mainnet) and `shape` (`event` for a
  one-off, which posts no resolved; `level` or `server` otherwise); a description written for
  whoever it wakes (what is happening, where the runbook is); for a level rule, a comment with its
  re-arm answer; and a comment naming what covers its silence.

---

## Rationale

- 0237 is a host file no Lambda can read, and ClickHouse's own endpoint is on the container's
  loopback. What they lack is a reader on the host, rules and a witness. The VM has the rules and
  a witness; the agent is the reader.
- A CloudWatch path for host facts would need a second AWS credential on the server, the
  explorer's, beside x402's, and still no scraper for ClickHouse.
- ADR 0054's objections to Prometheus were where it runs (on the database box), who runs it (a
  third system for two people), and Alertmanager as "yet another alarm state machine to configure
  and keep honest". The VM is elsewhere, has a witness of its own, and is already run for x402. Its
  Alertmanager configuration is shared and reviewed in soroban-infrastructure, and rule 2 above
  records where its state machine differs from CloudWatch's.
- AWS-side signals gain nothing by moving. Prometheus can't scrape a Lambda, and reading
  CloudWatch from the VM would add a hop, a credential and a per-metric charge to signals
  CloudWatch already evaluates.

---

## Alternatives Considered

### Alternative 1: Keep ADR 0054

**Description:** ADR 0054's pointer for 0237, a healthchecks.io-style ping from a host timer;
CloudWatch for the rest; and task 0476's own option, a scheduled Lambda that reads ClickHouse
over mTLS.

**Pros:**

- No new system to run: a ping service and one more check in a scheduled Lambda.
- stellar-prices-api already runs such a Lambda (`rollup-freshness-probe`: one schedule, many
  checks, `system.view_refreshes` among them), so 0476 alone could follow it.

**Cons:**

- A third delivery path and another vendor for 0237.
- Each ClickHouse fact is code in a Lambda plus a custom metric and an alarm ($0.30 and $0.10 a
  month, stellar-x402 ADR 0011).
- ClickHouse's own metrics stay unread: the endpoint is on the container's loopback.

**Decision:** REJECTED. The VM gives the host and ClickHouse an engine, with a witness in place.

### Alternative 2: Move every explorer alarm to Prometheus

**Description:** Read SQS, Lambda and API Gateway metrics into Prometheus with a CloudWatch
exporter on the VM, or have the Lambdas push their metrics.

**Pros:**

- One engine; one rule could join an AWS series and a ClickHouse one.

**Cons:**

- `GetMetricData` on every scrape, and an AWS credential on the VM beyond its SNS role.
- A second hop for signals CloudWatch evaluates natively.
- A Lambda push needs a client certificate per function; stellar-x402 ADR 0011 leaves "metrics
  from the Lambdas" for later.

**Decision:** REJECTED for now. Revisit when one rule needs an AWS metric and a ClickHouse one
together.

### Alternative 3: Prometheus on ch-prod-01

**Description:** Prometheus and Alertmanager in this repository's compose on ch-prod-01.

**Cons:**

- It would stop with every fault, full disk and reboot of the box it watches, and the metrics from
  the outage would be lost (ADR 0054's Alternative 1b, stellar-x402 ADR 0011).

**Decision:** REJECTED.

### Alternative 4: Grafana Cloud

**Description:** Grafana Cloud Free for metrics and alerts.

**Cons:**

- 10,000 series, 14 days, 3 people a month (stellar-x402 ADR 0011), and a second stack beside the
  VM.

**Decision:** REJECTED.

### Alternative 5: ClickHouse as the sink

**Description:** Check results and alerts stored in ClickHouse.

**Cons:**

- Observability inside the system it observes goes quiet exactly when it is needed (ADR 0054's
  Alternative 2).

**Decision:** REJECTED. Data checks read ClickHouse; their results and alerts live off it.

---

## Consequences

### Positive

- 0237 and 0476 get an engine: a textfile fact and a SQL check, each a rule in this repository.
- ClickHouse's own state is alarmed and graphed for the first time, beside the host and the
  facilitator in one Grafana.
- stellar-prices-api could later run its checks through an exporter of its own on this pattern,
  with `project="prices"` and its own user. That is its decision, not this ADR's.

### Negative

- Two engines: a reviewer first asks where a signal starts, and an alarm is defined in one of two
  places (CloudWatchStack, the rules file).
- A standing alert on the VM reminds every 24 hours or every hour; one in CloudWatch never does.
- Until the channel is settled (Open, below), the explorer's alarms arrive in two places: its
  CloudWatch channels and x402's.
- The agent and the exporter share ClickHouse's network namespace, so every restart or recreate of
  `clickhouse` must recreate them too, and an unplanned one pages until someone does.
- The explorer depends on x402's `x402-disk-root` for ClickHouse's disk, and that rule hasn't been
  fired live.
- The VM is a dependency this repository doesn't run: it is provisioned from
  soroban-infrastructure and operated with x402. Its size bounds what we scrape (a CPX12 holds
  about 8,000 series, stellar-x402 ADR 0011), hence the allowlist.
- CloudWatch's path still has no external witness, as in ADR 0054.
- What the explorer adds is components, not a stack, but several of them: an agent, a data-check
  exporter, a textfile-only node_exporter and the host timer that writes its files, a read-only
  ClickHouse user, a client certificate and key with an expiry rule, a rules file with `promtool`
  in CI, a dashboard, a deploy step that recreates the agent with `clickhouse`, and in
  soroban-infrastructure its CN, the pins and download tasks for its rules and dashboard, and its
  route.

---

## Open / Pending (gates proposed → accepted)

- [ ] (0681, Step 1, as are the next four) The channel agreed: the explorer's operators read `#stellar-x402-bot`, or an explorer route
      with its own webhook in soroban-infrastructure's `alertmanager.yml`.
- [ ] A second writer on the VM agreed with stellar-x402, and ADR 0011's sentence on its agent
      scraping ClickHouse changed to say the explorer's agent does.
- [ ] Rule 2's reminders accepted for the explorer, or an explorer route with a longer repeat.
- [ ] The VM's series count read (`/api/v1/status/tsdb`) before the allowlist is chosen.
- [ ] `x402-disk-root` seen in the channel once, with x402's agreement, since the explorer relies
      on it for ClickHouse's disk.
- [x] The implementation tasks created and linked from this ADR: 0681 (agent, rules, pins,
      dashboard), 0682 (data-check exporter, 0476 first), 0683 (textfile exporter and host timer,
      0237 next).
- [ ] The first data check (0476, in 0682) and the first host fact (the agent certificate's days
      left, in 0683) running and drilled, including a visible-object count of 0 and a stale
      textfile.
- [ ] Rule 5 demonstrated once on the VM's path with an explorer rule fired from a drill series
      (0683's certificate drill).
- [ ] On acceptance, ADR 0054 (`0054_one-alarm-engine-and-three-rules-for-alarms.md`) marked
      `superseded` with `by: '0062'`, and the "In force" lines of 0671–0674 repointed to 0062.

---

## Delivery Checklist (per ADR 0032)

- [ ] `docs/architecture/technical-design-general-overview.md`: N/A — no change to the system's
      overall design until the agent lands; updated with the implementation task if it names
      monitoring.
- [ ] `docs/architecture/database-schema/database-schema-overview.md`: N/A — no schema change;
      the exporter reads existing tables.
- [ ] `docs/architecture/backend/backend-overview.md`: N/A — the API is unchanged.
- [ ] `docs/architecture/frontend/frontend-overview.md`: N/A — the frontend is unchanged.
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`: N/A — its alarms stay
      in CloudWatch.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md`: pending — the agent, the
      exporters and the host timer on ch-prod-01 and the monitoring VM, with 0681–0683.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md`: N/A — unrelated.
- [ ] This ADR linked from `infrastructure-overview.md` at the monitoring section.

---

## References

- stellar-x402 `docs/adr/0011-facilitator-hosting.md`, "Keys, secrets and access", "Logs, metrics
  and alerts" and "Alternatives considered"
- stellar-x402 task 0139, the switch and its drills (`notes/G-switch-plan.md`); its rules in
  `deploy/metrics/rules/x402-production.yml` and its agent in `deploy/metrics/`
- soroban-infrastructure `monitoring/README.md` and `monitoring/ansible/roles/monitoring/files/alertmanager.yml`
- stellar-prices-api `packages/rollup-freshness-probe/` (`mv_drift.rs`, `refresh_waits.rs`)
- [ADR 0054](./0054_one-alarm-engine-and-three-rules-for-alarms.md)
