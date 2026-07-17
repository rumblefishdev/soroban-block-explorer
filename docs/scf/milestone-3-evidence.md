---
margin:
  x: 1.5cm
  y: 1.5cm
---

# Soroban Block Explorer - Milestone 3 Evidence

Project: Soroban Block Explorer  
Team: Rumble Fish  
Deliverable: Milestone 3 - Mainnet Launch

This document accompanies the Milestone 3 verification video and maps the
approved acceptance criteria to concrete evidence from the publicly launched
explorer, its monitoring, its load-test results, and its security posture.

## 1. Executive Summary

Milestone 3 is the public Mainnet Launch of the Soroban Block Explorer. The
explorer is publicly accessible at its production URL with the pre-launch access
gate removed; it renders live Stellar mainnet data within seconds of network
tip from the project's own indexed ClickHouse datastore (no third-party chain
API on the read path). The public surface is protected by AWS WAF and request
throttling; a CloudWatch dashboard with Slack-wired alarms gives live
visibility into ingestion lag, API health, and error rates. Capacity is demonstrated by a load test of the production API, and
all ingress and application security controls are verified against the security
checklist.

- Frontend: `https://sorobanscan.rumblefish.dev`
- API: `https://api-sorobanscan.rumblefishdev.com/v1`

## 2. Approved Deliverable

Deliverable 3 - Mainnet Launch:

> The explorer is opened to the public on mainnet, load-tested, monitored, and
> signed off against a security checklist, with a 7-day post-launch monitoring
> report demonstrating stability.

Acceptance criteria:

1. **AC1** — Publicly accessible at the production URL, serving live mainnet
   data within 30 seconds of network tip.
2. **AC2** — GitHub repository is public; `cdk deploy` from the README
   reproduces the stack on a fresh AWS account.
3. **AC3** — CloudWatch monitoring dashboard; alarms healthy; ingestion lag
   under 30 seconds.
4. **AC4** — Load-test report: p95 < 200 ms at the 1M-requests/month equivalent
   with error rate < 0.1%.
5. **AC5** — Security checklist signed off.
6. **AC6** — 7-day post-launch monitoring report (uptime, error rate, p95,
   ingestion lag per day).

## 3. Live Endpoints and Reviewer Access

| Resource         | URL / Access                                                                                       |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| Frontend         | `https://sorobanscan.rumblefish.dev` (public — gate removed)                                       |
| API base         | `https://api-sorobanscan.rumblefishdev.com/v1`                                                     |
| Swagger UI       | `https://api-sorobanscan.rumblefishdev.com/api-docs`                                               |
| OpenAPI JSON     | `https://api-sorobanscan.rumblefishdev.com/api-docs-json`                                          |
| API access model | <TODO: confirm post-launch API access — public read vs `x-api-key` behind edge; state final model> |
| Monitoring       | CloudWatch dashboard with Slack-wired alarms and X-Ray tracing                                     |

At launch the frontend is publicly accessible with no Basic Auth gate. The
verification video demonstrates the live application end-to-end.

## 4. Architecture Evidence

The launched system spans three paths, all reproducible from code (AWS CDK +
Hetzner Ansible):

1. **Ingestion / write path** (Milestone 1) — Galexie on ECS Fargate exports
   mainnet `LedgerCloseMeta` XDR to S3; a Rust indexer Lambda decodes each
   object and writes to ClickHouse. End-to-end ledger-close to DB-write is well
   under the 30-second target.
2. **Read path** (Milestone 2) — Browser → Cloudflare edge → API Gateway → Rust
   / axum API Lambda → ClickHouse (Hetzner, mTLS). Static SPA: Browser →
   CloudFront → private S3.
3. **Launch controls** (Milestone 3) — AWS WAF + request throttling on public
   ingress; CloudWatch dashboard + Slack alarms + X-Ray tracing.

Evidence images:

![Milestone 2 read path](./architecture-m2-read-path.png){width=55%}

_Figure 1 — Read path (reused from M2). The ingestion / write path is in the
Milestone 1 evidence._

<TODO: optional — launch-controls diagram (WAF + throttling + dashboard), or reuse architecture.png>

## 5. Acceptance Criteria Evidence

### AC1 - Public access, live data ≤30s from tip

The explorer is reachable at the production URL with the pre-launch gate
removed, and shows live mainnet data seconds behind network tip.

- <TODO: screenshot — production URL loading with no Basic Auth prompt (ac1-public-no-gate.png)>
- <TODO: screenshot/evidence — latest ledger on the explorer vs current network tip, delta < 30s (ac1-data-freshness.png)>

