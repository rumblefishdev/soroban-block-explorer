---
id: '0449'
title: 'OPS: cost allocation tags — the AWS account bills two projects as one'
type: OPS
status: backlog
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
- [ ] A per-tag budget or anomaly monitor exists and routes to the Slack topic
      already used by the production alarms
- [ ] The alert is verified by a deliberate test, not assumed to work

## Not in scope

Reducing any cost. This task makes cost attributable and makes a change in it
noticeable; 0447 and the volume findings handed to the other project's team are
where the money actually is.
