# WAF vs Cloudflare — Cost & Capability Comparison

> **Scope:** edge protection for the Soroban Block Explorer — the public CloudFront SPA
> distribution and the public **API Gateway** that serves on-chain data.
> **Verified:** 2026-06-01 against official AWS and Cloudflare pricing/docs (sources at bottom).
> **TL;DR:** Switching the WAF layer to Cloudflare trades AWS WAF's _variable, request-scaling_
> cost (and attack-inflated bills) for a _flat_ per-plan cost, plus free unmetered DDoS and a
> free browser-challenge — at the price of moving DNS, a second control plane, and losing raw
> request logs (Logpush is Enterprise-only).

---

## What is a WAF?

A **WAF (Web Application Firewall)** sits in front of a web app/API and inspects each
incoming HTTP request _before_ it reaches the origin (CloudFront, API Gateway, Lambda…).
It can **allow, block, challenge, or rate-limit** requests based on rules. Typical rules:

- **Managed rule sets** — vendor-maintained signatures for common attacks (SQL injection,
  XSS, known-bad inputs, malicious IPs).
- **Rate-based rules** — block a source IP that exceeds N requests in a time window.
- **Challenge/CAPTCHA** — force the client to prove it is a real browser (runs JS / solves a
  puzzle) — filters bots and scripts.

A WAF is **not** authentication. For a _public_ explorer serving _public_ blockchain data to
_anonymous_ visitors, a WAF cannot make the API "private" — it can only **raise the bar**
against bots, scrapers, and floods. Any request the browser makes is visible in DevTools and
can be replayed; "browser-only" is never absolute.

## What is Cloudflare?

**Cloudflare** is a global edge platform (CDN + security) that you put in front of your origin
by pointing your domain's DNS at Cloudflare (proxied / "orange-cloud"). All traffic flows
through Cloudflare's network first, where it provides **CDN caching, a WAF, unmetered DDoS
protection, bot mitigation, rate limiting, and challenges (Turnstile / Managed Challenge)** —
bundled into a **flat per-plan price** rather than per-request. It is an alternative _edge
layer_; it does not replace AWS for compute/storage/data.

---

## Setup at the time of this comparison (AWS WAF) — no longer live

Two **AWS WAF WebACLs** (one construct, identical rule set — `infra/src/lib/constructs/waf-web-acl.ts`):

| WebACL                            | Scope / region          | Attached to                      | Rate limit              |
| --------------------------------- | ----------------------- | -------------------------------- | ----------------------- |
| `production-soroban-explorer-cf`  | CLOUDFRONT (us-east-1)  | CloudFront SPA distribution      | 10 000 req / 5 min / IP |
| `production-soroban-explorer-api` | REGIONAL (eu-central-1) | API Gateway stage (the data API) | 2 000 req / 5 min / IP  |

Each WebACL runs 4 rules (priority order):

1. `AWSManagedRulesCommonRuleSet` — common web attacks
2. `AWSManagedRulesKnownBadInputsRuleSet` — known malicious payloads
3. `AWSManagedRulesAmazonIpReputationList` — block IPs on AWS threat-intel list
4. `RateLimit` — per-IP rate-based block

`defaultAction: allow` (block-known-bad model). Logs went to CloudWatch Logs
(`aws-waf-logs-*`, 1-month retention). Toggled by `enableWaf` in
`infra/envs/production.json` — `true` when this comparison was written. That setting
and the code behind it were removed entirely in task 0302.

> **Note:** the data API (API Gateway) is currently fully public and anonymous (proxy mode,
> `apiKeyRequired=false`); CORS is set to `https://sorobanscan.rumblefish.dev` but CORS does
> **not** stop non-browser clients. The raw `execute-api` URL also answers directly — see
> "Origin lockdown" below.

---

## Cost breakdown — AWS WAF (verified 2026-06-01)

