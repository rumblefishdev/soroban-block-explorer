---
id: '0277'
title: 'FEATURE: Migrate edge protection (WAF/DDoS) to Cloudflare'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0273']
tags:
  [
    priority-medium,
    effort-large,
    layer-infra,
    security,
    waf,
    cloudflare,
    ddos,
    dns,
    edge,
  ]
links:
  - docs/waf-vs-cloudflare/README.md
  - infra/src/lib/stacks/api-gateway-stack.ts
  - infra/src/lib/stacks/delivery-stack.ts
  - infra/src/lib/stacks/cloudfront-waf-stack.ts
  - infra/src/lib/stacks/hetzner-dns-stack.ts
  - infra/src/lib/constructs/waf-web-acl.ts
history:
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      Spawned after team decision (daily 2026-06-01) to move edge protection
      from AWS WAF to Cloudflare. Rationale + cost/capability comparison in
      docs/waf-vs-cloudflare/README.md.
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      Hardened after 5 independent senior-DevOps/security reviews. Folded in:
      concrete origin-lockdown mechanism (IP allowlist AND secret header, with a
      named enforcement point) + all bypass vectors, corrected DNS-delegation
      model (subdomain NS in parent rumblefish.dev zone, not a registrar),
      corrected mTLS rationale + CH-IP accepted-risk, secrets handling for a
      PUBLIC repo, mandatory TLS Full(strict), safe step ordering, soak gate,
      observability replacement, a negative-test matrix, and realistic rollback.
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      Plan-tier decision set to START ON FREE (Decision 1), upgrade to Pro only
      when full edge managed ruleset / 2nd rate-limit rule / finer bot control is
      needed (a plan flip, no DNS/re-migration). Free covers the primary goal:
      unmetered DDoS + free Managed Challenge + Transform Rules; API signature-WAF
      coverage retained via the kept REGIONAL AWS WAF. Pre-condition flagged:
      verify Transform Rules fit Free's rule budget (origin lockdown depends on it).
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      Decisions resolved by operator: D4 → 4a (keep CloudFront, lock the
      *.cloudfront.net domain via a viewer-request CloudFront Function checking
      the secret header). D2 → Terraform cloudflare provider, state in S3 with
      native lockfile, `apply` run by an operator locally to start (migrate to CI
      later), TF code in infra/cloudflare/, zone-scoped token in SSM/Secrets
      Manager. Remaining to ratify: D3 (WAF header enforcer), D5 (keep API WAF /
      drop only CLOUDFRONT WebACL), D6 (logging). External prerequisite:
      parent rumblefish.dev zone owner sign-off for the NS delegation change.
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      Reconciled to the flat-cost goal (edge cost independent of API traffic):
      D5 → drop BOTH AWS WebACLs (enableWaf:false) — keeping any AWS WAF
      reintroduces $0.60/M per-request. D3 → origin lockdown without AWS WAF, two
      flat options: (A) verified-on-Free IP-allowlist + X-Origin-Secret checked in
      the axum Lambda (trips API-types gate); (B) Authenticated Origin Pulls mTLS
      on the API GW custom domain (pending verify of AOP-on-Free + custom — not
      shared — cert). D6 → forensics from API GW access logs + Lambda + Cloudflare
      analytics (WAF logs gone). Step 2/7 + AC updated accordingly. Honest limits
      restated: protection layer flat, but API GW + Lambda still scale with real
      traffic; "browser-only" not achievable — only bot/scrape bar-raising.
  - date: '2026-06-01'
    status: backlog
    who: fmazur
    note: >
      ALL decisions closed. D3 → Path B (mTLS): Cloudflare zone-level AOP with own
      client cert + API GW mTLS (S3 truststore) + disableExecuteApiEndpoint —
      verified Free-viable + no extra API GW charge (2026-06-01); strong (unique
      cert, not global AOP shared cert), pre-compute, no crates/api change →
      API-types stays N/A. SPA/CloudFront keeps the secret-header lock (4a).
      D6 → default (API GW access logs + Lambda + CF analytics). Only non-decision
      items remain: parent rumblefish.dev NS sign-off (external), partner x-api-key
      inventory, staging dry-run. Task is decision-complete & ready to promote.
  - date: '2026-06-01'
    status: active
    who: fmazur
    note: >
      Promoted to active. All 6 decisions resolved + Free-tier / Universal SSL /
      AOP-on-Free verified against Cloudflare docs. Planning docs committed under
      docs/waf-vs-cloudflare/ (comparison EN/PL + certs & flow). Remaining before
      cutover are non-decision items: parent rumblefish.dev NS sign-off, partner
      x-api-key inventory, staging dry-run.
  - date: '2026-06-02'
    status: active
    who: fmazur
    note: >
      Step 1 progress: ADR 0048 (Cloudflare edge over AWS WAF, origins locked to
      Cloudflare) written as `proposed` and cross-linked (related_adr). Forward-
      looking ADR-0048 pointers added to docs/architecture/infrastructure/
      infrastructure-overview.md (§5.4 AWS WAF, §6.1, §6.3) — live WebACL
      description kept accurate; present-tense topology rewrite deferred to the
      Step 7 cutover when the ADR flips to `accepted`. Still outstanding in Step 1:
      external parent-zone NS sign-off.
  - date: '2026-06-03'
    status: active
    who: fmazur
    note: >
      MAJOR re-scope after deploy hit Cloudflare error 1116 (Free/Pro cannot proxy
      a bare subdomain; partial/CNAME = Business $200, subdomain-zone = Enterprise).
      New plan (daily + senior): move the spare company domain rumblefishdev.com to
      Cloudflare Free (full zone), API becomes api.sorobanscan.rumblefishdev.com;
      rumblefish.dev stays on Route53, untouched. Scope narrowed to API only (SPA
      stays S3/CloudFront + Shield Std). Repo split: company-level (zone + company
      DNS + zone settings + edge rulesets + TF-state bucket) goes to a NEW private
      repo `rf-domains`; sorobanscan-specific (api DNS record + AWS-side origin
      lock) stays here (infra/cloudflare/ + CDK). Ruleset ownership = model A
      (zone-owner rf-domains owns the per-phase rulesets, each rule http.host-scoped
      to the API; reversible to single-tenant model C via terraform state rm+import).
      soroban keeps its OWN TF-state bucket here (the deployed
      production-soroban-explorer-cf-tfstate via CloudflareBootstrapStack); rf-domains
      gets a separate bucket. Origin lock under the split prefers mTLS (per-host AOP
      owned by soroban + API GW mTLS) over secret-header (which would force a
      cross-repo shared secret + the transform rule into rf-domains). Dead
      rumblefishdev.com records (do NOT recreate): contact, wag-api-staging,
      gitlab-test. See full decision block in the body + docs/waf-vs-cloudflare/.
