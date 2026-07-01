---
id: '0338'
title: 'Load-testing harness (k6) with per-request ClickHouse correlation (B2) + loadTesting env flag'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0089']
tags: [priority-high, effort-medium, layer-testing, milestone-3, phase-launch]
milestone: 3
links:
  - docs/architecture/technical-design-general-overview.md
history:
  - date: 2026-06-30
    status: backlog
    who: fmazur
    note: >
      Task created. Concrete implementation of D3 criterion #4 load testing,
      superseding the stale generic plan in 0089 (which assumed RDS read-replica
      + staging, both decommissioned). Scope: k6 harness through Cloudflare,
      B2 per-request CH correlation via log_comment, loadTesting env flag,
      CH api_reader unlimited quota, CSV output.
  - date: 2026-06-30
    status: active
    who: fmazur
    note: 'Activated — starting implementation (Step 1 API log_comment + Step 2 loadTesting flag).'
---

# Load-testing harness (k6) with per-request ClickHouse correlation (B2) + loadTesting env flag

## Summary

Build a real HTTP load-testing harness for the production API and produce the
Deliverable 3 criterion #4 report (p95 <200 ms @ 1M req/month equiv, error rate
<0.1%) **plus** a capacity/stress story at ~1000 concurrent users. Each request
is correlated end-to-end with the ClickHouse query it triggered (read_rows /
read_bytes) via the **B2** approach (`log_comment` ↔ `request_id`), and results
export cleanly to CSV. Volumetric protections are lifted for the test window via
a new `loadTesting` env flag.

## Context

D3 §7.4 criterion #4 is the only hard ❌ in the deliverable; there is no HTTP
load harness in the repo today (only `crates/backfill-bench/`, which benches the
indexer write path, not the API). The existing backlog task **0089** is stale
(RDS read-replica + staging assumptions, both gone). This task is the concrete,
current-architecture replacement.

Full analysis (Part A criteria status + Part B test design) lives in
`.temp/milestone-3-claim-i-plan-testow-obciazeniowych.md` (gitignored working
doc).

### Architecture realities that shape this task (verified in code 2026-06-30)

- **Traffic path = through Cloudflare** (path I): k6 → `https://api-sorobanscan.rumblefishdev.com/v1`
  → Cloudflare → API GW → Lambda → (mTLS) → Hetzner ClickHouse.
- **Two gates in front of the app** (`main.rs`, outer→inner):
  1. `edge_lock` (`common/edge_lock.rs`) requires `X-Edge-Secret` — injected by
     Cloudflare (Transform Rule lives in the `rf-domains` repo). k6 needs a
     fallback `x-edge-secret` header if injection isn't live.
  2. `require_auth` (`auth/mod.rs`) — armed when `JWT_SECRET` set. k6 bypasses
     Turnstile/JWT via paid-tier **`x-api-key`**; the key MUST be in the server
     `API_KEYS` allowlist.
  - Diagnostics: 401 = auth/x-api-key, 403 = edge_lock, 429 = throttle/WAF.
- **DDoS protections that will dominate a naive test** (all AWS-side, in
  `infra/envs/production.json`): API GW throttle 50 rps / burst 100; WAF
  rate-based 2000 req / IP / 5 min (~6.7 rps from one IP); API GW response cache
  OFF. mTLS Lambda↔Hetzner is **unrelated** and stays untouched; `enableApiMtls`
  is a separate dormant AWS-side flag (leave false).
- **CH read_rows quota** (`crates/db-clickhouse/users.d/quotas.xml`): `api_reader`
  → `api_throttle` (50B read_rows/h). Filtered lists / `/assets/:id` (~21M
  read_rows/req) can blow it and 500 ALL CH endpoints (prior incident). A
  `<unlimited>` quota already exists in the file.

## Implementation Plan

### Step 1 — API: B2 `log_comment` correlation

- Middleware reading `X-Request-Id` (template: `common/edge_lock.rs`), set as a
  `tokio::task_local!` for the request's duration. Wire in `main.rs`.
- Inject `log_comment=<request_id>` into outgoing CH queries at ONE choke point
  (transport layer in `crates/db-clickhouse/src/mtls.rs` — CH options travel as
  URL query params, confirmed `clickhouse` 0.15 `query.rs:233`), so all ~136
  `fetch_*` call sites inherit it without edits.
- Guard: only set `log_comment` when `X-Request-Id` present (don't pollute normal
  prod query_log). Verify no hot path runs CH via `tokio::spawn` (would drop the
  task-local; `common/ch.rs` uses concurrent futures, OK).
- ⚠️ Regenerate API types if any `crates/api/**` DTO/route changes (CI gate).

### Step 2 — Infra: `loadTesting` env flag

- Add `loadTesting: boolean` to `EnvironmentConfig` (`infra/src/lib/types.ts`).
- When `true` (in `api-gateway-stack.ts`): skip WAF rate-based rule
  (`apiWafRateLimit`) and lift `apiGatewayThrottleRate/Burst`. Leave WAF managed
  rules, `edge_lock`, `enableApiMtls`, and Lambda↔Hetzner mTLS untouched.