AWS WAF's cost model is **fixed + per-request**, so it **scales with traffic** (and with attack volume).

| Component                                                     | Price                                                                                          | Scales with traffic? |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | -------------------- |
| Web ACL                                                       | **$5.00 / month** each                                                                         | no                   |
| Rule **or** managed rule group                                | **$1.00 / month** each (a managed group = one $1 charge, _not_ per sub-rule, _not_ free)       | no                   |
| Request inspection                                            | **$0.60 / million** requests (base, up to 1 500 WCU / 8 KB body)                               | **yes**              |
| Baseline managed rules (Common, KnownBadInputs, IpReputation) | no add-on fee (still incur the $1/group + $0.60/M)                                             | —                    |
| **CAPTCHA action**                                            | **$0.40 / 1 000 attempts**                                                                     | yes                  |
| **Challenge action** (silent)                                 | **~$0.15 / 1 000 responses served** — **NOT free**                                             | yes                  |
| Bot Control (Common)                                          | **$10 / month + $1.00 / million** analyzed (first 10 M/mo free)                                | yes                  |
| Bot Control (Targeted)                                        | $10 / month + **$10.00 / million**                                                             | yes                  |
| WAF logs → CloudWatch Logs                                    | $0.50 / GB ingest (us-east-1; ~$0.57 eu-central-1) + storage; 500 MB included per 1 M requests | yes                  |
| WCU overage                                                   | $0.20 / million per additional 500 WCU beyond 1 500                                            | yes                  |
| Oversized body inspection                                     | $0.30 / million per additional 16 KB                                                           | yes                  |

**Current fixed cost** (2 WebACLs × ($5 ACL + 4 × $1 rules)) = **~$18 / month**, plus
**$0.60 per million** inspected requests on each ACL, plus CloudWatch log costs.

> ⚠️ **Attack amplifies your bill:** under a flood/DDoS, AWS WAF charges $0.60/M for **every**
> inspected request — including the attacker's. WAF blocks _before_ Lambda (so you don't pay
> Lambda for blocked requests), but you still pay the inspection fee on attack traffic.
> AWS Shield Standard (free) provides only basic L3/4 DDoS; Shield Advanced is **$3 000/month**.

---

## Cost breakdown — Cloudflare (verified 2026-06-01)

Cloudflare's WAF/CDN cost model is **flat per plan** — **no per-request WAF inspection fee**,
**no egress charge** on standard proxied traffic.

| Plan         | Price (annual / monthly)                          | WAF-relevant inclusions                                                                                                                           |
| ------------ | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Free**     | **$0**                                            | Unmetered DDoS, CDN, free SSL, **Free Managed Ruleset only** (limited), 1 rate-limit rule, Bot Fight Mode, **Turnstile + Managed Challenge free** |
| **Pro**      | **$20/mo (annual) / $25/mo (monthly)** per zone   | Full Cloudflare Managed Ruleset + OWASP Core Ruleset, 2 rate-limit rules, Super Bot Fight Mode                                                    |
| **Business** | **$200/mo (annual) / $250/mo (monthly)** per zone | 5 rate-limit rules, advanced custom WAF                                                                                                           |
| Enterprise   | Contact sales                                     | 100 rate-limit rules, ML Bot Management, **Logpush (raw log export)**, Log action                                                                 |

Key facts (all verified):

- **No per-request WAF fee** on Free/Pro/Business — fundamentally different from AWS's $0.60/M.
- **Turnstile** (CAPTCHA-alternative) and **Managed/JS Challenge** actions are **free on all
  plans** (only the `Log` action is Enterprise-only) — this is the "browser-only" lever, at $0.
- **Unmetered DDoS** on every plan, including Free — attack traffic does **not** inflate the bill.
- **No bandwidth/egress charge** for standard proxied HTTP(S) traffic.
- Rate limiting is **bundled** (no per-request fee); the legacy per-10k-request billing was
  retired (API end-of-life 2025-06-15).
