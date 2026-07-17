# AC4 load-test raw results

Raw measurement data behind the AC4 section of `../milestone-3-evidence.md`.

Produced by the project's own load-test harness
([`crates/load-tests`](../../../crates/load-tests), built under task 0338) run
against the **production** API on 2026-07-17. Each tier is one open-model run at
a fixed request rate; the rate is chosen to represent a monthly request volume
(e.g. 0.48 req/s ≈ 1.2M req/month).

## Tiers

| Directory              | Run (UTC)              |     Rate | Requests |    p50 |    p95 | Errors |
| ---------------------- | ---------------------- | -------: | -------: | -----: | -----: | -----: |
| `ac4-1.2M-per-month/`  | `2026-07-17T11-08-22Z` |  0.48 /s |      168 | 168 ms | 577 ms |      0 |
| `8x-10.2M-per-month/`  | `2026-07-17T11-47-41Z` |  3.95 /s |    2,363 | 149 ms | 567 ms |      0 |
| `40x-49.3M-per-month/` | `2026-07-17T11-33-11Z` | 19.04 /s |   13,701 | 151 ms | 575 ms |      0 |

`ac4-1.2M-per-month` is the tier that corresponds to the acceptance criterion
(1M requests/month equivalent). The other two are capacity/stress tiers at 8×
and 40× that volume. All three ran the same 26 endpoints; every request returned
HTTP 200 (`err_class = ok`), hence the 0.000 % error rate.

## Files per tier

| File                 | Contents                                                                                                                                                                        |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `results.csv`        | One row per request. Client-side timing joined to the ClickHouse `system.query_log` for that exact request (the "B2" correlation, matched on `request_id`) — see columns below. |
| `client_summary.csv` | Per-endpoint aggregate: `requests, errors, err_rate_pct, p50_ms, p90_ms, p95_ms, p99_ms, max_ms`.                                                                               |

`results.csv` columns:

```
ts_ms, round, vu, request_id, endpoint, method, http_status, err_class,
duration_ms, ttfb_ms, read_rows, read_bytes, ch_queries, ch_duration_ms,
memory_max, url
```

`duration_ms` is the client-observed latency; `read_rows` / `read_bytes` /
`ch_queries` / `ch_duration_ms` come from the ClickHouse query log for the same
request. This pairing is what lets the evidence attribute latency to database
work versus fixed overhead versus external fetches.

The p50 / p95 figures quoted in `milestone-3-evidence.md` are percentiles of
`duration_ms` across all requests in a tier's `results.csv`.

## Notes

- The harness also emits `client.csv` and `query_log_per_request.csv` per run.
  Both are omitted here: `results.csv` is their join and a strict superset of
  the columns in each.
- Runs from the same session that are **not** part of the evidence table
  (`2026-07-17T11-05-42Z`, `2026-07-17T11-14-45Z`) are intentionally not
  included — they are warm-up / discarded passes.