- `validateConfig` must loudly warn / hard-error on
  `loadTesting=true && envName=production` to prevent accidental exposure.
- ⚠️ Cloudflare layer (zone owned by `rf-domains`) is NOT controlled by this flag
  — confirm in smoke whether Cloudflare itself rate-limits.

### Step 3 — Hetzner: CH `api_reader` unlimited quota (test window)

- `crates/db-clickhouse/users.d/services.xml`: `api_reader` quota `api_throttle`
  → `unlimited`. Per-query `read_only` cap (30 s / 4 GB) stays — quota ≠ per-query.
- Apply via ansible with **container recreate** (single-file bind-mount stale
  inode; `--tags app` alone is insufficient).

### Step 4 — k6 harness (`.temp/load-tests/<date>/`)

- `setup()` harvests real IDs from list endpoints (transactions/accounts/
  contracts/assets/ledgers/liquidity-pools, ~100 each) → VUs randomize per
  iteration (cover many cases, including deep cursors).
- Per request: generate uuid → headers `x-api-key` (+ `x-edge-secret` fallback) +
  `x-request-id`; measure `http_req_duration` + `http_req_waiting`.
- Scenarios: realistic SPA mix; heavy E3 (`/transactions/{hash}`, the only live
  archive-fetch+XDR path, no edge cache); aggregation (LP chart + filtered
  lists); cache (network/stats + contracts/{id}); search (post-0318).
- Profiles: ramp 0→1000 VU, plateau, spike, capacity-ramp to knee point.
- Client CSV (append, never overwrite — keep the time axis):
  `ts,round,vu,request_id,endpoint,method,url,http_status,err_class,duration_ms,ttfb_ms`
  (`err_class` splits 429 / 5xx / 504).

### Step 5 — Run

Smoke (low scale, verify `log_comment` lands in query_log) → variant (a)
production as-is (document throttle/WAF protection) → variant (b) with
`loadTesting=true` + CH unlimited quota, 1000 VU. Coordinate the window (SNS→Slack
alarms), have a rollback checklist.

### Step 6 — Collect + join + report

- Export `system.query_log` over the window: per `log_comment`, sum read_rows /
  read_bytes, max query_duration_ms, memory. `INTO OUTFILE ... FORMAT CSVWithNames`.
- Join `client.csv.request_id = query_log.log_comment` → `results.csv`.
- Report two verdicts: (1) literal SCF #4 pass at light load; (2) measured
  capacity + bottlenecks (knee point, E3 cost, CH rows/bytes per endpoint) at 1000 VU.

## Acceptance Criteria

- [ ] API emits per-request CH `log_comment` from `X-Request-Id` (one choke point, guarded)
- [ ] `loadTesting` env flag lifts WAF rate + API GW throttle; validateConfig guards prod
- [ ] CH `api_reader` unlimited-quota path documented + applied via container recreate
- [ ] k6 harness: setup() ID harvest, randomized detail IDs, all scenarios, CSV append
- [ ] Smoke confirms `log_comment` ↔ `request_id` correlation works
- [ ] Variant (a) + (b) runs executed; client CSV + query_log CSV collected
- [ ] `results.csv` joins client + server side per request
- [ ] Report: SCF #4 literal pass + capacity/bottleneck story at 1000 VU
- [ ] Rollback completed (loadTesting→false, CH quota→api_throttle, container recreate)
- [ ] Docs updated (evergreen, ADR 0032): API middleware, infra flag, CH quota — or N/A noted

## Design Decisions

### From Plan

1. **B2 over header-forward**: per-request rows/bytes via `log_comment` ↔
   `system.query_log`, not by forwarding `X-ClickHouse-Summary` as a response
   header. The header path requires rewriting ~136 `fetch_*` call sites (summary
   only on `RowCursor`, dropped by `fetch_all`) + `wait_end_of_query=1` (a
   latency confound). query_log records accurate read_rows server-side with no
   confound and no per-call-site change.

2. **Traffic through Cloudflare (path I)**, not direct-to-origin mTLS: keeps the
   real production path; `x-api-key` (paid tier) bypasses Turnstile for k6.

## Future Work

- Decide whether to fold/refresh/archive 0089 once this lands (this supersedes it).
- Possible follow-up: expose CH summary as a response header for ongoing
  per-request observability (beyond load testing) — separate task if wanted.

## References

- `.temp/milestone-3-claim-i-plan-testow-obciazeniowych.md` — full Part A/B analysis
- D3 requirements: `docs/architecture/technical-design-general-overview.md` §7.4 (l. 1450–1476)
- Auth/edge: `crates/api/src/main.rs`, `auth/mod.rs`, `common/edge_lock.rs`
- Throttle/WAF: `infra/envs/production.json`, `api-gateway-stack.ts`, `constructs/waf-web-acl.ts`
- CH quota: `crates/db-clickhouse/users.d/{quotas,services}.xml`
- Parent: 0089 (generic D3 load-testing, stale)