---

# Migrate edge protection (WAF/DDoS) to Cloudflare

## Summary

Team decided (daily 2026-06-01) to front the explorer with **Cloudflare** instead of the two
AWS WAF WebACLs. Put Cloudflare's edge (WAF + unmetered DDoS + Managed Challenge / Turnstile +
rate limiting) in front of the public CloudFront SPA and the public **API Gateway** (data API),
then **lock the AWS origins so they only accept Cloudflare traffic**. Primary goal: stop trivial
scraping/abuse of the public data API and get real DDoS protection; secondary: flat/predictable
cost and a free browser challenge. Rationale + verified cost model: [`docs/waf-vs-cloudflare/README.md`](../../../docs/waf-vs-cloudflare/README.md).

> The data is public on-chain data served to anonymous browsers, so "browser-only" is **not**
> achievable (requests are replayable). This raises the bar against bots/floods; it is not access control.

## ⚠️ Decyzje — aktualizacja 2026-06-03 (model A + split na `rf-domains`)

Po uderzeniu w **Cloudflare error 1116** (Free/Pro **nie proxuje gołej poddomeny**;
partial/CNAME = Business $200, subdomain-zone = Enterprise — 3× zweryfikowane) plan się
zmienił. Nadrzędne nad starszymi „Decisions 1–6" poniżej tam, gdzie kolidują.

