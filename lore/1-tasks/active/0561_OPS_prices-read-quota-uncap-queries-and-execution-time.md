---
id: '0561'
title: 'prices_read quota: drop the hourly queries / execution_time caps that blacked out prices-api on 2026-09-03'
type: OPS
status: active
related_adr: []
related_tasks: ['0314', '0243', '0338', '0250', '0477']
tags:
  [
    'clickhouse',
    'prices-api',
    'rbac',
    'incident',
    'effort-small',
    'priority-high',
  ]
links: []
history:
  - date: '2026-09-17'
    status: active
    who: stkrolikiewicz
    note: >
      Created from the prices-api side's incident forensics (their task 0260,
      "Cause named — 2026-09-17"): the 2026-09-03 read-path collapse was the
      `prices_read` quota, 10,000 queries/h, refusing 28,853 requests with
      CH code 201 until the clock hour rolled over. Karol informed on Slack
      the same day. Plan: PR here, then an in-place file overwrite on the box
      (0477 precedent), verified in system.quota_limits.
---

# prices_read quota: drop the hourly queries / execution_time caps

## Summary

Set `queries` and `execution_time` in the `prices_read` quota
(`crates/db-clickhouse/users.d/quotas.xml`) to 0 (unlimited). Keep
`read_rows` / `read_bytes` / `result_rows` as they are — they are the
resource guards that stop the prices tenant from exhausting the shared box.
Do **not** move `prices_reader` to the `unlimited` quota.

## Context

On 2026-09-03 a prices-api load test (100 req/s of cache misses, 5 min) took
the whole prices-api read path down for 26 minutes. Confirmed on 2026-09-17
from `system.query_log` on the box: ClickHouse logged exactly **28,853**
code-201 refusals — the same count as the gateway 5XX —

```
Quota for user `prices_reader` for 3600s has been exceeded:
queries = 10001/10000. Interval will end at 2026-09-03 07:00:00.
```

first at 06:34:10, last at 06:57:54, recovery at 07:00 when the
non-randomized hourly interval rolled over. The database was not loaded:
median 7–8 ms at 5.5 k queries/min against `max_connections` 4096 and
`max_concurrent_queries` 1000.

The 10,000 admitted queries measured what one costs (9 ms, 17.5 k rows,
~1.5 MiB), which puts the other caps of the same quota at:

| cap              | value / h | trips at about            |
| ---------------- | --------- | ------------------------- |
| `queries`        | 10,000    | 10 k — tripped 2026-09-03 |
| `execution_time` | 1000 s    | ~110 k queries            |
| `read_bytes`     | 1 TiB     | ~700 k queries            |
| `read_rows`      | 50 B      | ~2.8 M queries            |

The remaining M3 runs send 30 k / 150 k / 300 k misses (100 / 500 / 1000
req/s × 5 min), so raising `queries` alone moves the wall onto
`execution_time`. Both go; the byte/row guards stay and still leave ~2×
margin for the largest run.

Precedent in this repo: `api_reader` was refused with code 201 on seven days
in June (10,360 refusals) and was moved to `unlimited` on 2026-07-01
(c51ba735, task 0338). `api_throttle` applies to nobody today; `prices_read`
was a verbatim copy of it ("mirrors api_throttle", task 0314).

Full evidence, raw readings and the per-cap arithmetic live in
stellar-prices-api: `lore/1-tasks/active/0260_RESEARCH_read-path-collapse-connections-or-queries/`
(README "Cause named — 2026-09-17" + `notes/R-clickhouse-box-readings-2026-09-17.md`).

Observation, not this task's: those code-201 rows are quotas being enforced
on the Caddy `X-ClickHouse-User` path, which task 0250 and the "Known
limitations" section of `clickhouse-rbac.md` record as not enforced. Raised
with Karol separately; neither is touched here.

## Implementation Plan

### Step 1: repo change (PR)

- `quotas.xml`, `<prices_read>` only: `queries` 10000→0, `execution_time`
  1000→0; `read_rows`, `read_bytes`, `result_rows`, `errors` unchanged; the
  "mirrors api_throttle" comment replaced by the reason (2026-09-03 incident,
  prices task 0260). Per-query `max_execution_time` stays enforced by the
  `read_only` profile.
- `docs/architecture/security/clickhouse-rbac.md`: `prices_read` no longer
  "copied verbatim from api_throttle"; the `api_throttle` line gets the
  numbers the XML actually has (50 B / 1 TiB, not 1 B / 100 GB).

### Step 2: deploy — in-place overwrite, no ansible, no restart

Same mechanism as 0477: the file is bind-mounted per file, so a truncate +
write keeps the inode and ClickHouse hot-reloads `users.d`. NOT `--tags app`
(re-renders `.env` from a possibly stale local `soroban-prod.env` under
`no_log`, and would recreate the whole stack).

1. Confirm the box still matches `origin/develop` before the merge
   (`quotas.xml` and `services.xml` were identical on 2026-09-17):
   `ssh sorban-prod 'cat /srv/app/crates/db-clickhouse/users.d/quotas.xml' | diff - crates/db-clickhouse/users.d/quotas.xml`
2. After merge, from the merged checkout:
   `ssh sorban-prod 'cat > /srv/app/crates/db-clickhouse/users.d/quotas.xml' < crates/db-clickhouse/users.d/quotas.xml`
3. Verify (~10 s later), via `docker exec app-clickhouse-1 clickhouse-client`:
   `SELECT max_queries, max_execution_time, max_read_rows FROM system.quota_limits WHERE quota_name = 'prices_read'`
   → `max_queries` and `max_execution_time` NULL, `max_read_rows` 50000000000.
   If the old values persist: `SYSTEM RELOAD CONFIG`. A container restart
   only in a window agreed with Karol — it should not be needed.
4. Record the outcome in prices task 0260.

## Acceptance Criteria

- [ ] `prices_read`: `queries` and `execution_time` unlimited;
      `read_rows` / `read_bytes` / `result_rows` unchanged
- [ ] The XML comment and the `clickhouse-rbac.md` description tell the truth
      about this quota
- [ ] After deploy: `system.quota_limits` shows NULL for `max_queries` and
      `max_execution_time`, without a CH restart
- [ ] Every other user and quota unchanged (the XML diff touches only the
      `prices_read` block)
- [ ] Deploy outcome recorded in stellar-prices-api task 0260
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`
      (quotas section); other architecture docs N/A (no shape change).
- [ ] **API types regenerated** — N/A (no `crates/api` change).

## Notes

- Only ~2.8 misses/s sustained fit under the old cap; the worst organic hour
  in the 14 days before 2026-09-17 was 909 queries. The cap guarded nothing
  except against the load tests it was meant to survive.
- Deploying the merged file leaves repo and box byte-identical, so the next
  `--tags app` rsyncs nothing and the inode stays put (0477 lesson).
