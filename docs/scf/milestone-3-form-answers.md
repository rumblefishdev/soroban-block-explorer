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
>    (49.3M-requests/month equivalent; 16,232 requests measured) at a **0.000%
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
>    verified against a signed security checklist (least-privilege IAM, managed
>    edge rule sets and rate limiting in front of the API plus an origin lock and
>    API Gateway throttling, no public datastore endpoint, secrets in Secrets
>    Manager, TLS end-to-end, server-side input validation, encrypted-at-rest
>    ledger storage, automated off-box backups).
> 6. **Post-launch report:** a 7-day post-launch monitoring report covers uptime,
>    error rate, p95 latency, and ingestion lag per day.
> 7. **Real-user testing since launch:** the explorer recorded **146 active
>    users across 310 sessions** (Google Analytics, 30 days to 2026-08-07), and
>    Stellar community members raised **11 improvement reports**, of which **5
>    are resolved** — including a redesign of the transaction-detail page that
>    the reports drove. The rest are tracked as open work. Details in the
>    evidence document § 6.
>
> Full evidence package:
> <TODO: Google Drive folder share link>

## Field 2 - Deliverable Verification - Video

> <TODO: Google Drive video share link>

## Field 3 - Additional Deliverable Verification

> **Evidence package:** <TODO: Drive folder link> - contains the Milestone 3
> verification video, `milestone-3-evidence.pdf` (with the monitoring and launch
> screenshots embedded), and the raw load-test result CSVs. The signed security
> checklist and the 7-day post-launch report are linked from the PDF and from
> the section below.
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
> - Signed security checklist:
>   `https://github.com/rumblefishdev/soroban-block-explorer/blob/master/docs/scf/milestone-3-security-checklist.md`
> - 7-day post-launch report:
>   `https://github.com/rumblefishdev/soroban-block-explorer/blob/master/docs/scf/milestone-3-7day-report.md`

## Field 4 - Support Needed

> -