- **D7 — Hostname + strefa.** API ląduje na **`api.sorobanscan.rumblefishdev.com`** za
  **Cloudflare Free (cała strefa `rumblefishdev.com`)** — zapasowej domeny firmy (dziś tylko
  redirect → `rumblefish.dev`). **`rumblefish.dev` zostaje na Route53, nietknięta.**
  SPA (`sorobanscan.rumblefish.dev`) i `ch.` **bez zmian**.
- **D8 — Scope = tylko API.** SPA jest statyczne (S3/CloudFront + Shield Standard), niska
  powierzchnia, challenge na froncie user-hostile → **lock SPA odpada**; litera taska (SPA) tu
  zawężona.
- **D9 — Split repo.** **Firmowe** (strefa + rekordy `rumblefishdev.com` + zone settings + **rulesety
  edge** + **bucket na TF-state**) → **NOWE prywatne repo `rf-domains`** (CloudFormation na bucket
  tam, nie u nas). **Sorobanowe** (rekord `api.sorobanscan` + **AWS-side origin lock**) → **to repo**
  (`infra/cloudflare/` + CDK). Cel: firmowy DNS nie żyje w repo sorobanscana.
- **D10 — Własność rulesetów = model A.** Rulesety to **per-(strefa,faza) singletony** → jeden
  właściciel TF. **Posiada je właściciel strefy (`rf-domains`)**, a każda reguła jest
  **`http.host`-scoped do `api.sorobanscan.rumblefishdev.com`**. Skaluje się na wiele projektów w
  jednej strefie; **odwracalne** do single-tenant (model C) przez `terraform state rm` +
  `terraform import` (bez niszczenia/odtwarzania, bez downtime).
- **D11 — State.** Sorobanowy Cloudflare TF ma **własny bucket tutaj** (już zdeployowany
  `production-soroban-explorer-cf-tfstate` przez `CloudflareBootstrapStack`). `rf-domains` ma
  **osobny** bucket. **Bucketu NIE niszczymy.**
- **D12 — Origin lock pod splitem → mTLS.** Preferowane **mTLS** (per-host **AOP** posiadane przez
  soroban + **API GW mTLS** truststore w CDK + `disableExecuteApiEndpoint`), bo jest **samo-zawarte
  w soroban** (cert + truststore). **Secret-header odpada pod splitem** — wymusiłby (a) Transform
  Rule jako zone-ruleset → do `rf-domains`, (b) **sekret współdzielony między repo**. Dokładny zakres
  AOP (zone vs per-host na Free) potwierdzić w **dry-run na staging** (Step 3).
- **Martwe rekordy `rumblefishdev.com`** (NIE odtwarzać w CF): **contact, wag-api-staging, gitlab-test**.
- **⚠️ DNS.** Pełne przejście strefy `rumblefishdev.com` → **wszystkie** rekordy odtworzyć w CF
  przed flipem NS (mechanizm redirectu, MX/SPF/DKIM/DMARC, **CAA musi dopuszczać CF**); pre-obniżyć
  TTL; potwierdzić, kto zmienia **NS u rejestratora**. Inwentaryzacja z żywej strefy (account
  firmowy), nie z pamięci.

> Diagram + tabela domen: [`docs/waf-vs-cloudflare/diagram-komunikacji-edge.md`](../../../docs/waf-vs-cloudflare/diagram-komunikacji-edge.md);
> analiza opcji A–G: [`docs/waf-vs-cloudflare/decyzja-edge-poddomena.md`](../../../docs/waf-vs-cloudflare/decyzja-edge-poddomena.md).

## Current state (verified against the code)

- **API Gateway** — `LambdaRestApi`, `proxy: true`, `REGIONAL`, `apiKeyRequired=false`
  (`api-gateway-stack.ts:45-67`). **`disableExecuteApiEndpoint` is NOT set** → the raw
  `https://{id}.execute-api.eu-central-1.amazonaws.com/production` URL answers directly. Custom
  domain `api.sorobanscan…` + Route 53 A/AAAA (`:131-167`). Usage plan + API key exist but do
  **not** gate (proxy mode). No resource policy today.
