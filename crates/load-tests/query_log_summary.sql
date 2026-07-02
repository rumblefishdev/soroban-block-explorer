-- Per-endpoint ClickHouse cost over a load-test window (task 0338, Step 6).
--
-- The harness stamps every CH query's `log_comment` with `<endpoint-code>-<hex>`
-- (see harness X-Request-Id), so the endpoint code is the first '-' segment and
-- we can attribute read_rows / read_bytes per endpoint with NO client-side join.
--
-- Run on the ClickHouse host, scoped to your run's UTC window, e.g.:
--   docker compose exec -T clickhouse clickhouse-client \
--     --user=default --password=clickhouse \
--     --param_start='2026-06-30 12:00:00' --param_end='2026-06-30 13:00:00' \
--     --queries-file crates/load-tests/query_log_summary.sql
--
-- For a CSV to join with client_summary.csv, append on the host:
--   ... FORMAT CSVWithNames  > query_log_summary.csv     (or INTO OUTFILE '...')

SELECT
    splitByChar('-', log_comment)[1]         AS endpoint,
    count()                                  AS ch_queries,
    uniqExact(log_comment)                   AS http_requests,
    round(avg(read_rows))                    AS read_rows_avg,
    max(read_rows)                           AS read_rows_max,
    round(avg(read_bytes))                   AS read_bytes_avg,
    max(read_bytes)                          AS read_bytes_max,
    round(quantile(0.95)(query_duration_ms)) AS ch_p95_ms,
    max(memory_usage)                        AS memory_max,
    -- TOTAL rows the endpoint pulled over the window = the real quota pressure.
    -- Use sum() (= avg x ch_queries), NOT avg x http_requests: list endpoints
    -- fan out to ~2 CH queries/request (data + count), so an http-request-based
    -- proxy under-ranks them. This is the unambiguous "what to cache/index" key.
    sum(read_rows)                           AS read_rows_total
FROM system.query_log
WHERE type = 'QueryFinish'
  AND log_comment != ''
  AND log_comment NOT LIKE 'harvest-%'        -- drop setup() harvest requests
  AND event_time BETWEEN {start:DateTime} AND {end:DateTime}
GROUP BY endpoint
ORDER BY read_rows_total DESC
FORMAT PrettyCompact;

-- CAVEAT for the client⨝server join: `http_requests` (uniqExact log_comment)
-- counts only requests that issued >=1 CH query. CACHE HITS (e.g. netstats,
-- ctrdetail) issue NO CH query → absent here, though present in client.csv with
-- full volume. So client_summary.requests >= query_log.http_requests; the
-- difference = cache hits + non-CH paths. "Low read_rows" can mean "served from
-- cache", not "cheap" — read the two CSVs together with that in mind.

-- Per-request drill-down (full join key) — read_rows/bytes for one HTTP request:
--   SELECT log_comment AS request_id, sum(read_rows), sum(read_bytes),
--          max(query_duration_ms)
--   FROM system.query_log
--   WHERE type='QueryFinish' AND log_comment = '<paste request_id from client.csv>'
--   GROUP BY log_comment;
