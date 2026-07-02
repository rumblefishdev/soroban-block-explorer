# load-tests — Soroban Block Explorer API load harness (task 0338)

`--vus` concurrent users loop **every** endpoint with **no think-time** for
`--duration`. Each request carries a unique `X-Request-Id` so ClickHouse
`system.query_log` records `read_rows`/`read_bytes` per request (the "B2"
correlation). All output lands in `crates/load-tests/out/<UTC-start>/`
(gitignored). Each step below says which folder to run it from.

> Prereq (deployed once, committed CH config): `api_reader`'s quota + the
> `log_comment` `changeable_in_readonly` constraint (`profiles.xml` +
> `config.d/access-control.xml`) must be live on the box — sync CH config +
> force-recreate the container. Without the constraint every request 500s.

---

## 1. Deploy the infra in load-test mode

**Run from: `infra/`**

Set `"loadTesting": true` in `infra/envs/production.json`, then deploy the two
affected stacks (NOT `--all` — it sweeps unrelated stacks):

```bash
AWS_PROFILE=soroban-explorer make deploy-production-apigateway
AWS_PROFILE=soroban-explorer make deploy-production-compute
```

- **ApiGateway** — lifts the 50 rps throttle + drops the WAF per-IP rate rule.
- **Compute** — sets `LOAD_TESTING` on the API Lambda → arms the `log_comment` middleware.

## 2. Build the harness

**Run from: repo root**

```bash
cargo build --release -p load-tests
```

## 3. Set auth + run

**Run from: repo root**

`API_KEY` must be in the server `API_KEYS` allowlist; `EDGE_SECRET` is the
Cloudflare edge secret from Secrets Manager:

```bash
export API_KEY="<paid-tier key present in the server API_KEYS>"
export EDGE_SECRET=$(AWS_PROFILE=soroban-explorer aws secretsmanager get-secret-value \
  --secret-id soroban/production/cloudflare/edge-secret --query SecretString --output text)
```

Base URL — pick one:

- **Direct to the API Gateway origin** (bypasses Cloudflare's own rate limit; use
  this for backend capacity). Take the `ApiEndpoint` from the ApiGateway stack
  output (the `…execute-api.eu-central-1.amazonaws.com/production` URL) and append
  `/v1`. Requires `--edge-secret` (Cloudflare won't inject it off its own path).
- **Through Cloudflare** — `https://api-sorobanscan.rumblefishdev.com/v1` (the real
  path, but subject to Cloudflare's own rate limit → HTTP 429 `error code: 1015`,
  which `loadTesting` does NOT control).

`API_KEY`/`EDGE_SECRET` are read from the exported env (above) — pass them as
env, NOT as `--api-key`/`--edge-secret` flags, so the secrets never land in the
process arg list (`ps aux`) during the run.

```bash
# smoke first — expect mostly 200
./target/release/load-tests \
  --base-url <API_GW_ORIGIN>/v1 \
  --vus 10 --duration 1m

# full run
ulimit -n 65535
./target/release/load-tests \
  --base-url <API_GW_ORIGIN>/v1 \
  --vus 1000 --duration 1h --harvest 500
```

Output → `crates/load-tests/out/<UTC-start>/client.csv`. At the end the harness
prints the run dir and a **ready-to-paste** `--param_start='…' --param_end='…'`
window — copy it for step 4 (no `date -u` needed).

Diagnostics if the smoke isn't 200: `401` = `x-api-key` not in `API_KEYS`;
`403` = missing `x-edge-secret` (or WAF `NoUserAgent` — the harness sets a UA);
`429`/`1015` = Cloudflare rate limit (use the direct origin); `500 db_error` =
CH rejected `log_comment` — the `profiles.xml` `changeable_in_readonly`
constraint (+ `config.d/access-control.xml`) isn't deployed to the box.

## 4. Pull the per-request query_log to your laptop

**Run from: repo root**

Paste the window the harness printed. Read-only `SELECT` on `system.query_log`;
CSV streams over SSH straight to your disk (nothing is written on the box):

```bash
cat crates/load-tests/query_log_per_request.sql | \
  ssh -i ~/.ssh/<key> deploy@<box-ip> \
    "docker exec -i app-clickhouse-1 clickhouse-client \
      --param_start='<UTC start>' --param_end='<UTC end>'" \
  > crates/load-tests/out/<UTC-start>/query_log_per_request.csv
```

`-i` (NOT `-it`) — a TTY breaks the pipe.

## 5. Join into one CSV

**Run from: repo root**

```bash
./target/release/join \
  --client    crates/load-tests/out/<UTC-start>/client.csv \
  --query-log crates/load-tests/out/<UTC-start>/query_log_per_request.csv \
  --out       crates/load-tests/out/<UTC-start>/results.csv
```

`results.csv` = one dry row per request:
`ts_ms,round,vu,request_id,endpoint,method,http_status,err_class,duration_ms,ttfb_ms,read_rows,read_bytes,ch_queries,ch_duration_ms,memory_max,url`.
Compute any stats in post-processing.

## 6. Roll back after the test

**Run from: `infra/`** — set `"loadTesting": false`, then
`make deploy-production-apigateway` + `make deploy-production-compute`. Then
rotate / remove your `API_KEY` from the server `API_KEYS`.
