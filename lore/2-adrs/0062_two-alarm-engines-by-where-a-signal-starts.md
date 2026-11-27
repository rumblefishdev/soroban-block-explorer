---
id: '0062'
title: "Two alarm engines by where a signal starts, with ADR 0054's rules for both"
status: proposed
deciders: [karolkow, stkrolikiewicz]
related_tasks:
  [
    '0237',
    '0476',
    '0428',
    '0488',
    '0447',
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
  - lore/1-tasks/archive/0428_OPS_alert-on-accounts-recent-mv-refresh-failure.md
  - infra/src/lib/stacks/cloudwatch-stack.ts
  - infra/src/lib/stacks/ingestion-alarms.ts
  - crates/db-clickhouse/config.d/prometheus.xml
history:
  - date: 2026-11-26
    status: proposed
    who: stkrolikiewicz
    note: >
      Drafted to supersede ADR 0054 (`0054_one-alarm-engine-and-three-rules-for-alarms.md`) now
      that a monitoring VM runs outside ch-prod-01 (stellar-x402 ADR 0011, task 0139). The first
      draft added an explorer metrics agent beside ClickHouse, a data-check exporter and a
      textfile exporter (tasks 0681–0683, PR #763).
  - date: 2026-11-27
    status: proposed
    who: stkrolikiewicz
    note: >
      Slimmed after Karol's review on PR #762. The demand behind the machinery was 0237 and
      0476: 0476 stays closed by 0428's decision, with a ready CloudWatch design, and ClickHouse's
      disk is already alarmed (`prices-production-ch-disk-free`). Decision 2 (the explorer's own
      agent) is replaced by "nothing new runs for the explorer", 0681–0683 are dropped, and rule 1
      gains an exception for facts that must page when the VM can't. Checked on ch-prod-01: the
      ClickHouse metrics endpoint listens on `0.0.0.0:9363`, not the container's loopback.
---

# ADR 0062: Two alarm engines by where a signal starts, with ADR 0054's rules for both

**Related:**

- [ADR 0054: one alarm engine and three rules](./0054_one-alarm-engine-and-three-rules-for-alarms.md)
  (not `0054_concurrent-clickhouse-reads.md`, which shares its id): its rule 1 is replaced; its
  rules 2–5 are restated below and hold for both engines
- [Task 0428](../1-tasks/archive/0428_OPS_alert-on-accounts-recent-mv-refresh-failure.md): the
  refresh alarm closed by decision, with its return conditions and its design
- [Task 0237: reboot-required alert](../1-tasks/backlog/0237_FEATURE_reboot-required-notification.md),
  the one host fact this ADR places now
- [Task 0476: alert on a stalled `balance_aggregates_mv` refresh](../1-tasks/backlog/0476_OPS_balance-aggregates-refresh-alert.md),
  under 0428's decision
- [Task 0488](../1-tasks/backlog/0488_OPS_backfill-must-not-starve-production.md), whose step 4
  plans a disk and a lag alarm
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

What changed between 2026-11-17 and 2026-11-25 (stellar-x402 ADR 0011, task 0139):

- **A monitoring VM**, `monitoring-hel1-01`, a Hetzner Cloud CPX12 in Helsinki, in another
  location than ch-prod-01 in Falkenstein. It runs Prometheus, Alertmanager and Grafana for every
  project on ch-prod-01, provisioned from soroban-infrastructure `monitoring/`. Each project keeps
  its rules in its own repository; the VM downloads them at a pinned commit and checks them with
  `promtool`. Today only x402's are pinned.
- **One Prometheus agent on ch-prod-01, x402's**, the VM's only writer. It scrapes the
  facilitator, the host's node_exporter (with the textfiles x402's host probe writes) and itself.
  x402 owns the host's metrics: `x402-disk-root` (80%) and `x402-memory`. ADR 0011 planned that
  one agent to scrape ClickHouse too; its config doesn't.
- **A witness for the VM.** An always-firing Watchdog goes to an SNS topic every minute, and a
  CloudWatch alarm fires when it stops. A failed Slack send is posted through SNS, a failed SNS
  send through Slack. Both reach x402's channel.
- **One Slack webhook**, to x402's alarm channel `#stellar-x402-bot`. The explorer's CloudWatch
  alarms post to its own channels, one per environment.

What the explorer was waiting on:

- **0237:** `/var/run/reboot-required` on ch-prod-01, a host fact. The task proposed its own Slack
  webhook and a cooldown file.
- **0476:** a stalled `balance_aggregates_mv` refresh. 0428 measured the same class on all 8
  refreshable views (0 exceptions over 29 days, a refresh in every hour of the view's life) and
  closed the alarm by decision, with two return conditions (a silent refresh failure observed,
  or 0447 raising `REFRESH EVERY`) and a design for its return: `now() - last_success_time`
  published from the indexer's existing CloudWatch call. Neither condition is met.
- **ClickHouse's disk:** already alarmed, as `prices-production-ch-disk-free` (`Prices/Rollup`,
  20% free) and by `x402-disk-root` on the same filesystem.
- **ClickHouse's own metrics** (merges, failed queries): no task needs them yet. The endpoint is
  on (`crates/db-clickhouse/config.d/prometheus.xml`) and nothing scrapes it. It listens on
  `0.0.0.0:9363` inside the container, so the Compose network reaches it: checked on ch-prod-01
  on 2026-11-27 (`ss` in the container's namespace, and `wget http://clickhouse:9363/metrics`
  from the Caddy container). The `<listen_host>` under `<prometheus>` had no effect; ClickHouse
  binds the port on the top-level `listen_host` list like every other port. It is not published
  on the host.

ADR 0054's revisit conditions for its Alternatives 1 and 1b are not met, and this ADR doesn't
claim them. It answers 0054's Negative consequences instead: signals that start outside AWS need
their own decision, and the stack 1b rejected now runs off the box and is already operated for
x402.

---

## Decision

**1. Each signal has one engine, chosen by where it starts.** This replaces ADR 0054's rule 1.

- **Components emit metrics, or a host probe writes a textfile; none notifies a human.** No other
  notifier (a webhook, an email relay, a cron that posts) is added for any signal.
- **AWS-side signals stay in CloudWatch**, as ADR 0054 decided: metrics an AWS resource in this
  account emits (SQS, Lambda, API Gateway, ECS), and what our Lambdas measure as part of their own
  work (`IngestionLagSeconds`, the `IndexerFailures` log filter, and 0428's
  `now() - last_success_time` when it returns). Delivery stays SNS → Chatbot (Amazon Q
  Developer) → Slack, and thresholds stay in CloudWatchStack.
- **Host facts and ClickHouse's own metrics go to Prometheus on the monitoring VM.** They are
  read on the host, by x402's agent, and no new Lambda is written to read one.
- **Exception: a fact that must page when the VM can't goes to CloudWatch.** A whole-Hetzner
  outage or a dead VM silences the VM's path, so a fact whose alarm has to survive that is read
  by a Lambda over mTLS, as stellar-prices-api's `rollup-freshness-probe` reads ClickHouse, with
  no AWS credential on the host. stellar-x402 made the same call for OpenBao's audit disk (its
  ADR 0011, "it must page when the VM can't"). The rule that places such a fact names why.
