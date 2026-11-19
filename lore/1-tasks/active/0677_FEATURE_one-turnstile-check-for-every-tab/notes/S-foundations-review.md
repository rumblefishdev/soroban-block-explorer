---
title: 'Foundations review: is every layer needed?'
type: synthesis
status: developing
spawns: []
tags: [review, premise]
links: []
history:
  - date: 2026-11-19
    status: developing
    who: karolkow
    note: 'Fresh opus review, asked by the owner to question everything.'
---

# Foundations review (2026-11-19)

Fresh opus reviewer, goal and constraints only, allowed to challenge owner
decisions. Checked by the main thread where marked.

| #   | Finding                                                                                                                                                                                                                                                | Evidence                                                                                   | Hits            |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------ | --------------- |
| B1  | The gate on every read is not required by any text we hold: the Tranche #3 clause lives only in the panel response (not in `stellar-scf-submissions`); the RFP asks for "two end-to-end example integrations"; stellar-x402 task 0041 says "To define" | `stellar-x402` `docs/rfp/07-x402-facilitator-bazaar.md:124`; task 0041                     | decision 1      |
| B2  | ADR 0048 itself says a browser-only API is not achievable (requests replay) and that the edge is "not to implement access control"                                                                                                                     | ADR 0048, Context                                                                          | decision 1      |
| B3  | One solve buys a portable 15-min JWT (no IP/UA binding) ≈ 9,000 reads at the Cloudflare limit of 100 / 10 s per IP; paid solvers ≈ $1–3 per 1,000 (estimate)                                                                                           | `crates/api/src/auth/jwt.rs:13-35`; soroban-infrastructure `cloudflare/variables.tf:35-45` | decision 1      |
| B4  | Hypothesis: search engines that render the SPA must pass Turnstile to see data; not checked (owner's Search Console)                                                                                                                                   | `cloudflare/zone.tf` (Search Console TXT); no SSR in `web/`                                | decision 1      |
| B5  | API Gateway usage plan + partner key gates nothing (`apiKeyRequired=false` on `{proxy+}`); **measured** 0 usage on `production-partner-plan` 2026-10-21..11-19                                                                                         | `infra/src/lib/stacks/api-gateway-stack.ts:159-163`; `aws apigateway get-usage`            | none            |
| B6  | Two CORS truths (API Gateway preflight MOCK + axum `CorsLayer`); axum can answer preflights alone (~100 more Lambda calls a day)                                                                                                                       | `api-gateway-stack.ts:128-147`; `crates/api/src/main.rs:149-159`                           | none            |
| B7  | Dead config: mTLS truststore branch, legacy and rumblefishdev.com API domains, `apiGatewayCacheTtlImmutable` never read                                                                                                                                | `infra/envs/*.json`; `infra/src/lib/types.ts:114`                                          | 0678 keeps mTLS |
| B8  | Cookie as the only carrier: a browser blocking all cookies reads nothing (works per tab today). Fallbacks: cookie + Bearer (mint also returns the token), or `localStorage` JWT + memory + Bearer (frontend only, ~15 lines)                           | derived from decision 6                                                                    | decisions 4, 6  |
| B9  | Kept: Cloudflare WAF + rate limit, edge-secret lock, stage throttle (global cap, not per IP)                                                                                                                                                           | —                                                                                          | —               |
| B10 | Rejected: same-origin `/v1` on CloudFront (bypasses Cloudflare or breaks per-IP limits), Turnstile pre-clearance (cross-origin challenge has no CORS header)                                                                                           | derived, untested                                                                          | —               |
