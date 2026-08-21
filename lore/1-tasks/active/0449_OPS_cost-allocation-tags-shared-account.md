---
id: '0449'
title: 'OPS: cost allocation tags — the AWS account bills two projects as one'
type: OPS
status: active
related_adr: []
related_tasks: ['0447']
tags: [phase-future, effort-small, priority-medium, ops, cost, aws]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from the July cost investigation, which took several hours mostly
      because no cost figure in the account can be attributed to a project
      without reverse-engineering it from CloudWatch and the database.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Tag inventory measured on our side: all stacks emit
      Project=soroban-block-explorer, Environment=production, ManagedBy=cdk.
      The co-tenant project's tag status is unverified - the read-only
      tagging-API call is outside the assistant's allowlist, needs an
      operator run of resourcegroupstaggingapi get-resources. Activation of
      the Project cost-allocation tag in Billing is risk-free (reporting
      metadata only, touches no resources) and non-retroactive, so worth
      doing immediately even before the co-tenant aligns; untagged resources
      land in the "no tag key" bucket.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Two blockers dissolved by measurement (read-only). (1) The co-tenant
      ALREADY tags consistently: sampled prices-* Lambdas all carry
      Project=stellar-prices-api with the same Environment/ManagedBy keys as
      ours - the "agree the key with its owners" step is de facto done.
      (2) The empty Cost-allocation-tags page mystery: account 750702271865
      is a MEMBER of AWS Organization o-gj0pr49dpf; the management (payer)
      account is 045028348791 (invoices@rumblefish.pl), and cost allocation
      tags can only be activated THERE. Action re-routed: ask the management
      account owner to activate the Project tag (Billing -> Cost allocation
      tags -> User-defined -> Project -> Activate). Activation is
      non-retroactive - every day unactivated is a day of history that stays
      unattributable (the July investigation cost a full day for exactly
      this reason).
  - date: 2026-08-10
    status: backlog
    who: karolkow
    note: >
      Big day for this task. (1) Tag ACTIVATED by the management-account
      owner, verified from CE: Cost Explorer groups by Project with real
      values; a historical backfill was reportedly requested too (mgmt-side
      feature; applies tags resources carried at usage time, so Galexie
      tasks stay unattributed historically). (2) Untagged bucket broken
      down: ~84% of account spend, of which ~74% is Galexie Fargate - ECS
      does not propagate service tags to tasks; FIXED in CDK
      (propagateTags: SERVICE, commit 78bcf735), forward-only. (3) Cost
      Anomaly Detection added (commit 29dbf9a1): per-SERVICE monitor over
      the whole account + IMMEDIATE subscription onto the alarm topic +
      costalerts.amazonaws.com topic-policy grant; threshold in config,
      set to 3 USD cumulative-per-anomaly after discussion. Measured
      before adding: the account had ZERO anomaly monitors and ZERO
      budgets. (4) Residual untagged after the fix ~0.6 USD/day:
      inherently untaggable (public IPv4, X-Ray, CW metrics) plus
      hand-provisioned secrets (8 ours: mtls/*, ca/key, operator/env,
      ops/deploy-ssh-key; 2 prices') - one-time tag-resource commands for
      the operator. (5) The "RDS" line in the untagged bucket is OURS,
      not the co-tenant's: two manual snapshots of the retired staging
      Postgres in us-east-1 (20+40 GB, Apr/May 2026) billing
      ChargedBackupUsage - staging retired by 0249, PG by 0243/0244;
      deletion commands handed to the operator (irreversible, data
      worthless by construction). (6) Runbook docs/runbooks/costs.md
      added: per-project view, untagged-remainder view, how to read an
      anomaly alert, stated non-coverage (Hetzner invoice, management
      account, slow creep until Budgets land). Remaining here: per-project
      Budgets with forecast alerts after ~a week of honestly-attributed
      data, and the second anomaly subscription for the co-tenant's
      channel if they want one.
  - date: 2026-08-10
    status: backlog
    who: karolkow
    note: >
      Operator pass executed and verified the same day. (1) Both retired
      staging-Postgres snapshots in us-east-1 deleted - describe-db-snapshots
      now returns zero; the RDS billing line should hit 0 within ~2 days.
      Safe by written policy: 0249 retained them as 30-day last-resort
      insurance, expired since May/June, restore-share lists empty. (2) All
      8 hand-provisioned secrets tagged
      (Project/Environment/ManagedBy=manual), verified 13/15 secrets in the
      account now carry Project - the remaining 2 are the co-tenant's
      (prices/production/clickhouse-mtls-*), theirs to tag. Gotcha for the
      runbook: the operator's admin session defaults to us-east-1, so
      Secrets Manager calls need an explicit --region eu-central-1 (the
      first tagging pass bounced off ResourceNotFound harmlessly). Durable
      fix still owed: the 0227 provisioning path should tag at creation so
      future hand-made secrets do not reopen the gap.
  - date: 2026-08-10
    status: backlog
    who: karolkow
    note: >
      Decision: per-project AWS Budgets are DROPPED, not deferred. What is
      already in place suffices - attribution is one Cost Explorer view,
      step changes are covered by account-wide Cost Anomaly Detection, and
      a second alerting mechanism for two people is exactly the
      per-component-notifier growth ADR 0054 exists to stop. The
      slow-creep gap Budgets would have covered is accepted and named in
      docs/runbooks/costs.md. This closes the last open scope item; the
      task archives once the anomaly-detection deploy is verified.
  - date: 2026-08-19
    status: active
    who: karolkow
    note: >
      Moved backlog -> active. The status contradicted production: the
      detection half of this task (account-wide Cost Anomaly Detection, its
      SNS route and the topic policy grant) has been deployed and live since
      the 2026-08-18 release, while the file still claimed the task had not
      started. All seven acceptance criteria remain unticked and genuinely
      open — none were verified — so this is a status correction, not a
      completion. Found by the 0455 review sweep (finding 36).
---

# The AWS account bills two projects as one

## Summary

The AWS account hosts this project and a second one. Cost Explorer can group by
service and usage type but not by project, so no answer to "what does the
explorer cost" is available without inference. Cost allocation tags fix that.

## Context

In July the account's Lambda line went from roughly $200 to $600. Establishing
what caused it required, in order:

1. Cost Explorer by usage type — showed the increase was **data transfer out**
   ($5.08 → $346.28 for 28 days), not compute
2. CloudWatch per-function invocation and duration counts for June against July —
   showed nine functions belonging to the other project with **no June data at
   all**, and our own indexer growing only 29 % / 31 %
3. `system.part_log` on ClickHouse, split by database and by day — showed the
   daily insert curve of the other project matching the daily egress curve
   almost gigabyte for gigabyte, while our own step change on 13 July moved the
   egress not at all

Only step 3 settled it. That is three separate data sources and a database query
to answer a question a tag would have answered in one call.

Two wrong conclusions were reached and corrected along the way: first that the
egress was negligible, then that all of it was ours. Both would have been
avoided by tagging.

## Implementation

- Tag every billable resource with a project key. CDK supports stack-level tags
  (`Tags.of(app).add(...)`), so this is close to a one-liner per app — but the
  second project's resources need the same treatment, so agree the key and the
  values with its owners first.
- Activate the tag as a **cost allocation tag** in Billing. Note that activation
  is not retroactive: data starts accruing from activation, so earlier months
  stay unattributable.
- Confirm coverage: anything untagged lands in a "no tag key" bucket, and a
  partially-tagged account is only marginally better than an untagged one.
- Record in `docs/deployment.md` that new stacks must carry the tag.

## Detection, not just attribution

Attribution answers "whose cost is this". It does not answer "why did nobody
notice for three weeks". The July increase ran from 6 July and was found on
28 July, by hand, because someone looked at a bill.

There is no cost alarm on the account, and the CloudWatch dashboard covers
ingestion lag, Lambda latency, API errors and queue depth — nothing about spend.

Once the tag exists, per-project detection becomes possible:

- **AWS Budgets** with an actual-vs-forecast alert per project tag, wired to the
  existing SNS topic that already feeds Slack (`stacks/observability-stack.ts`).
  A monthly budget with an 80 % / 100 % / forecast trigger would have fired in
  the first week of July.
- **Cost Anomaly Detection** with a monitor per tag value. This catches the shape
  the July increase actually had — a step change against a stable baseline —
  which a fixed threshold catches late or not at all.
- A spend widget on the existing production dashboard, if only so the number is
  in a place people already look.

Worth stating plainly: the useful signal here was **usage quantity**, not dollars.
Egress went 88.68 GB → 3,880.66 GB, a 43x step, while the dollar figure stayed
small enough in absolute terms to look unremarkable on a shared account. Alarm on
the quantity as well as the amount.

## Acceptance Criteria

- [ ] Tag key and values agreed with the other project's owners
- [ ] All CDK stacks in this repo emit the tag
- [ ] Tag activated for cost allocation in Billing
- [ ] `ce get-cost-and-usage --group-by TAG` returns a per-project split with no
      material spend in the untagged bucket
- [ ] `docs/deployment.md` states the requirement for new stacks
- [ ] An anomaly monitor exists and routes to the Slack topic (per-tag
      Budgets dropped 2026-08-10 — see history; anomaly detection committed,
      checks off after deploy)
      already used by the production alarms
- [ ] The alert is verified by a deliberate test, not assumed to work

## Not in scope

Reducing any cost. This task makes cost attributable and makes a change in it
noticeable; 0447 and the volume findings handed to the other project's team are
where the money actually is.
