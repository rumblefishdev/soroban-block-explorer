# API 5xx — every error gets accounted for

The `production-api-gateway-5xx` alarm pages when **any** API request
returned a 5xx in a 5-minute window. There is no threshold to reason
about: the measured base rate is zero (24 consecutive clean days at the
time the alarm was rewritten, 2026-08-10), so a single 5xx is an event.

**The rule this runbook exists for: every 5xx is a defect, and every one
gets an owner.** The alarm only says _something_ happened; this
procedure guarantees each error is seen and attributed, without anyone
re-deriving the method mid-incident.

## 1. Pull the errors

CloudWatch Logs Insights, log group `/aws/lambda/production-soroban-explorer-api`,
time range = around the alarm window (the log group keeps 30 days):

```
filter level = "ERROR"
| stats count(*) as cnt by fields.message, fields.error
| sort cnt desc
```

The API logs variables as structured fields (`fields.error`,
`fields.pool_id`, `fields.account_id`, `fields.tx_hash`, …), so group or
filter by whichever entity the messages point at.

ClickHouse error codes worth recognizing on sight:

| Code | Meaning         | Past instance (2026-07)                             |
| ---- | --------------- | --------------------------------------------------- |
| 60   | UNKNOWN_TABLE   | query ran mid-DDL — coordinate schema changes       |
| 241  | MEMORY_LIMIT    | too-heavy read (`list_pools` FINAL era)             |
| 48   | NOT_IMPLEMENTED | unsupported query construct — deterministic per URL |

## 2. Check for gateway-only errors

Gateway `5XXError` also counts 502/504 the Lambda log never sees
(timeouts, integration failures). If the metric count exceeds what step
1 found, compare with Lambda `Errors`/`Duration` for the same window.
There is no access logging on the stage (deliberate — add it only when
a silent-504 investigation actually needs it).

## 3. Account for every error

Each distinct error class from step 1 ends in exactly one of:

- **an existing task** — add a dated note there,
- **a new task** — spawn it,
- **a known one-off** (deploy window, upstream blip) — say so where the
  alarm was discussed, with the evidence.

No class gets dismissed as noise: if the alarm pages regularly, the fix
is repairing the 5xx class it points at — never widening the alarm
(policy comment lives next to the alarm in
`infra/src/lib/stacks/cloudwatch-stack.ts`).

## Standing visibility

The dashboard carries the 5xx sum widget — that is the "how many this
week" view; the pager is only for "it just happened".