### AC2 - Public repo + reproducible `cdk deploy`

The repository is public and the stack is reproducible from code.

- Public repository: `https://github.com/rumblefishdev/soroban-block-explorer`
- <TODO: consolidated fresh-AWS-account runbook in `infra/README.md` (task 0128); a few out-of-band steps remain by design — Hetzner Server order, Hetzner Storage Box order>
- <TODO: screenshot — GitHub repo public (ac2-repo-public.png)>

### AC3 - Monitoring dashboard, ingestion lag

CloudWatch dashboard with Slack-wired alarms and X-Ray is live.

- <TODO: API observability widgets — API Lambda Throttles/Errors, API Gateway latency, WAF blocked requests>
- <TODO: seconds-based ingestion-lag metric feeding both the dashboard and the AC6 report>
- <TODO: screenshot — production dashboard, healthy alarms (ac3-dashboard.png, ac3-alarms-ok.png)>

### AC4 - Load-test report

**Status: partially met — error rate met with a ~100× margin; p95 not met.**
This section states the result plainly and then accounts for it in full.

| AC4 half    | Target   | Measured (1M-req/month equivalent) | Verdict     |
| ----------- | -------- | ---------------------------------- | ----------- |
| Error rate  | < 0.1 %  | **0.000 %** (0 failures)           | **Met**     |
| p95 latency | < 200 ms | **577 ms**                         | **Not met** |

Median latency (p50) is **168 ms** at the required load and **151 ms** at 40×
that load — the typical request is inside the target. The miss is in the tail.

#### Method: why the load is expressed as a rate, not as virtual users

The criterion is "1M requests/month equivalent" — a **request rate**
(1,000,000 / 30 days ≈ **0.4 req/s**). The harness therefore drives an **open
model**: requests arrive at a fixed rate (Poisson), independent of how fast the
server answers.

The earlier plan called for "≥1000 concurrent users". That framing was
abandoned for a measured reason, not for convenience: a virtual-user driver is
**closed-loop**, so its request rate is an _output_, not an input — each user
waits for a response before sending the next. Our own measurement of the same
`--vus 4` configuration produced **9,459 req/s against a local stub** and
**~10 req/s against production**. A VU count therefore cannot express "1M
requests/month"; only an arrival rate can. Open-model testing is what actually
measures the stated criterion. (Recorded in task 0357.)

#### Result across load tiers

Two open-model series were run against the production API (~33k requests
total). Representative tiers:

| Load tier                      |     Rate | Requests |  Error rate |    p50 |    **p95** |
| ------------------------------ | -------: | -------: | ----------: | -----: | ---------: |
| **1.2M req/month (AC4 level)** |  0.48 /s |      168 | **0.000 %** | 168 ms | **577 ms** |
| 10.2M req/month (8×)           |  3.95 /s |    2,363 | **0.000 %** | 149 ms |     567 ms |
| **49.3M req/month (40×)**      | 19.04 /s |   13,701 | **0.000 %** | 151 ms | **575 ms** |

**The p95 is flat across a 40× range of load (577 ms → 575 ms).** This is the
central finding: latency is **not load-dependent** in the tested range. The
system absorbs forty times the required traffic with an unchanged tail and zero
failures — capacity is proven far beyond the criterion. The p95 miss is
therefore **not a capacity problem**; it is a set of fixed per-request costs
that no amount of headroom removes.

#### Where the 577 ms comes from

Decomposing the same measurement by endpoint class:

| Scope                                                | p95 @ AC4 load | p95 @ 40× load |
| ---------------------------------------------------- | -------------: | -------------: |
| All 26 endpoints                                     |         577 ms |         575 ms |
| Excluding the 2 synchronous-external-fetch endpoints |         342 ms |         506 ms |
| Excluding those and `lplist`                         |     **280 ms** |         317 ms |

Three distinct causes, in order of size:

1. **Synchronous external fetches on two detail endpoints (largest).**
   `txdetail` and `nftdetail` fetch data from outside our infrastructure while
   the request is open. `txdetail` spends **53 ms in ClickHouse and 1,074 ms
   (p95) waiting on an S3 archive read in `us-east-2` from a Lambda in
   `eu-central-1`** — a cross-region round trip per request. `nftdetail`
   spends **96 ms in ClickHouse and 1,604 ms (p95)** on a third-party IPFS
   gateway plus a `token_uri()` RPC. These are **deliberate architectural
   decisions** (ADR 0029, ADR 0043: heavy/detail-only fields are fetched at
   read time rather than persisted), not defects — but they place the tail of
   those two endpoints partly outside our control.

