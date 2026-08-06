---
prefix: S
title: Which of the sixteen children this umbrella actually retires
status: mature
---

# S — Child triage

The umbrella claims "sixteen tasks, two defects". Reading all sixteen: eight are
genuine instances, three form a separate cost cluster, five do not belong and
should be re-scoped out rather than left implying coverage they will not get.

All sixteen were still `backlog` at the time of this triage.

## Genuine instances (8) — the umbrella retires or reshapes these

| Task     | Defect | Note                                                                                                                                                         |
| -------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [[0454]] | 1 + 2  | The anchor. Alarm filter vs emitted strings, and lag emitted only on success                                                                                 |
| [[0400]] | 1      | `init.sql` vs prod schema. Carries a second comparison: architecture docs still describe a retired Postgres world                                            |
| [[0312]] | 1      | CDK vs deployed. **The comparator already exists** — `make diff-production` reports it. Nothing schedules it                                                 |
| [[0406]] | 1      | **Closed 2026-08-06** — CI rust job provisions CH from compose + init.sql sidecar, runs both gates, red-verified by sabotage. First real Actions run pending |
| [[0434]] | 1      | Protocol tables vs the chain. Different mechanism — the reference is an external crate, not a repo file                                                      |
| [[0428]] | 2      | Refreshable MV reports rows from metadata; a failed refresh is indistinguishable from success                                                                |
| [[0237]] | 2      | Reboot flag: the box knows, nothing asks. Different delivery path — not on CloudWatch                                                                        |
| [[0382]] | 1      | Verify-range vs Horizon. **The strongest comparator in the set** and the umbrella never says so: our index is the declaration, the chain is the actual       |

## Cost cluster (3) — defect 3, related but its own thread

| Task     | Note                                                                                        |
| -------- | ------------------------------------------------------------------------------------------- |
| [[0449]] | Cost attribution. Tags already emitted by all 11 stacks; only Billing activation is missing |
| [[0447]] | MV rewriting its whole target every 2 min. Cause of the volume, not an observability defect |
| [[0448]] | Research on insert-body compression. Cost/perf research; no observability content           |

## Re-scope out (5) — related in spirit, not instances

| Task     | Why it does not fit                                                                                                                                                    |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [[0232]] | Not a comparison problem. The live writer structurally cannot hold the invariant; the task is choosing a mitigation per column. Detection is an addition, not the task |
| [[0250]] | Needs a probe that deliberately exceeds a quota, not a comparator. Research/decision task                                                                              |
| [[0403]] | "Shipped without verifying" — adjacent shape, but the fix is a one-off validation run                                                                                  |
| [[0087]] | Frontend telemetry. The umbrella's own Notes already exclude it; it should not be in `related_tasks`                                                                   |
| [[0127]] | A report deliverable, not a defect                                                                                                                                     |

## What the triage changes

1. The headline "sixteen instances" is inflated. Eight is the honest number, and
   eight is still a strong argument for one comparator.
2. [[0382]] belongs in the defect-1 table and is currently missing from it.
3. [[0312]] proves the comparator is not the hard part — `cdk diff` already
   answers it. The hard part is that nothing runs on a schedule and nothing reads
   the output. That is the actual deliverable.
4. Nothing in the set covers **cost detection**. [[0449]] proposes AWS Budgets and
   Cost Anomaly Detection in prose, but no task owns it, so the "we did not notice
   the spend rise for three weeks" problem has no owner. Either fold it into
   [[0449]]'s acceptance criteria or spawn it.
