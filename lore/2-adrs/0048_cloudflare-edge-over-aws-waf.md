---
id: '0048'
title: 'Cloudflare edge (WAF/DDoS) over AWS WAF, origins locked to Cloudflare'
status: accepted # proposed | accepted | deprecated | superseded
deciders: [fmazur, team]
related_tasks: ['0277', '0273', '0302']
related_adrs: ['0001', '0047']
tags: [infra, security, waf, cloudflare, ddos, dns, edge]
links:
  - docs/waf-vs-cloudflare/README.md
  - lore/1-tasks/archive/0277_FEATURE_migrate-edge-protection-to-cloudflare.md
history:
  - date: '2026-06-02'
    status: proposed
    who: fmazur
    note: >
      ADR created during task 0277 Step 1. Team decided the direction at the
      2026-06-01 daily; status stays `proposed` until the Step 7 cutover + soak
      land, then flips to `accepted`. External prerequisite outstanding:
      parent rumblefish.dev zone owner sign-off for the NS delegation change.
  - date: '2026-06-10'
    status: accepted
    who: fmazur
    note: >
      ACCEPTED — deployed and verified in production (task 0277). As-built
      diverged from the ADR's Path-B mTLS: the origin lock ships via the
      SECRET-HEADER variant (Cloudflare Transform Rule stamps `X-Edge-Secret`;
      axum `edge_lock` 403s anything without it), chosen because the repo split
      made a shared secret self-contained vs cross-repo mTLS. Scope also grew
      from "lockdown" into a full paid-API access layer (Cloudflare Turnstile
      widget → session JWT free tier + X-API-Key paid tier) + a CORS layer.
      AWS WAF teardown + soak deferred (backlog 0283 — that id was later
      reassigned in an ID-collision sweep; the teardown task is 0302). NS flipped on the
      `rumblefishdev.com` registrar (OVH), not the parent rumblefish.dev zone.
  - date: '2026-07-27'
    status: accepted
    who: karolkow
    note: >
      Decision 5 taken up by task 0302, which corrected three things this ADR
      left implicit. (a) `enableWaf:false` removes the us-east-1 WebACL stack
      from the CDK app but does not delete it, and once removed from the app
      `cdk destroy` can no longer address it — the teardown needs a raw
      CloudFormation delete, ordered after the consumer stack releases its
      cross-region export claim. (b) Decision 5's premise ("Cloudflare fronts
      the edge") holds only for the API: the frontend zone is still on Route 53,
      so dropping the CLOUDFRONT WebACL leaves the SPA distribution with no edge
      filtering. Confirmed as the intended end state, not a gap — the
      distribution serves static edge-cached files from a private S3 origin, and
      fronting CloudFront with Cloudflare would stack two CDNs. Treat the
      "move the frontend zone first" path as an open question rather than a
      deferred obligation. (c) The cost rationale is framed on the wrong
      component: the `$0.60/M` request fee is ~0.08 USD/mo at real volume
      (~130k req/mo); the saving is the fixed WebACL + rule fee, measured
      18.07 USD in June 2026. Teardown outcome recorded in task 0302.
---

# ADR 0048: Cloudflare edge (WAF/DDoS) over AWS WAF, origins locked to Cloudflare

**Related:**

- [Task 0277: Migrate edge protection (WAF/DDoS) to Cloudflare](../1-tasks/archive/0277_FEATURE_migrate-edge-protection-to-cloudflare.md)
- [Task 0273: Deploy web frontend to CloudFront](../1-tasks/archive/0273_FEATURE_deploy-web-frontend-to-cloudfront.md)
- [Task 0302: Drop both AWS WAF WebACLs (executes Decision 5)](../1-tasks/active/0302_FEATURE_drop-aws-waf-after-soak/README.md)
- [Cost + capability comparison](../../docs/waf-vs-cloudflare/README.md)

---

## Context

The explorer's public edge is fronted by **two AWS WAF WebACLs** — one REGIONAL
(API Gateway) and one CLOUDFRONT (SPA). The public **data API** (API Gateway →
Rust/axum Lambda → Hetzner ClickHouse) serves anonymous, read-only on-chain data
and is trivially scrapeable. Two problems motivated a change:

1. **Cost shape.** AWS WAF bills **per request** (`$0.60 / million` evaluated
   requests, on top of `$5/mo` per WebACL and `$1/mo` per rule). Edge cost
   therefore scales with API traffic — exactly the axis a scraper or flood
   inflates. The migration's primary cost goal is an edge whose price is **flat
   and independent of request volume**.
2. **DDoS posture.** AWS Shield Standard is automatic but shallow; meaningful
   network/transport DDoS protection is Shield Advanced at **$3 000/mo**, which
   is not justifiable for this project.

The data is public on-chain data served to anonymous browsers, so a
"browser-only API" is **not achievable** — requests are replayable. The realistic
goal is to **raise the bar against bots/floods and get real DDoS protection**, not
to implement access control.

The full cost model and capability comparison (verified against AWS + Cloudflare
docs on 2026-06-01) live in
[`docs/waf-vs-cloudflare/README.md`](../../docs/waf-vs-cloudflare/README.md).