- **No signal goes through both**, except a fact that already has an alarm on each side for its
  own reason, such as the disk above.

**2. Nothing new runs for the explorer.** ADR 0011's single agent stays the only writer on the VM.

- **0237** becomes one more check in x402's host probe and one rule beside `x402-disk-root`: a
  pending reboot is a fact about the shared host, which x402's probe and rules already cover. The
  explorer's `security` role keeps what writes the flag (task 0227). To be agreed in stellar-x402
  (Open, below).
- **0476** stays under 0428's decision. When a return condition is met, it is built as 0428
  designed it, on the CloudWatch path.
- **ClickHouse's own metrics** wait for a task that needs them. That task makes x402's agent scrape
  `clickhouse:9363` over the Compose network, not through ClickHouse's network namespace, with:
  - `project="explorer"` set by the job's relabelling, so the explorer's alerts don't read as
    x402's;
  - an allowlist of metric names and a `sample_limit` on the job, since a CPX12 holds about
    8,000 series (stellar-x402 ADR 0011) and the explorer's series must not crowd out x402's;
  - the rules in this repository, checked with `promtool` in CI, pinned in soroban-infrastructure,
    with the order of the three repositories' changes and who runs the playbook named in that
    task;
  - a CI check that every explorer rule's selectors carry `project="explorer"`: the ingest admits
    a writer by its certificate's CN, so labels are trusted, not checked;
  - rules on ClickHouse's `events` and `errors` counters with `increase()`, since a restart resets
    them.
