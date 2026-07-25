# SCF Milestone 3 - Form Answers

Copy the text inside each field into the matching SCF form field.

## Field 1 - Tranche Deliverables

> **Deliverable 3 - Mainnet Launch.**
>
> Milestone 3 opens the Soroban Block Explorer to the public on Stellar mainnet,
> load-tested, monitored, and signed off against a security checklist.
>
> - Frontend: `https://sorobanscan.rumblefish.dev`
> - API: `https://api-sorobanscan.rumblefishdev.com/v1`
>
> What is live and verifiable:
>
> 1. **Public access, live data:** the explorer is publicly accessible at its
>    production URL with the pre-launch access gate removed, and renders live
>    mainnet data within 30 seconds of network tip from our own indexed
>    ClickHouse datastore (no third-party chain API on the read path).
> 2. **Reproducible from code:** the GitHub repository is public; the full AWS
>    CDK + Hetzner Ansible deploy reproduces the stack on a fresh AWS account.
> 3. **Monitoring:** a CloudWatch dashboard with Slack-wired alarms and X-Ray
>    tracing tracks ingestion lag (under 30 seconds), API latency, and error
>    rates.
> 4. **Load test:** the production API sustains **40× the required load**
>    (49.3M-requests/month equivalent, ~33k requests measured) at a **0.000%
>    error rate** — AC4 asks for under 0.1%. The **median request meets the
>    latency target** (p50 151–168 ms). **The p95 target is not met: 577 ms
>    against 200 ms.** The tail is flat across a 40× load range (577 ms → 575
>    ms), so it is not a capacity limit — it is attributed, to the millisecond,
>    to three measured causes: two detail endpoints that fetch from outside our
>    infrastructure while the request is open (a deliberate, documented
>    architecture decision — cross-region ledger archive and a third-party IPFS
>    gateway), one identified ClickHouse cost with a specified fix, and a fixed
>    ~40 ms gateway/Lambda/mTLS floor per request. The same test drove a 69%
>    reduction in database work during this milestone. Full accounting, including
>    what we do not meet and why, is in the evidence document § AC4.
> 5. **Security:** all ingress and application security controls are in place and
>    verified against a signed security checklist (least-privilege IAM, AWS WAF +
>    throttling, no public datastore endpoint, secrets in Secrets Manager, TLS
>    end-to-end, server-side input validation, encrypted-at-rest ledger storage,
>    automated off-box backups).
> 6. **Post-launch report:** a 7-day post-launch monitoring report covers uptime,
>    error rate, p95 latency, and ingestion lag per day.
>
> Full evidence package:
> <TODO: Google Drive folder share link>

## Field 2 - Deliverable Verification - Video

> <TODO: Google Drive video share link>

## Field 3 - Additional Deliverable Verification

> **Evidence package:** <TODO: Drive folder link> - contains the Milestone 3
> verification video, `milestone-3-evidence.pdf`, monitoring and launch
> screenshots, the load-test results CSV, the signed security checklist, and the
> 7-day post-launch report.
>
> **Live application (public):**
>
> - Frontend: `https://sorobanscan.rumblefish.dev`
> - API base: `https://api-sorobanscan.rumblefishdev.com/v1`
> - Swagger UI: `https://api-sorobanscan.rumblefishdev.com/api-docs`
> - OpenAPI JSON: `https://api-sorobanscan.rumblefishdev.com/api-docs-json`
>
> **Source code:**
>
> - Repository: `https://github.com/rumblefishdev/soroban-block-explorer`
> - Technical design:
>   `https://github.com/rumblefishdev/soroban-block-explorer/blob/master/docs/architecture/technical-design-general-overview.md`
> - Infra / deploy runbook:
>   `https://github.com/rumblefishdev/soroban-block-explorer/blob/master/infra/README.md`

## Field 4 - Support Needed

> -