---

## Decision

Front the explorer with **Cloudflare** and **drop both AWS WAF WebACLs**:

1. **Plan tier — start on Free.** Cloudflare **Free** covers the primary goal:
   **unmetered DDoS**, **Managed Challenge / Turnstile** (the anti-bot lever, free
   on all plans), and **Transform Rules** for the origin secret. Upgrade to **Pro**
   ($20/mo annual) later — a plan flip with no DNS change / no re-migration — only
   if the full Cloudflare Managed Ruleset + OWASP at the edge, a 2nd rate-limit
   rule, or finer bot control is wanted.
2. **IaC — Terraform `cloudflare` provider.** State in **S3** (versioned,
   encrypted, native S3 lockfile). `apply` run by an operator locally to start,
   migrating to CI (OIDC already exists, see [ADR 0001](./0001_OIDC-cicd-and-public-repo-secret-separation.md))
   once stable. Code lives in **`infra/cloudflare/`**. The Cloudflare API token is
   **zone-scoped, least-privilege**, stored in SSM/Secrets Manager — never in `.tf`
   or committed state.
3. **Origin lockdown — Path B (mTLS) for the API.** Lock the API at the TLS layer
   so it accepts **only Cloudflare**: Cloudflare **zone-level Authenticated Origin
   Pulls** with **our own** uploaded client cert (unique — not global AOP's shared
   cert) **+ AWS API Gateway mTLS** on the REGIONAL custom domain (truststore in
   S3) **+ `disableExecuteApiEndpoint: true`**. The origin rejects any
   non-Cloudflare cert at the TLS handshake — no `crates/api` change.
4. **CloudFront — keep it, lock via secret header.** CloudFront cannot do viewer
   mTLS, so the `*.cloudfront.net` domain is locked with a **secret header**: a
   Cloudflare **Transform Rule** injects `X-Origin-Secret`, a **viewer-request
   CloudFront Function** rejects requests lacking it. Secret stored out of git,
   rotated dual-accept, never logged.
5. **AWS WAF teardown — drop both WebACLs** (`enableWaf:false`), but **only after**
   a soak confirms Cloudflare + lockdown are stable. Keeping any AWS WAF would
   reintroduce the `$0.60/M` per-request cost the migration exists to remove.
6. **Logging.** With AWS WAF gone, origin-side forensics come from **API Gateway
   access logs + Lambda logs (CloudWatch)** + **Cloudflare sampled analytics**;
   existing CloudWatch 5xx/error alarms stay. WAF block/match logs are lost
   (Logpush raw export is Enterprise-only) — accepted.

`ch.sorobanscan` (Hetzner ClickHouse, Caddy mTLS on :443) stays **DNS-only
(grey-cloud)** — Cloudflare's proxy would terminate TLS and break the client-cert
mTLS, and the box needs a public IP + DNS for ACME HTTP-01 cert renewal anyway.

Cloudflare **SSL/TLS mode = Full (strict)** end-to-end — never Flexible.

DNS authority moves to Cloudflare by **changing the NS records of the delegated
`sorobanscan.rumblefish.dev` subdomain in the parent `rumblefish.dev` zone** (owned
outside this repo) — **not** a registrar change. This requires sign-off from the
parent-zone owner.

---

## Rationale

- **Flat cost beats per-request.** Cloudflare's edge (WAF + DDoS + challenge) is a
  flat plan price; AWS WAF is `$0.60/M` on top of fixed fees. Break-even is
  ~12M req/mo, and the abuse traffic this migration targets is precisely
  high-volume — the worst case for the metered model.
- **Unmetered DDoS on Free.** Cloudflare includes unmetered L3/4/7 DDoS mitigation
  on every plan; the AWS equivalent (Shield Advanced) is $3 000/mo.
- **mTLS is the strongest lockdown available without app changes.** A unique
  uploaded client cert (zone-level AOP) verified at the API Gateway handshake
  rejects every bypass path (raw `execute-api`, REGIONAL custom domain) at the TLS
  layer — pre-compute, no `crates/api` change, so the API-types codegen gate stays
  N/A. It is strictly stronger than global AOP's shared cert or an in-app secret.
- **Free is sufficient for a read-only public-data API.** Low SQLi/XSS surface; the
  Free Managed Ruleset + Managed Challenge + one rate-limit rule cover the real
  threat (scraping/floods). Pro is a no-migration flip if depth is later needed.

---

## Alternatives Considered

### Alternative 1: Keep AWS WAF (do nothing)

**Description:** Retain both WebACLs, optionally add Shield Advanced.

**Pros:**

- No DNS migration, no new vendor, no parent-zone dependency.

**Cons:**

- Per-request `$0.60/M` cost scales with the exact abuse traffic we want to stop.
- Real DDoS protection costs $3 000/mo (Shield Advanced).

**Decision:** REJECTED — fails the flat-cost and affordable-DDoS goals.

### Alternative 2: Keep one AWS WAF (API REGIONAL) alongside Cloudflare