- **Bot Management** (ML per-request bot scores) is **Enterprise-only**; Free/Pro get Bot
  Fight / Super Bot Fight Mode.
- **Logpush (raw request log export) is Enterprise-only.** Free/Pro/Business get only
  sampled/aggregated analytics dashboards — **no raw log export**.

---

## Side-by-side comparison

| Dimension         | AWS WAF (current)                        | Cloudflare (Free)                        | Cloudflare (Pro)        |
| ----------------- | ---------------------------------------- | ---------------------------------------- | ----------------------- |
| Pricing model     | Fixed + **per-request**                  | **Flat $0**                              | **Flat $20–25/mo**      |
| Fixed monthly     | ~$18 (2 WebACLs)                         | $0                                       | $20–25                  |
| Per-request fee   | $0.60 / million                          | none                                     | none                    |
| Managed WAF rules | Full (Common/KnownBad/IpRep)             | Limited (Free ruleset only)              | Full (Managed + OWASP)  |
| Rate limiting     | per-IP rate rule (incl.)                 | 1 rule                                   | 2 rules                 |
| Browser challenge | CAPTCHA $0.40/1k, Challenge ~$0.15/1k    | **Free** (Turnstile / Managed Challenge) | **Free**                |
| Bot mitigation    | Bot Control ($10/mo + $1/M)              | Bot Fight Mode                           | Super Bot Fight Mode    |
| DDoS              | Shield Standard (basic); Adv = $3 000/mo | **Unmetered, free**                      | **Unmetered, free**     |
| Egress            | CloudFront egress billed                 | **Free**                                 | **Free**                |
| Raw request logs  | CloudWatch Logs (full)                   | sampled analytics only                   | sampled analytics only  |
| Control plane     | CDK / CloudFormation (single IaC)        | separate (dashboard / Terraform)         | separate                |
| DNS               | Route 53 (unchanged)                     | must move to Cloudflare                  | must move to Cloudflare |

### Cost scenarios (base WAF only, ignoring challenge/bot add-ons)

| Monthly requests        | AWS WAF (~$18 fixed + $0.60/M) | Cloudflare Free | Cloudflare Pro |
| ----------------------- | ------------------------------ | --------------- | -------------- |
| 10 M                    | ~$24                           | $0              | $25            |
| 100 M                   | ~$78                           | $0              | $25            |
| 1 B (e.g. under attack) | **~$618**                      | $0              | $25            |

**Break-even:** AWS WAF matches Cloudflare **Pro** (~$25/mo monthly) at **~12 M requests/month**;
above that AWS is more expensive. Versus Cloudflare **Free ($0)**, AWS WAF is **always** more
expensive (it has a ~$18 fixed floor). Add CAPTCHA/Challenge/Bot Control and AWS grows faster.

---

## Pros & cons

### AWS WAF (current)

**Pros**

- Native AWS integration — one IaC (CDK), WAF metrics + sampled requests in CloudWatch, full
  raw logs in CloudWatch Logs.
- No DNS change, no second vendor/control plane.
- Tightly scoped per-resource (separate CloudFront vs API ACLs).

**Cons**

- Cost **scales with traffic** and **with attacks** ($0.60/M on all inspected requests).
- "Browser-only" challenge (CAPTCHA/Challenge) and Bot Control are **paid per use**.
- Weak built-in DDoS (Shield Standard); real DDoS resilience = Shield Advanced at $3 000/mo.
- CloudFront egress is billed.

### Cloudflare

**Pros**

- **Flat, predictable** cost — request spikes and DDoS do **not** inflate the bill.
- **Free unmetered DDoS** and **free egress**.
- **Free browser challenge** (Turnstile / Managed Challenge) — directly serves the
  "filter non-browsers/bots" goal at $0.
- Strong managed WAF + bot mitigation bundled (Pro).

**Cons**

