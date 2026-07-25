# Milestone 3 - Deliverable Verification Video Script

Purpose: record a short SCF deliverable verification video for Milestone 3:
Mainnet Launch.

Target length: 4-6 minutes.

## Before recording

Prepare these values and keep the tabs / consoles open:

| Item                    | Value                                                                                                                                                                                                                                          |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend (public)       | `https://sorobanscan.rumblefish.dev`                                                                                                                                                                                                           |
| API                     | `https://api-sorobanscan.rumblefishdev.com/v1`                                                                                                                                                                                                 |
| Swagger UI              | `https://api-sorobanscan.rumblefishdev.com/api-docs`                                                                                                                                                                                           |
| CloudWatch dashboard    | `production-soroban-explorer` (eu-central-1) — [console link](https://eu-central-1.console.aws.amazon.com/cloudwatch/home?region=eu-central-1#dashboards/dashboard/production-soroban-explorer)                                                |
| CloudWatch alarms       | [alarms list](https://eu-central-1.console.aws.amazon.com/cloudwatch/home?region=eu-central-1#alarmsV2:) — filter the name column to the `production-` block                                                                                   |
| Load-test results       | `milestone-3-evidence.md` § AC4 (tier table + decomposition) — measured 2026-07-17                                                                                                                                                             |
| Security checklist      | `milestone-3-security-checklist.md` (9 controls + OWASP Top 10 mapping, signed)                                                                                                                                                                |
| Ledger (freshness demo) | Read the latest sequence off the explorer at record time and compare it with a neutral source — `stellar.expert/explorer/public` or `horizon.stellar.org/ledgers?order=desc&limit=1`. They should match within one or two ledgers (~5 s each). |

## Scene 1 - Intro and scope

SHOW: frontend home page (public, no gate).

SAY:

> Hi, I am <DEV_NAME> from Rumble Fish. This video verifies Milestone 3 of the
> Soroban Block Explorer: Mainnet Launch. The explorer is now public. I will
> show public access with the pre-launch gate removed, live mainnet data within
> seconds of network tip, the monitoring dashboard,
> the load-test results, and the security posture.

## Scene 2 - AC1: public access + live data ≤30s

SHOW: production URL loading with no Basic Auth prompt; latest ledger on the
explorer next to current network tip.

SAY:

> The explorer is publicly accessible at its production URL — no login gate. The
> latest ledger shown here is <N>; the current network tip is <N or N+1>, so the
> data is within seconds of tip. The read path is our own indexed ClickHouse,
> not a third-party chain API.

## Scene 3 - AC2: public repo + reproducible deploy

SHOW: public GitHub repo; `infra/README.md` fresh-account runbook.

SAY:

> The repository is public. The infra README is a fresh-AWS-account runbook:
> `cdk deploy` reproduces the AWS side, and the Hetzner side is stood up from
> Ansible. A few steps are out-of-band by design — ordering the Hetzner server
> and storage box.

## Scene 4 - AC3: monitoring

SHOW: CloudWatch dashboard (ingestion lag, API latency/errors, WAF blocks);
alarms in OK state.

SAY:

> This is the production CloudWatch dashboard: ingestion lag under 30 seconds,
> API latency and error rates, and WAF blocked requests. Alarms are wired to
> Slack.

## Scene 5 - AC4: load test

> **Presenter note — read this before recording.** The earlier draft of this
> scene claimed "p95 is under 200 ms" and "a stress pass at about 1000
> concurrent users". **Both statements are false** and must not be said on
> camera. The measured p95 at the required load is **577 ms**, and the harness
> uses an open (rate-based) model with no virtual-user concept. AC4's error-rate
> half **is** met (0.000 %). Say the result, then the reason — the numbers below
> are strong on their own and the full accounting is in
> `milestone-3-evidence.md` § AC4.

SHOW: the tier table (rate → error rate → p50 → p95) from
`milestone-3-evidence.md` § AC4.

SAY:

> We load-tested the production API. The criterion is one million requests per
> month, which is a rate — about zero point four requests per second — so the
> harness drives requests at a fixed arrival rate rather than with virtual
> users. That distinction matters: a virtual-user driver is closed-loop, so its
> rate is an output, not an input. We measured the same four-user configuration
> producing nine thousand requests per second against a local stub and about ten
> against production — so a user count cannot express "one million per month".
> Only a rate can.

SHOW: the error-rate column across all tiers (all zeros).

SAY:

> On error rate, AC4 asks for under nought point one percent. We measured zero —
> not a single failed request out of roughly thirty-three thousand, including a
> tier at forty times the required load.

SHOW: the p95 row — 577 ms at 1.2M/month next to 575 ms at 49.3M/month.

SAY:

> On latency, we do not meet the target and I want to be direct about it. The
> criterion is a p95 under two hundred milliseconds; we measure five hundred and
> seventy-seven. The median request is inside the target at one hundred and
> sixty-eight milliseconds, but the tail is not.
>
> The important part is why. Look at these two numbers: five hundred and
> seventy-seven milliseconds at the required load, and five hundred and
> seventy-five at forty times that load. The tail does not move with traffic.
> This is not a capacity limit — the system absorbs forty times the required
> load with an unchanged tail and zero errors. It is a set of fixed per-request
> costs, and we have measured each one.

SHOW: the decomposition table (all / minus external-fetch / minus `lplist`).

SAY:

> Three causes. First, two detail endpoints fetch data from outside our
> infrastructure while the request is open — a transaction detail spends fifty
> three milliseconds in our database and over a second waiting on an archive
> read in a different AWS region. That is a deliberate architecture decision to
> fetch heavy fields at read time instead of storing them, and it is documented
> — but it puts that tail partly outside our control. Second, one endpoint
> carries a database cost we own: it scans a pool's full history to derive a
> creation date. We have identified the query, and the fix is specified.
> Third, every request pays about forty milliseconds of gateway, Lambda and
> mutual-TLS overhead before any data is read, plus about fifteen milliseconds
> per database round trip.

SHOW: (optional) the 78.3 → 23.89 billion rows figure and the unmodified-endpoint
improvements.

SAY:

> This load test was not just a measurement — it drove the optimisation. Total
> database work per run fell by sixty-nine percent during this milestone, and
> endpoints we did not touch at all got three to four times faster as a result,
> which told us their earlier latency was contention, not their own cost. Each
> request is correlated to the exact ClickHouse query it triggered, which is how
> we could attribute every millisecond.

## Scene 6 - AC5: security posture

SHOW: security checklist; WAF; ClickHouse not publicly reachable.

SAY:

> Security controls are verified against our checklist: least-privilege IAM with
> no wildcard actions, AWS WAF and throttling on public ingress, no public
> datastore endpoint — ClickHouse is bound to loopback behind mutually
> authenticated TLS on a firewalled host — secrets in Secrets Manager, TLS
> end-to-end, and server-side validation on every input.

## AC6 - 7-day report (not in this recording)

AC6 is evidenced by the written report `milestone-3-7day-report.md`, generated
after the 7-day post-launch window — so it is **not** part of this recording,
which is made at launch. If a short on-camera walkthrough of the finished report
is wanted, record it as a separate add-on once the window closes (can be done by
whoever runs the report).

## Close

SAY:

> That verifies Milestone 3: the Soroban Block Explorer is live on mainnet,
> monitored, load-tested, and secured. Thanks for watching.