**Description:** Drop only the CLOUDFRONT WebACL; keep the API one for signature-WAF
depth.

**Pros:**

- Retains full AWS managed-rule depth on the API.

**Cons:**

- Reintroduces `$0.60/M` per-request on the API — the highest-volume surface — so
  the flat-cost goal is not met.

**Decision:** REJECTED — Cloudflare's edge WAF covers this; Pro upgrade available
if more depth is needed.

### Alternative 3: Path A — IP-allowlist + `X-Origin-Secret` in the axum Lambda

**Description:** Lock the API by allowlisting Cloudflare IP ranges at API Gateway +
checking a secret header inside the Rust handler.

**Pros:**

- Flat-cost too; no mTLS cert machinery.

**Cons:**

- Enforces in **application code** (weaker than a TLS-handshake reject), couples
  lockdown to a deploy, and **trips the API-types codegen gate** (a `crates/api`
  change).

**Decision:** REJECTED as primary; **retained as fallback** if API GW mTLS proves
impractical.

### Alternative 4: Retire CloudFront, point Cloudflare at S3

**Description:** Serve the SPA straight from S3 via Cloudflare, drop CloudFront.

**Cons:**

- Larger change; loses CloudFront Functions / OAC setup already working.

**Decision:** REJECTED — keep CloudFront + secret-header lock (Decision 4a).

---

## Consequences

### Positive

- **Flat, predictable edge cost** independent of request volume.
- **Unmetered DDoS** on the Free plan.
- **Free Managed Challenge / Turnstile** raises the bot/scrape bar at no cost.
- Origins (`execute-api`, REGIONAL custom domain, `*.cloudfront.net`) are locked so
  they only answer Cloudflare — closes the direct-origin bypass vectors.
- No `crates/api` change → API-types codegen gate stays N/A.

### Negative

- **New external dependency** on the parent `rumblefish.dev` zone owner for the NS
  delegation change; rollback is bounded by parent-zone NS/SOA TTL (hours), not
  instant.
- **Request metadata transits Cloudflare** (data processor) — low sensitivity for
  public data; confirm no `x-api-key` ever rides in a URL.
- **WAF block/match logs are lost** (Logpush is Enterprise) — forensics fall back to
  API GW access logs + Lambda + Cloudflare sampled analytics.
- **Partner `x-api-key` callers** must egress through Cloudflare too — inventory
  required before cutover.
- **`ch.sorobanscan` gets zero Cloudflare protection** (stays grey-cloud for mTLS +
  ACME) — accepted risk, mitigated by host firewall + Caddy rate limiting.
- Re-creating a torn-down WebACL is a **fresh CDK deploy** (logs are
  `RemovalPolicy.DESTROY`), not a toggle.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md),
any ADR that changes the shape of the system MUST be landed together with the
corresponding updates to `docs/architecture/**`. Tick each that applies before
marking the ADR `accepted`:

- [ ] `docs/architecture/technical-design-general-overview.md` updated (or N/A) —
      `N/A until cutover` — overview names "WAF" generically; the edge-vendor swap
      is captured in infrastructure-overview. Re-confirm at Step 7.
- [x] `docs/architecture/database-schema/database-schema-overview.md` —
      `N/A — edge/DNS change does not touch schema`
- [x] `docs/architecture/backend/backend-overview.md` —
      `N/A — Path B mTLS = no crates/api change; backend contract unchanged`
- [x] `docs/architecture/frontend/frontend-overview.md` —
      `N/A — transparent Managed Challenge planned; revisit only if Turnstile widget added (Step 6)`
- [x] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` —
      `N/A — ingestion path unaffected`
- [x] `docs/architecture/infrastructure/infrastructure-overview.md` updated —
      forward-looking ADR-0048 pointers added to §5.4 (AWS WAF), §6.1, §6.3 while
      the WebACLs remain live; the present-tense topology rewrite lands with the
      Step 7 cutover (when this ADR → `accepted`).
- [x] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` —
      `N/A — unrelated`
- [x] This ADR is linked from each updated doc at the relevant section
      (infrastructure-overview §5.4 / §6.1 / §6.3).

> This ADR is `proposed`: the decision is made but the edge is **not yet deployed**.
> The present-tense topology rewrite of `infrastructure-overview.md` (and any
> overview-doc edit) lands in the implementation PR at Step 7, when status flips to
> `accepted`. Until then the doc carries forward-looking pointers and the live
> AWS WAF description stays accurate.

---

## References

- [docs/waf-vs-cloudflare/README.md](../../docs/waf-vs-cloudflare/README.md) — cost
  - capability comparison (verified 2026-06-01)
- [Cloudflare plans](https://www.cloudflare.com/plans/) — Free/Pro/Business tiers
- [AWS WAF pricing](https://aws.amazon.com/waf/pricing/) — per-request model
- [Authenticated Origin Pulls](https://developers.cloudflare.com/ssl/origin-configuration/authenticated-origin-pull/) — zone-level, own cert
- [API Gateway mTLS](https://docs.aws.amazon.com/apigateway/latest/developerguide/rest-api-mutual-tls.html) — custom-domain truststore