- **The channel.** Until an explorer rule exists on the VM, there's nothing to route. If one
  arrives with an explorer route of its own, that route gives up what x402's channel gets: the
  Watchdog's alarm, the failed-send rule and `x402-disk-root`. It also has to sit before the
  shared `shape` and `severity` routes, which have no `continue`, and so loses their no-resolved
  and hourly behaviour unless it repeats them.

**3. ADR 0054's rules 2–5 hold for both engines.** Restated here, so this ADR stands on its own,
with what each means on the VM:

- **Rule 2: alarm on change, not on level, wherever a condition can persist.** A standing
  condition belongs on a dashboard, an event on the pager. Carve-out: a level alarm is correct
  where policy forces the steady state to zero (a DLQ, 5xx), because it is drained and re-armed.
  In Alertmanager an alert instance is its label set, so a new cause posts while another fires. A
  standing alert repeats at the shared route's interval, a `warning` every 24 hours and a
  `critical` every hour (stellar-x402 task 0139, decisions 1 and 2). That is a deliberate change,
  for the VM's path, to ADR 0054's rule 2, under which "a standing condition never re-pages";
  CloudWatch's path keeps it as it was.
- **Rule 3: absence is breaching only where nothing else witnesses the same absence.** Paging
  twice for one fault is how alarms get muted. On the VM, x402's `server-silent` and
  `textfile-stale` witness its agent and its host probe, so a rule placed there by decision 2
  inherits them and adds no absence rule of its own.
- **Rule 4: a page caused by planned work the operator just performed is cheap; suppression
  logic is expensive. Don't build it.** One knowing page per planned pause is the accepted cost.
  Revisit only if knowing pages become genuinely noisy, with a count, not a feeling.
- **Rule 5: the delivery path is verified on every change to it, before the change is called
  done.** A message that arrives counts, not a diff that looks right.
  - CloudWatch's path, as ADR 0054 had it: every deploy of the alarm stack is done only when one
    message has travelled CloudWatch → SNS → Chatbot → Slack and been seen in the channel.
  - The VM's path: a new or changed rule is done when the VM lists it at the new pin
    (`/api/v1/rules`) and it has fired once into the channel from a real or a drill series.
  - A change to `alertmanager.yml` is soroban-infrastructure's, verified there after every
    playbook run.
- **In every rule:** the labels the shared Alertmanager routes and groups on (`project`, `check`,
  `severity`, with `critical` only for what would page on mainnet, and `shape`); a description
  written for whoever it wakes; for a level rule, a comment with its re-arm answer; and a comment
  naming what covers its silence.

---

## Rationale

