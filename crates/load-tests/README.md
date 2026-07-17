# load-tests — Soroban Block Explorer API load harness (task 0338)

Two drivers. **Pick by the question you are answering:**

| Flag | Model | Rate is | Use for |
|---|---|---|---|
| `--rps` | open — Poisson arrivals, spawn-and-forget | an **input** | AC4 "N req/month" (task 0357) |
| `--vus` | closed — N users sweep every endpoint, no think-time | an **output** | saturation / knee-finding |

`--vus` **cannot express a req/month target**: it sends as fast as the server
answers, so the same `--vus 4` produced 9,459 rps against a local stub and ~10
rps against prod. It also suffers *coordinated omission* — when the backend
slows, the client backs off with it, so the measured p95 flatters a struggling
server. `--rps` fixes both: arrivals follow a schedule fixed in advance and do
not care how slow the answers are.

Each request carries a unique `X-Request-Id` so ClickHouse `system.query_log`
records `read_rows`/`read_bytes` per request (the "B2" correlation). All output
lands in `crates/load-tests/out/<UTC-start>/` (gitignored). Each step below says
which folder to run it from.

## AC4 tiers — req/month → rps

`rps = req_per_month / 2_592_000` (30d). The harness echoes the implied
req/month on startup, so a fat-fingered rate is obvious before the run.

| Tier | req/month | `--rps` | vs prod edge limits |
|---|---|---|---|
| A | 1M | `0.386` | under both (116 per 5-min WAF window) |
| B | 10M | `3.858` | under both (1,157 / 2,000 WAF window) |
| C | 50M | `19.29` | **needs `loadTesting: true`** — 5,787 per 5-min window vs the 2,000 per-IP WAF rule |

Two things to know before reading the numbers:

- **Tier A cannot demonstrate AC4 on its own.** At 0.386 rps an hour yields
  ~1,390 requests over 26 endpoints. Error rate <0.1% needs n ≥ 3,000 just for
  the rule of three (0 errors in n → 95% upper bound 3/n) — below that, a single
  error reads as a multi-percent failure. Tier B/C carry the statistical power;
  A is the **contention control** (if its p50 matches B's, there is no queueing,
  so B's large-n per-endpoint p95 is a valid estimate at A's rate).
- **Latency here is dominated by per-query cost, not load** (0357: 13-25M rows
  read per request at idle). Lowering the rate does not lower p95. If a tier
  fails, the fix is in the query/schema, not in the capacity.

Tier C's 19.29 rps is a **single-IP** artifact — real 50M/month arrives from
thousands of IPs, which is why the per-IP WAF rule has to come off for it and
not because the rate itself is unrealistic. The prod throttle (`50` rps) is a
~130M req/month ceiling, so it is not the binding constraint at any tier.

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
AWS_PROFILE=sorobanscan make deploy-production-apigateway
AWS_PROFILE=sorobanscan make deploy-production-compute
```

Prod is account `750702271865` / **`eu-central-1`**. If the `sorobanscan` profile
still defaults to another region, pin it once — every `aws` call below is
regional, and a wrong region fails as a confusing `ResourceNotFoundException`
rather than an auth error:

```bash
aws configure set region eu-central-1 --profile sorobanscan
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
export EDGE_SECRET=$(AWS_PROFILE=sorobanscan aws secretsmanager get-secret-value \
  --region eu-central-1 \
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
  --rps 5 --duration 1m

# AC4 tiers (see the table above) — one run each, own out/ dir per run
./target/release/load-tests --base-url <API_GW_ORIGIN>/v1 \
  --rps 0.386 --duration 6m  --harvest 500   # A — 1M/mo, contention control
./target/release/load-tests --base-url <API_GW_ORIGIN>/v1 \
  --rps 3.858 --duration 10m --harvest 500   # B — 10M/mo
./target/release/load-tests --base-url <API_GW_ORIGIN>/v1 \
  --rps 19.29 --duration 12m --harvest 500   # C — 50M/mo, needs loadTesting

# saturation / knee-finding (NOT an AC4 number — see the model table above)
ulimit -n 65535
./target/release/load-tests \
  --base-url <API_GW_ORIGIN>/v1 \
  --vus 1000 --duration 1h --harvest 500
```

Run the tiers **in order and one at a time** — they contend with each other, and
tier A's whole job is to measure an uncontended baseline. Each run writes its own
`out/<UTC-start>/`, so tiers never mix; steps 4-5 are per-run.

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
