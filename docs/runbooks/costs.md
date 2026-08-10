# Costs — how to answer "whose is it" and "why did it spike"

The account (`750702271865`) is shared by two projects —
`soroban-block-explorer` and `stellar-prices-api` — and both tag every
resource with `Project`. The `Project` cost-allocation tag is **active** in
Billing (activated 2026-08 from the organization management account, with a
historical backfill requested), so cost attribution is a filter, not an
investigation.

## "How much does each project cost?"

Cost Explorer → date range → **Group by: Tag → Project**. Daily granularity
shows the per-project split; the July-2026 investigation that took a day of
manual correlation is now this one view.

## "What is the untagged remainder?"

Group by **Service** with filter **Tag: Project → No tag**. Known standing
contents (measured 2026-08-10, ~0.6 USD/day after the Galexie fix below):

- **Inherently untaggable** (AWS limitation, permanent): public IPv4 hours,
  X-Ray traces, most CloudWatch metric/alarm charges. Small and stable — a
  step change HERE is still caught by anomaly detection (below).
- **Fixable leftovers**: hand-provisioned secrets (mTLS bundles, CA key,
  operator env, deploy SSH key) carry no tags — tag them once with
  `aws secretsmanager tag-resource` when touched; CDK-created secrets are
  tagged automatically.

Historical caveats: Galexie Fargate tasks are tagged only from the
`propagateTags` deploy (2026-08); billing data before tag backfill coverage
stays partially unattributed.

## "Something spiked — where does the alert come from?"

**Cost Anomaly Detection** (native, free) runs a per-SERVICE monitor over
the whole account — both projects, tagged or not. It learns each service's
daily baseline and, when one service's unexplained excess accumulates past
`costAnomalyAlertThresholdUsd` (see `infra/envs/production.json`), publishes
to the alarm SNS topic → AWS Chatbot → the team Slack channel. The alert
names the **service** and the impact in USD; attribute it to a project with
the Group-by-Project view above.

Mechanics worth knowing when reading an alert:

- The threshold gates **one anomaly's cumulative excess** (per service), not
  a daily or monthly total — two services 2 USD over baseline each are two
  separate below-threshold events.
- The counter closes when the service returns to its baseline; the next
  deviation starts a new one.
- Cost data refreshes a few times a day, so detection latency is hours —
  that is a property of AWS billing itself, not of the monitor.

Defined in `infra/src/lib/stacks/cloudwatch-stack.ts` (monitor +
subscription + the `costalerts.amazonaws.com` topic-policy grant).

## What this does NOT cover

- **The Hetzner ClickHouse box** — invoiced outside AWS, fixed monthly
  amount, no variance to alarm on.
- **The organization management account's own spend** — an org-wide monitor
  could only live there; this account-level monitor covers 100% of the
  account both projects actually run in.
- **Slow creep** (a few percent per week never looks like a spike) — that is
  the job of per-project AWS Budgets with forecast alerts, planned once tag
  propagation has produced a week of honestly-attributed data (task 0449).