- **Raw request logs are Enterprise-only** (Logpush) — on Free/Pro you lose the CloudWatch-grade
  request log visibility you have today.
- **DNS must move to Cloudflare** (operational change; note `ch.sorobanscan…` mTLS must stay
  unproxied).
- **Second control plane** outside CDK (more to manage; Terraform provider exists).
- **Origin lockdown required** (see below) or the protection is bypassable.
- Free plan's managed WAF rules are limited (full set needs Pro).

---

## What switching to Cloudflare gives us (vs. current AWS WAF)

**Gains**

- Predictable flat cost; attacks/spikes don't raise the bill.
- Free unmetered DDoS (vs. weak Shield Standard / $3 000 Shield Advanced).
- Free browser challenge (Turnstile/Managed Challenge) to filter bots/scripts on the data API.
- Free egress (relevant if the front is also proxied).
- Stronger bundled bot mitigation.

**Losses / costs of switching**

- Raw request logs (Logpush) become Enterprise-only — visibility downgrade.
- DNS migration + a second control plane outside CDK.
- Engineering work: **origin lockdown** + frontend challenge integration.

---

## Critical regardless of choice: origin lockdown

Today the API answers on both `api.sorobanscan.rumblefish.dev` **and** the raw `execute-api`
URL. Any edge protection (AWS WAF Challenge **or** Cloudflare) is **bypassable** unless the
origin (API Gateway / CloudFront) is locked to accept traffic **only** from the edge — via an
API Gateway resource policy (IP allowlist) and/or a secret header injected by the edge and
enforced at the origin. Without this, an attacker hits the origin directly and skips the WAF.

---

## Recommendation summary

- **Low/moderate traffic, no attacks:** both are cheap; Cloudflare Free ($0) is cheaper than
  AWS WAF's ~$18 floor.
- **"Filter non-browsers/bots on the data API":** Cloudflare's free challenge tilts the
  economics (AWS Challenge/CAPTCHA are paid per use).
- **Real DDoS concern:** Cloudflare wins on both cost (flat) and protection (unmetered).
- **If raw log visibility and single-IaC simplicity matter more than DDoS/cost predictability:**
  stay on AWS WAF and add a Challenge action + tighter rate limit + origin lockdown.
- **Do origin lockdown either way.**

---

## Sources (verified 2026-06-01)

- AWS WAF pricing — https://aws.amazon.com/waf/pricing/
- AWS WAF CAPTCHA & Challenge — https://docs.aws.amazon.com/waf/latest/developerguide/waf-captcha-and-challenge.html
- AWS Managed Rules list — https://docs.aws.amazon.com/waf/latest/developerguide/aws-managed-rule-groups-list.html
- AWS WAF managed rule groups — https://docs.aws.amazon.com/waf/latest/developerguide/waf-managed-rule-groups.html
- CloudWatch Logs pricing — https://aws.amazon.com/cloudwatch/pricing/
- Cloudflare plans — https://www.cloudflare.com/plans/
- Cloudflare WAF managed rules — https://developers.cloudflare.com/waf/managed-rules/
- Cloudflare rate limiting — https://developers.cloudflare.com/waf/rate-limiting-rules/
- Cloudflare ruleset actions — https://developers.cloudflare.com/ruleset-engine/rules-language/actions/
- Cloudflare Turnstile plans — https://developers.cloudflare.com/turnstile/plans/
- Cloudflare bots — https://developers.cloudflare.com/bots/
- Cloudflare Logpush — https://developers.cloudflare.com/logs/logpush/

> **Verification caveats:** the AWS **Challenge** action figure (~$0.15/1 000 responses) and the
> **eu-central-1** CloudWatch ingestion rate (~$0.57/GB) are well-attested in prose/docs but were
> not lifted verbatim from the live rendered pricing tables — confirm in the **AWS Pricing
> Calculator** before using for a contract-grade budget. All other figures were confirmed against
> the official pages above.