- The demand was two signals. 0237 is one host fact on a host whose metrics x402 already reads;
  0476 is closed by a measurement, with a design for its return that needs no new component.
  ClickHouse's disk is alarmed twice already. A second agent, a data-check exporter, a textfile
  exporter, a ClickHouse user, a client certificate and their drills (roughly 45–70 hours across
  the first draft's 0681–0683) would serve none of them better.
- Rule 1 still settles where the next signal goes, which is what ADR 0054 left open. AWS-side
  signals gain nothing by moving: Prometheus can't scrape a Lambda, and pulling the explorer's
  ~25 alarm metrics into the VM would cost $2–11 a month and add `cloudwatch_exporter`'s 600 s
  default delay to a 120 s backlog threshold (`ingestion-alarms.ts`).
- The exception keeps rule 1 from forbidding what stellar-x402 already did for a good reason.

---

## Alternatives Considered

### Alternative 1: Keep ADR 0054

**Description:** ADR 0054's pointer for 0237, a healthchecks.io-style ping from a host timer, and
CloudWatch for the rest.

**Cons:**

- A third delivery path and another vendor for one host fact the VM can already read.
- Leaves host facts and ClickHouse's metrics without a ruling, as 0054 itself says.

**Decision:** REJECTED.

### Alternative 2: The explorer's own agent beside ClickHouse (this ADR's first draft)

**Description:** A Prometheus agent in ClickHouse's network namespace as a second writer on the
VM, a data-check exporter with a read-only ClickHouse user, and a textfile-only node_exporter with
a host timer (tasks 0681–0683).

**Cons:**

- The namespace bought no privacy: the endpoint listens on `0.0.0.0` and the Compose network
  already reaches it. It did cost: every restart of `clickhouse` strands the agent and the
  exporter until they are recreated, right when they matter.
- Its first host fact was its own certificate, and its first data check a signal 0428 closed.
- A second writer, CN, key, WAL, rules pin and route to run for two signals.

**Decision:** REJECTED. Revisit when the explorer has enough host or ClickHouse signals that
x402's agent and host probe can't carry them.

### Alternative 3: Move every explorer alarm to Prometheus

**Description:** Read SQS, Lambda and API Gateway metrics into Prometheus with a CloudWatch
exporter on the VM, or have the Lambdas push their metrics.

**Cons:**

- `GetMetricData` on every scrape, an AWS credential on the VM beyond its SNS role, $2–11 a month,
  and a delay that undoes the backlog threshold.
- A Lambda push needs a client certificate per function; stellar-x402 ADR 0011 leaves "metrics
  from the Lambdas" for later.

**Decision:** REJECTED for now. Revisit when one rule needs an AWS metric and a ClickHouse one
together.

### Alternative 4: Prometheus on ch-prod-01

**Cons:** It would stop with every fault, full disk and reboot of the box it watches (ADR 0054's
Alternative 1b, stellar-x402 ADR 0011).

**Decision:** REJECTED.

### Alternative 5: ClickHouse as the sink

**Cons:** Observability inside the system it observes goes quiet exactly when it is needed (ADR
0054's Alternative 2).

**Decision:** REJECTED.

---

## Consequences

### Positive

- 0237 gets an engine for the cost of one probe check and one rule, with no webhook and no
  cooldown file.
- The next host or ClickHouse signal has a ruling and a path that already runs.
- Nothing new to deploy, secure or drill on ch-prod-01 for the explorer.

### Negative

- Two engines: a reviewer first asks where a signal starts, and an alarm is defined in one of two
  places.
- A standing alert on the VM reminds every 24 hours or every hour; one in CloudWatch never does.
- The explorer's host alarms are x402's rules in x402's channel, so the explorer's operators read
  `#stellar-x402-bot` for them.
- The explorer depends on a VM and an agent this repository doesn't run.
- CloudWatch's path still has no external witness, as in ADR 0054.

---

## Open / Pending (gates proposed → accepted)

- [ ] 0237 agreed in stellar-x402: the probe check, the rule beside `x402-disk-root`, and ADR
      0011's sentence on its agent scraping ClickHouse left as it is.
- [ ] 0237's rule fired once into the channel from a touched `/var/run/reboot-required` (rule 5).
- [ ] 0488 step 4 reread under rule 1: its disk alarm already exists twice; its lag alarm is a
      Lambda measure, so CloudWatch.
- [ ] On acceptance, ADR 0054 (`0054_one-alarm-engine-and-three-rules-for-alarms.md`) marked
      `superseded` with `by: '0062'`, and the "In force" lines of 0671–0674 repointed to 0062.

---

## Delivery Checklist (per ADR 0032)

- [ ] `docs/architecture/technical-design-general-overview.md`: N/A — no change to the system's
      design.
- [ ] `docs/architecture/database-schema/database-schema-overview.md`: N/A — no schema change.
- [ ] `docs/architecture/backend/backend-overview.md`: N/A — the API is unchanged.
- [ ] `docs/architecture/frontend/frontend-overview.md`: N/A — the frontend is unchanged.
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`: N/A — its alarms stay
      in CloudWatch.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md`: pending — which engine
      watches what, and this ADR linked at the monitoring section.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md`: N/A — unrelated.

---

## References

- stellar-x402 `docs/adr/0011-facilitator-hosting.md`, "Logs, metrics and alerts" and
  "Alternatives considered"; its host probe `deploy/libexec/x402-host-probe` and rules
  `deploy/metrics/rules/x402-production.yml`
- soroban-infrastructure `monitoring/README.md` and `monitoring/ansible/roles/monitoring/files/alertmanager.yml`
- stellar-prices-api `packages/rollup-freshness-probe/` (`mv_drift.rs`, `refresh_waits.rs`)
- Karol's review on PR #762 (2026-11-26) and the reply (2026-11-27)
- [ADR 0054](./0054_one-alarm-engine-and-three-rules-for-alarms.md)
