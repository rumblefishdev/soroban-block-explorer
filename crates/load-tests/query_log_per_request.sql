-- Per-REQUEST ClickHouse cost for a load-test window (task 0338).
--
-- One row per HTTP request: `log_comment` is the harness `X-Request-Id`, so this
-- JOINs to client.csv on `request_id`. Emits CSV to STDOUT (no INTO OUTFILE), so
-- it can be streamed straight to the local machine over SSH — see README.md
-- ("Pull the per-request query_log to your laptop").
--
-- ch_queries > 1 for endpoints that fan out (list = data + count), so read_rows
-- is summed across all CH queries the one HTTP request issued.
SELECT
    log_comment            AS request_id,
    sum(read_rows)         AS read_rows,
    sum(read_bytes)        AS read_bytes,
    count()                AS ch_queries,
    max(query_duration_ms) AS ch_duration_ms,
    max(memory_usage)      AS memory_max
FROM system.query_log
WHERE type = 'QueryFinish'
  AND log_comment != ''
  AND log_comment NOT LIKE 'harvest-%'
  AND event_time BETWEEN {start:DateTime} AND {end:DateTime}
GROUP BY log_comment
FORMAT CSVWithNames;
