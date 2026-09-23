---
id: '0576'
title: 'CloudFront standard logs for the delivery distribution, kept 30 days'
type: OPS
status: active
related_adr: []
related_tasks: ['0519']
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
---

# CloudFront standard logs for the delivery distribution, kept 30 days

## Summary

Turn on CloudFront standard (access) logging for the
`Explorer-production-Delivery` distribution, into a new private bucket,
`production-soroban-explorer-cf-logs`, whose objects expire after 30 days.

## Status: Active

**Current state:** code on `ops/0576_cloudfront-standard-logs`.

## Context

- The distribution (`EA2TLS5SS5M87`) serves the explorer SPA and, since
  [[0519]], the Prices API portal under `/api` + `/api/*`. The portal has been
  public since 2026-09-23, when basic auth came off `/api/*`.
- Nothing records its requests. Checked 2026-09-23: `Logging.Enabled` is
  `false`, there are no real-time logs and no v2 delivery sources, and the
  portal's bucket has neither S3 server access logs nor request metrics. The
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

`make -C infra deploy-production-delivery`, once the Delivery diff shows only
the new bucket and the distribution's `Logging` block.

## Acceptance Criteria

- [ ] The Delivery diff shows only the new bucket and the `Logging` block
- [ ] Deployed; a request to `/api/` shows up in a log object in the bucket
- [ ] The deployed bucket carries the 30-day expiration rule
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (CloudFront CDN)
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` changes

## Notes

- 30 days, not longer: log lines carry viewer IP addresses, and the Prices
  portal's privacy policy (served from this distribution) says technical logs
  are generally kept for up to 30 days. A count that must outlive the logs has
  to be aggregated before they expire.
- The logs cover the whole distribution, explorer and portal alike. The portal
  is the `/api` paths.