2. **A known ClickHouse cost on one endpoint.** `lplist` derives each pool's
   creation ledger by scanning that pool's full snapshot history
   (8.28M rows/request). This is our own technical debt: the design decision
   that removed the stored column (task 0208) assumed this read would be cheap;
   measurement shows it is not. The cause is identified to the query, the fix is
   specified, and it is scheduled — not unexplained.

3. **A fixed infrastructure floor (irreducible today).** Every request pays
   **≈ 40 ms** before any data is read (API Gateway + Lambda + mTLS to
   ClickHouse), plus **≈ 15 ms per ClickHouse round trip**. Measured across all
   26 endpoints, the median non-database overhead fits
   `≈ 40 ms + 15 ms × (number of CH queries)` — from 51 ms for a
   single-query endpoint to 157 ms for an 8-query endpoint. An endpoint issuing
   five queries has **already spent ~115 ms of the 200 ms budget** before
   ClickHouse returns a single row.

#### Optimisation performed during this milestone

The load test drove a measured optimisation pass on the read path. Total
ClickHouse work per test run fell from **78.3 billion to 23.89 billion rows
(−69 %)** at identical load, via two query fixes ([#347], [#349]) and one index.
Endpoints that were **not modified at all** improved by 3–4× as a direct result
(`ldgdetail` p95 890 → 319 ms, `ldglist` 1015 → 229 ms) — evidence that the
earlier latency was contention from a saturated database rather than the
endpoints themselves.

- <TODO: attach the raw results CSVs (per-tier `results.csv` + `client_summary.csv`) under screenshots/ or docs/scf/load-tests/>
- <TODO: post-run rollback note — `loadTesting` flag → false, API_KEY rotation>

#### Honest assessment

Restated plainly for the reviewer: **the p95 < 200 ms criterion is not met at
any tested tier, including the required one — 577 ms against a 200 ms target.**
It is not met even with all three causes above excluded (280 ms). What the
measurement does establish:

- the **error-rate half of AC4 is met with a ~100× margin** (0.000 % vs 0.1 %);
- the **median request meets the latency target** (168 ms at the required load);
- **capacity is proven at 40× the required load** with no degradation and no
  errors;
- every millisecond of the gap is **attributed to a named, measured cause**,
  each with a decided path: persist the two runtime-fetched field sets (or
  serve them asynchronously), restore `lplist`'s stored creation ledger, and
  reduce the per-query overhead if the 15 ms proves to be a per-query mTLS
  handshake rather than an irreducible round trip.

[#347]: https://github.com/rumblefishdev/soroban-block-explorer/pull/347
[#349]: https://github.com/rumblefishdev/soroban-block-explorer/pull/349

### AC5 - Security checklist signed off

All ingress and application controls are in place and verified: least-privilege
IAM (no wildcard actions), AWS WAF + request throttling, no public datastore
endpoint (ClickHouse bound to loopback behind mTLS/Caddy on a firewalled host),
secrets in AWS Secrets Manager, TLS end-to-end, server-side input validation on
every endpoint, SSE-S3 at rest on the public ledger bucket, and automated
off-box ClickHouse backups.

- <TODO: signed security checklist document (task 0090) — controls list + sign-off; note KMS/PITR items are satisfied by architecture-appropriate equivalents after the RDS retirement>

### AC6 - 7-day post-launch monitoring report

Generated from production telemetry over the first 7 days after launch.

- Report template + metric queries: `milestone-3-7day-report.md`
- <TODO: filled report — uptime, error rate, p95, ingestion lag per day; zero ledger gaps over the window>

## 6. Source References

| Resource          | Link                                                                                                                                                               |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Public repository | [rumblefishdev/soroban-block-explorer](https://github.com/rumblefishdev/soroban-block-explorer)                                                                    |
| Technical design  | [technical-design-general-overview.md](https://github.com/rumblefishdev/soroban-block-explorer/blob/master/docs/architecture/technical-design-general-overview.md) |
| Infra / deploy    | [infra/README.md](https://github.com/rumblefishdev/soroban-block-explorer/blob/master/infra/README.md)                                                             |
| Load-test harness | [crates/load-tests](https://github.com/rumblefishdev/soroban-block-explorer/tree/master/crates/load-tests)                                                         |
| M1 / M2 evidence  | [milestone-1-evidence.md](./milestone-1-evidence.md) · [milestone-2-evidence.md](./milestone-2-evidence.md)                                                        |