- **CloudFront SPA** — S3 origin is **already private via OAC** (`delivery-stack.ts:52-61,200`,
  `BLOCK_ALL`). The exposed surface is the **distribution domain `*.cloudfront.net`**, not S3.
- **`ch.sorobanscan`** — A record to a **public Hetzner IPv4** (from SSM `/soroban/production/ch-ip`,
  `hetzner-dns-stack.ts:30-77`). Caddy terminates **client-cert mTLS on :443** and renews its
  public cert via **ACME HTTP-01** (needs :80 + public DNS reaching the box).
- **DNS** — the delegated `sorobanscan.rumblefish.dev` **hosted zone** (Route 53). It has **no
  registrar**; authority is set by **NS records in the parent `rumblefish.dev` zone** (owned
  outside this repo).
- AWS WAF log group uses `RemovalPolicy.DESTROY` (`waf-web-acl.ts:58-62`) → tearing WAF down
  deletes the WebACLs **and** their logs; "re-enable" is a fresh deploy, not a toggle.

## Bypass vectors that origin lockdown MUST close

Any of these reaches the origin **skipping Cloudflare** (no WAF/challenge/rate-limit/DDoS):

1. Raw API GW `execute-api` URL.
2. The **REGIONAL custom-domain** API GW endpoint (answers regardless of the Cloudflare DNS record).
3. The CloudFront **`*.cloudfront.net`** distribution domain.
4. (`ch.sorobanscan` is intentionally NOT behind Cloudflare — see accepted risk.)

## Decisions (all ✓ resolved 2026-06-01 — only external sign-off + data-gathering remain, see below)

