# load-tests — Soroban Block Explorer API load harness (task 0338)

`--vus` concurrent users loop **every** endpoint with **no think-time** for
`--duration`. Each request carries a unique `X-Request-Id` (`<code>-<hex>`) so
the server stamps `system.query_log.log_comment` with it (B2 correlation): you
get **client-side latency/errors** AND **server-side read_rows/read_bytes** per
endpoint (and per request), without a fragile per-line log scrape — the harness
writes the CSVs directly.

This is a **diagnostic**, not a pass/fail gate. A 1000×1h run is *expected* to
shed load (202/429/5xx) once a backend ceiling is hit — the value is the **knee
point** and **which endpoints are hottest/most expensive**, which drives the
caching/indexing work.

## Build

```bash
cargo build --release -p load-tests
```

## Outputs (in `--out-dir`)

- `client.csv` — one row per request: `ts_ms,round,vu,request_id,endpoint,method,http_status,err_class,duration_ms,ttfb_ms,url`
- `client_summary.csv` — per-endpoint `requests,errors,err_rate_pct,p50,p90,p95,p99,max`

`err_class` splits `ok / 4xx / 5xx / 429 (throttle) / 403 (edge) / 401 (auth) / 504 (timeout) / conn`.

## Preconditions for a PROD run

1. **`loadTesting: true`** deployed (lifts WAF rate + GW throttle AND arms the
   `log_comment` middleware via `LOAD_TESTING` env) — `make -C infra deploy-production-apigateway` + `deploy-production-compute`.
2. **CH `api_reader` → `unlimited` quota** applied + container recreated.
3. **`API_KEY`** present in the server `API_KEYS` allowlist (paid-tier bypass of Turnstile/JWT).
4. **`ulimit -n 65535`** on the generator box (1000 VUs ≈ 1000 sockets).
5. Coordinate the window (SNS→Slack alarms will fire) and have the rollback ready.

## Run

Smoke first (low scale; confirm `log_comment` lands, harvest non-empty):

```bash
ulimit -n 65535
./target/release/load-tests \
  --base-url https://api-sorobanscan.rumblefishdev.com/v1 \
  --api-key "$API_KEY" \
  --vus 10 --duration 30s --out-dir .temp/load-tests/run-smoke
```

Baseline 1000×1h (note the UTC start/end — you need them for the query_log window):

```bash
date -u; ulimit -n 65535
./target/release/load-tests \
  --base-url https://api-sorobanscan.rumblefishdev.com/v1 \
  --api-key "$API_KEY" \
  --vus 1000 --duration 1h --out-dir .temp/load-tests/run-baseline
date -u
```

Local (against the local API + local ClickHouse, see `.temp/local-api-clickhouse-README.md`):

```bash
# local API must run with LOCAL_API=1 LOAD_TESTING=true and CH datasource envs
./target/release/load-tests \
  --base-url http://127.0.0.1:9100/lambda-url/api/v1 \
  --vus 20 --duration 30s --out-dir .temp/load-tests/run-local
```

Env vars work too (`BASE_URL`, `API_KEY`, `EDGE_SECRET`, `VUS`, `DURATION`, `HARVEST`, `OUT_DIR`).

## Collect the server side + analyse

On the ClickHouse host, scoped to the run window:

```bash
docker compose exec -T clickhouse clickhouse-client --user=default --password=clickhouse \
  --param_start='<UTC start>' --param_end='<UTC end>' \
  --queries-file crates/load-tests/query_log_summary.sql
```

Join `client_summary.csv` (client latency/errors) with `query_log_summary.csv`
(server read_rows/bytes) on the `endpoint` column → the ranked list of endpoints
to cache/index. Drill into a single request via `client.csv.request_id` =
`query_log.log_comment` (see the comment at the bottom of the `.sql`).

> **Caveat — the join is apples-to-oranges on request counts.** A cache hit
> (e.g. `netstats`, `ctrdetail`) issues **no** CH query, so it is **absent** from
> `query_log_summary.csv` while still counted in `client_summary.csv`. Hence
> `client_summary.requests ≥ query_log.http_requests`; the difference = cache
> hits + non-CH paths. A low `read_rows_total` can mean "served from cache", not
> "cheap" — that gap (high client volume, low CH cost) is itself a signal the
> endpoint is *already* well cached.

## Diagnostic loop (the actual goal)

```
1. baseline run → lawina 202/429/5xx + knee point; log_comment ⇒ hottest/priciest endpoints
2. fix the binding constraint: cache hot IMMUTABLE endpoints + skip-index the heavy filters
3. re-run → CH now sees only cache-misses → check the hour runs clean
4. iterate until 1000×1h passes with an acceptable error rate
```

## After the test — ROLLBACK

- `loadTesting: true → false` in `infra/envs/production.json` + redeploy ApiGateway **and** Compute.
- CH `api_reader` quota `unlimited → api_throttle` in `services.xml` + container recreate.
