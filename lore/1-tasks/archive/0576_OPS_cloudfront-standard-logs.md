---
id: '0576'
title: 'CloudFront standard logs for the delivery distribution, kept 30 days'
type: OPS
status: completed
related_adr: []
related_tasks: ['0519', '0577']
tags: ['infra', 'cloudfront', 'prices-api', 'effort-small']
links: []
history:
  - date: '2026-09-23'
    status: active
    who: stkrolikiewicz
    note: >
      Opened from the prices-api side. The Prices portal went public under
      /api/* on 2026-09-23, and nothing records its traffic. Starts active:
      the change is one bucket and one distribution property.
  - date: '2026-09-23'
    status: completed
    who: stkrolikiewicz
    note: >
      PR #485 merged (5b02933f). Delivery deployed from develop 9049e4e5 at
      11:56-11:58 UTC (108 s): LogBucket created, Distribution updated, nothing
      else changed. First log object arrived by 12:00 UTC with the three test
      requests, /api/ included. Two files changed (+33 lines), no tests
      touched. Archived.
---

# CloudFront standard logs for the delivery distribution, kept 30 days

## Summary

Turn on CloudFront standard (access) logging for the
`Explorer-production-Delivery` distribution, into a new private bucket,
`production-soroban-explorer-cf-logs`, whose objects expire after 30 days.

## Status: Completed

**Current state:** live on production since 2026-09-23 11:58 UTC; logs arriving.

## Context

- The distribution (`EA2TLS5SS5M87`) serves the explorer SPA and, since
  [[0519]], the Prices API portal under `/api` + `/api/*`. The portal has been
  public since 2026-09-23, when basic auth came off `/api/*`.
- Nothing recorded its requests. Checked 2026-09-23: `Logging.Enabled` was
  `false`, there were no real-time logs and no v2 delivery sources, and the
  portal's bucket had neither S3 server access logs nor request metrics. The
  explorer SPA loads Google Tag Manager, but the portal deliberately loads no
  third-party script, so Google Analytics never sees it.
- Standard logs are the one way to count portal entries without a script or a
  cookie. They see full page loads only (entry, refresh): navigation inside an
  SPA never reaches CloudFront.

## Implementation Plan

### Step 1: Log bucket and logging on the distribution

`delivery-stack.ts`: a bucket with `OBJECT_WRITER` ownership (legacy standard
logging delivers through ACLs, which `BUCKET_OWNER_ENFORCED` disables),
`BLOCK_ALL`, SSE-S3 and a 30-day expiration rule; `enableLogging` +
`logBucket` on the distribution. Cookies stay out of the log lines.

### Step 2: Docs

`docs/architecture/infrastructure/infrastructure-overview.md`, CloudFront CDN.

### Step 3: Deploy

Deploy Delivery once its diff shows only the new bucket and the
distribution's `Logging` block (see decision 3 for the command).

## Acceptance Criteria

- [x] The Delivery diff shows only the new bucket and the `Logging` block
- [x] Deployed; a request to `/api/` shows up in a log object in the bucket
- [x] The deployed bucket carries the 30-day expiration rule
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (CloudFront CDN)
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` changes

## Implementation Notes

- PR #485: `infra/src/lib/stacks/delivery-stack.ts` +25 (`LogBucket`,
  `enableLogging`, `logBucket`, header comment),
  `infrastructure-overview.md` +8. CI: TypeScript lint/build/typecheck green,
  Rust jobs skipped.
- `cdk diff --strict` of Delivery, on the branch and again on develop
  6cf9d467: `[+] LogBucket`, `[~] Distribution` (`Added: .Logging`),
  `CDKMetadata`. No IAM statement changes.
- After the deploy: distribution `Deployed`, `Logging.Enabled: true`,
  `IncludeCookies: false`. The bucket ACL carries CloudFront's log-delivery
  grant (canonical ID `c4c1ede6…`, `FULL_CONTROL`) beside the owner's. Lifecycle
  30 days `Enabled`, `ObjectWriter`, public access block all `true`. `/`,
  `/api/` and `/api/docs` answer 200 without credentials.
- First object: `EA2TLS5SS5M87.2026-09-23-11.45cf87ec.gz`, about two minutes
  after three requests with user agent `lore-0576-verify` — `GET /` 200 Miss,
  `GET /api/` 200 Miss, `GET /api/docs` 200 RefreshHit.
- A log line carries the path the viewer asked for (`/api/docs`), not the
  `/api/index.html` the routing function rewrites it to, so entries can be
  counted per portal page.

## Design Decisions

### From Plan

1. **Legacy standard logging to S3**, not v2 delivery or real-time logs: the
   only need is counting requests, and S3 storage is the whole cost.
2. **30-day expiry**: the lines carry viewer IPs, and the Prices portal's
   privacy policy keeps technical logs for up to 30 days.

### Emerged

3. **Deployed with `cdk deploy Explorer-production-Delivery --exclusively`**
   rather than `make -C infra deploy-production-delivery`. The Make target
   has no `--exclusively`, so CDK bundles every stack and cross-compiles the
   three Rust Lambdas (~10 min cold) for a deploy that ships none of them.
   Delivery depends on no other stack (`app.ts` passes it only `env` and
   `config`), so the deployed set is the same. The diff with the same flag took
   ~24 s.
4. **Deployed from `develop` on a laptop, not from a tag**: `master`
   (f29ef7bf, 2026-09-21) is 107 commits behind and still has
   `enableApiSpaBasicAuth: true`.
5. **No `logFilePrefix`**: the bucket holds this distribution's logs only.

## Notes

- A count that must outlive the logs has to be aggregated before they expire.
- The logs cover the whole distribution, explorer and portal alike. The portal
  is the `/api` paths.
- ⚠️ Until develop reaches master, a `-Delivery` or `-all` tag redeploys this
  stack from master: logging off and `/api` basic auth back on. A standard
  release tag does not deploy Delivery.
- The explorer's privacy policy ([[0577]]) lists IP, pages, referrer and time
  (§2.1), names AWS as a processor (§7) and fixes no period (§11), so 30 days
  fits. It makes "understand how the Service is used" subject to consent;
  reading explorer usage out of these logs is a question for that policy.