1. **[✓ RESOLVED] Plan tier** → **START ON FREE**, upgrade to **Pro** later only if/when needed. Free already
   covers the primary goal: **unmetered DDoS** + **Managed Challenge / Turnstile** (the anti-bot
   lever — free on all plans) + Transform Rules for the origin-secret. Signature-WAF depth on Free
   is the limited **Free Managed Ruleset** only (AWS WAF is dropped — D5), which is acceptable here:
   the API is **read-only over public data** (low SQLi/XSS surface) and full OWASP is available via
   a flat **Pro** upgrade later if needed. Design around Free's constraints: **1 rate-limit rule** (put it on the API; defer
   the SPA), only the limited **Free Managed Ruleset** at the edge, and **Bot Fight Mode** (coarser
   than Pro's Super Bot Fight Mode). **Upgrade to Pro** ($20/mo annual, $25/mo monthly) when you
   want the full Cloudflare Managed Ruleset + OWASP at the edge, a 2nd rate-limit rule (e.g. a
   separate SPA rule), or finer bot control — it's a **plan flip with no DNS change / no
   re-migration**, fully additive.
   **VERIFIED 2026-06-01 (Cloudflare docs):** both lockdown legs are Free-viable —
   **zone-level Authenticated Origin Pulls** (own client cert, for the **API** mTLS lock) is on
   Free, and **Request-Header Transform Rules** (`X-Origin-Secret` for the **SPA/CloudFront** lock)
   are on Free (10-rule shared budget, static value needs no regex). Day-1 Pro is not required. Free constraints to design around: the **single** rate-limit rule can match only
   **Path + Verified Bot**, count by **IP**, with a fixed **10 s** window (coarser than AWS's
   2 000/5 min); Free WAF managed rules = the limited **Free Managed Ruleset** only; **Bot Fight
   Mode** (not Super). Design check (NOT a blocker): forwarding to the AWS origins needs **no
   Host-header rewrite** (Host/SNI override is Enterprise-only) because API GW custom domain +
   CloudFront alternate-domain are configured for the public hostnames Cloudflare forwards —
   confirm in Step 2.
2. **[✓ RESOLVED] IaC** → **Terraform `cloudflare` provider** (not the dashboard). State in **S3
   (versioned, encrypted) with the native S3 lockfile**. `apply` run **by an operator locally to
   start**, migrate to CI/GitHub Actions once stable (OIDC already exists in `cicd-stack`). TF code
   lives in **`infra/cloudflare/`**. Cloudflare API token **zone-scoped, least-privilege** (never
   the Global API Key), in SSM/Secrets Manager — never in `.tf` or committed state. Drift checked
   via a periodic `terraform plan`.
3. **[✓ RESOLVED → Path B, mTLS] Origin-lockdown enforcement** → **no AWS WAF** (flat cost). Lock the
   API at the TLS layer: **Cloudflare zone-level Authenticated Origin Pulls** with **your OWN**
   uploaded client cert (unique → strong; **not** global AOP's shared cert) **+ AWS API Gateway
   mTLS** on the REGIONAL custom domain (truststore = your CA bundle in S3) **+
   `disableExecuteApiEndpoint: true`** (the default endpoint bypasses custom-domain mTLS).
   **Verified Free-viable + no extra API GW charge (2026-06-01).** Origin rejects any non-Cloudflare
   cert **at the handshake** — pre-compute, **no `crates/api` change** (API-types gate stays N/A).
   API GW does **not** check revocation → add a Lambda authorizer only if CRL/OCSP is required.
   _(Fallback, NOT chosen: Path A = IP-allowlist + `X-Origin-Secret` checked in the axum Lambda —
   flat too, but enforces in app code and is weaker than a unique cert.)_
   Note: **CloudFront/SPA** can't do viewer mTLS → it stays on the secret-header lock (Decision 4a).
4. **[✓ RESOLVED → 4a] CloudFront fate** → **keep CloudFront**; lock the `*.cloudfront.net` domain
   to Cloudflare via a **viewer-request CloudFront Function** checking the secret header. (Option b
   — point Cloudflare at S3 and retire CloudFront — rejected: bigger change.)
5. **[✓ RESOLVED → drop both] AWS WAF teardown** → **`enableWaf:false`** — drop **both** WebACLs
   (REGIONAL API + CLOUDFRONT). Keeping any AWS WAF reintroduces the per-request `$0.60/M` cost the
   migration exists to remove (flat-cost goal); lockdown moves to the D3 enforcer, the edge WAF is
   Cloudflare's. Do the teardown **only after** the Step 7 soak confirms Cloudflare + lockdown stable.
6. **[✓ RESOLVED] Logging** → with the AWS WAF gone, origin-side forensics come from **API Gateway
   access logs + Lambda logs in CloudWatch** (keep/enable them) + **Cloudflare sampled analytics**
   at the edge; existing CloudWatch 5xx/error alarms stay. Accept loss of WAF block/match logs
   (Logpush raw export is Enterprise).

## Implementation Plan (ordered for safety — lock before cutover)

### Step 1 — Decision + ADR + parent-zone dependency

Resolve the decisions above; write an ADR (`lore/2-adrs/_template.md`) for "Cloudflare edge over
AWS WAF" and link it. **Identify and get sign-off from the owner of the parent `rumblefish.dev`
zone** (NS delegation change is theirs). Update `docs/architecture/**` topology.

> **⚠️ SUPERSEDED by D7 (2026-06-03).** The parent-`rumblefish.dev`-zone NS-delegation model
> below was for the abandoned "proxy `sorobanscan.rumblefish.dev`" plan. Under **D7** we move the
> **whole spare apex `rumblefishdev.com`** to Cloudflare (full zone), so authority flips via a
> **registrar NS change on `rumblefishdev.com`** — NOT a parent-zone delegation, and **not** owned
> by the `rumblefish.dev` zone owner. The Step-1 dependency is therefore: **who controls the
> `rumblefishdev.com` registrar + its live Route 53 records** (company account). `rumblefish.dev`
> stays untouched.

### Step 2 — Build origin lockdown BEFORE any DNS change (no AWS WAF) — Path B (mTLS), chosen

Enforce that the origin accepts **only Cloudflare** at the TLS layer:

- **API (mTLS):** enable Cloudflare **zone-level Authenticated Origin Pulls** with **your own**
  uploaded client cert (unique — not global AOP's shared cert). On AWS, enable **mTLS** on the
  **REGIONAL custom domain** with a **truststore in S3** (the CA that signed that client cert); set
  **`disableExecuteApiEndpoint: true`** (confirm the custom-domain base-path mapping is live FIRST,
  else you 403 your own edge). API GW rejects any non-Cloudflare cert at the handshake — pre-compute,
  no `crates/api` change. (API GW doesn't check revocation → Lambda authorizer only if CRL/OCSP needed.)
- **SPA/CloudFront (Decision 4a):** CloudFront can't do viewer mTLS → keep the **secret-header**
  lock: a Cloudflare **Transform Rule** injects `X-Origin-Secret`, a **viewer-request CloudFront
  Function** checks it. Store the secret out of git (SSM/Secrets Manager), rotate **dual-accept**,
  **never log it**.
- Cloudflare **SSL/TLS = Full (strict)** — never Flexible.
- _(Fallback if API mTLS proves impractical: Path A = API GW IP-allowlist + `X-Origin-Secret`
  checked in the axum Lambda — flat, but in-app and weaker; would re-introduce the API-types gate.)_

### Step 3 — Staging dry-run

Rehearse the whole flow (zone, proxy, lockdown, challenge) on the **staging** zone first. Validate
the negative-test matrix there before touching production.

### Step 4 — Cloudflare zone + DNS migration (production)

> **⚠️ SUPERSEDED by D7/D8 (2026-06-03).** Scope is now **API-only on a different apex**:
> create the **`rumblefishdev.com`** full zone (owned by **`rf-domains`**), recreate **ALL** its
> live records there (mail/SPF/DKIM/DMARC, CAA-for-CF, apex redirect — inventory the company
> Route 53 zone first), proxy **only** `api.sorobanscan.rumblefishdev.com`, then flip authority via
> a **registrar NS change on `rumblefishdev.com`** (whole apex → Cloudflare; full setup is the only
> Free/Pro option). **SPA `sorobanscan.rumblefish.dev` and `ch.` stay on Route 53, untouched** — no
> parent-`rumblefish.dev` NS change at all. The Route 53 reconciliation below applies only to the
> **API** record set, not the SPA/CH ones. The original sub-bullets are kept for historical context:

- Pre-lower TTLs on the affected records **days ahead**.
- Create the Cloudflare zone; recreate records. **Proxy (orange)** `sorobanscan` + `api.sorobanscan`;
  **keep `ch.sorobanscan` DNS-only (grey)** — Cloudflare's proxy terminates TLS and won't pass the
  client cert through to Caddy, so proxying would break mTLS (it runs on :443, there is no special
  port). Flip authority via **NS records in the parent `rumblefish.dev` zone** (or use a Cloudflare
  partial/CNAME setup if available) — **not** a registrar change.
- **Reconcile CDK-owned Route 53 records**: the `ARecord`s **and `AaaaRecord`s** in
  `delivery`/`api`/`hetzner` stacks become orphaned/conflicting once Cloudflare is authoritative.
  Decide their fate (remove from CDK, or keep Route 53 as a non-authoritative copy) so CDK does
  not fight Cloudflare.

### Step 5 — Edge security config

Cloudflare WAF: the **Free Managed Ruleset** to start (full Cloudflare Managed + OWASP after a Pro
upgrade); the **single Free rate-limit rule placed on the API** replacing the AWS 2 000/5 min
equivalent (SPA rate-limit deferred until Pro); and a **Managed Challenge scoped to suspicious
traffic** on the API. **Do not blanket-challenge JSON/XHR** — verify the SPA's `fetch()` calls and
the existing `x-api-key` partner path survive (Managed Challenge can return an HTML interstitial
that breaks API clients).

### Step 6 — Frontend (only if needed)

If interactive Turnstile is used, integrate the widget + token into API calls. If a scoped Managed
Challenge suffices transparently, no frontend change.

### Step 7 — Verify (negative-test matrix), soak, then drop AWS WAF

Run the test matrix (below). **Soak** for an agreed window. Only then **`enableWaf:false`** to drop
**both** WebACLs (per D5). Coordinate basic-auth removal with task 0273 when the API read-path
(0243) goes live.

## Acceptance Criteria

- [ ] Decisions 1–6 resolved + ADR written/linked; parent-zone owner signed off
- [ ] Origin secret + Cloudflare API token stored **out of git** (SSM/Secrets Manager), token
      zone-scoped least-privilege; origin secret **never committed / never logged**
- [ ] Cloudflare **SSL/TLS = Full (strict)** confirmed end-to-end
- [ ] `disableExecuteApiEndpoint: true` set; **API locked via mTLS** (zone-level AOP + API GW mTLS,
      S3 truststore); **SPA/CloudFront locked via secret-header** CloudFront Function. **No AWS WAF.**
- [ ] **Negative-test matrix passes** (all return 403 / blocked):
  - [ ] direct `execute-api` URL
  - [ ] direct REGIONAL custom-domain endpoint
  - [ ] direct `*.cloudfront.net` domain
  - [ ] request lacking the lockdown proof (missing/wrong `X-Origin-Secret`, or missing client cert under mTLS)
- [ ] **Positive paths work**: SPA loads + its `fetch()` API calls succeed through the challenge;
      `x-api-key` partner path still works; `ch.sorobanscan` mTLS handshake still succeeds
- [ ] **Caddy cert renewal verified** post-cutover (force a renewal — `ch` stayed grey-cloud so
      ACME HTTP-01 still resolves)
- [ ] CDK Route 53 records reconciled (no CDK-vs-Cloudflare DNS conflict)
- [ ] Observability: API GW access logs + Lambda logs in CloudWatch (WAF logs gone) + alarms on
      Cloudflare challenge/block rate + a recurring synthetic check that direct-origin stays blocked
- [ ] Rollback rehearsed (see below)
- [ ] **Docs updated** — `docs/architecture/**` topology reflects the Cloudflare edge; ADR added
- [ ] **API types regenerated** — **N/A** (Path B mTLS = no `crates/api` change; the SPA header lock
      is edge/CloudFront-Function config, not app code).

## Rollback

> **⚠️ SUPERSEDED by D7 (2026-06-03).** Rollback now means reverting the **`rumblefishdev.com`
> registrar NS** back to its previous provider (keep the old Route 53 records for that apex intact
> to revert to) — **bounded by the registrar NS/SOA TTL (can be hours), not instant**; this is why
> TTLs are pre-lowered. **`rumblefish.dev` is never touched, so the SPA/CH need no rollback.**
> Re-creating any torn-down WebACL is still a **fresh CDK deploy** (logs were `DESTROY`-removed),
> not a toggle — budget for it. (Original text below kept for context.)

Revert NS records in the parent `rumblefish.dev` zone back to Route 53 — **bounded by parent-zone
NS/SOA TTL (can be hours), not instant**; this is why TTLs are pre-lowered. Keep the Route 53 zone
intact during cutover. Re-creating any torn-down WebACL is a **fresh CDK deploy** (logs were
`DESTROY`-removed), not a toggle — budget for it.

## Risks / accepted risks

- **`ch.sorobanscan` gets ZERO Cloudflare protection** — its box IP stays public in DNS (required
  for mTLS + ACME). Accepted risk; mitigate with a **host firewall** (allow only :443 + SSH from
  known IPs) and Caddy-level rate limiting / fail2ban. mTLS remains the access control.
- **Origin-IP/secret leak surface** — Full(strict) TLS + no-log of the secret header are mandatory,
  not optional.
- **Third-party log transit** — request metadata (IPs, paths) now transits Cloudflare; low
  sensitivity for public data, but confirm no `x-api-key` ever rides in a URL. Note in the ADR
  (Cloudflare as data processor).
- **Partner `x-api-key` callers must egress through Cloudflare too** — once the origin is locked to
  CF, any partner/automation hitting the API directly (not via `api.sorobanscan…`) breaks.
  **Inventory partner callers** before cutover and confirm they go through the proxied hostname.
- **DNS cutover** is the highest-risk step (parent-zone dependency + propagation) — staging dry-run
  - pre-lowered TTLs first.

## Out of scope

- ML Bot Management (Cloudflare Enterprise) and Logpush raw-log export (Enterprise) — not in this task.
