---
id: '0553'
title: 'FEATURE: testnet environment — the same binaries against Stellar Testnet, own AWS stacks, a `testnet` database on the shared ClickHouse'
type: FEATURE
status: backlog
related_adr: ['0052']
related_tasks: ['0390', '0314', '0240', '0548']
tags:
  [
    priority-medium,
    effort-large,
    layer-infra,
    layer-frontend,
    testnet,
    clickhouse,
    ci-cd,
  ]
links:
  - infra/envs/production.json
  - infra/src/lib/types.ts
  - infra/src/lib/stacks/cicd-stack.ts
  - infra/src/lib/stacks/compute-stack.ts
  - crates/db-clickhouse/users.d/services.xml
  - infra/src/lib/stacks/ingestion-stack.ts
  - crates/backfill-runner/src/partition.rs
  - crates/backfill-runner/src/snapshot/archive.rs
history:
  - date: '2026-09-14'
    status: backlog
    who: stkrolikiewicz
    note: >
      Filed from ADR 0052 (proposed 2026-07-14), whose Implementation section
      ends in "own task, TBD". The seven phases below are its phases, after
      checking the repo for what is already configuration and what still
      hardcodes mainnet.
  - date: '2026-09-28'
    status: backlog
    who: karolkow
    note: >
      Re-checked on develop: all seven mainnet items still hold. Added five more
      (Galexie START ledgers, backfill-runner's own pubnet constants, reset
      detection, prices grant, degraded enrichment). Decided: backfill from the
      current testnet genesis via the public data lake, and alarm on a reset
      instead of relying on announcements. The "roughly quarterly" reset
      cadence was stale and is corrected.
  - date: '2026-09-29'
    status: backlog
    who: karolkow
    note: >
      Decided: mainnet stays on its own Galexie; testnet reads the public data
      lake once a week-long freshness measurement passes, then a ~2-month
      standing comparison against Galexie. Recorded the one-night measurement,
      the content check and the Galexie cost.
  - date: '2026-09-30'
    status: backlog
    who: karolkow
    note: >
      PRs A-C merged, D1-D2 open. Testnet Lambdas get their own ClickHouse
      users; the reader reads prices. Doorbell every 2 s from a one-hour lake
      measurement. Patches listed for a from-scratch sweep at the epic's end.
      Task turned into a directory.
---

# Testnet environment

## Summary

Deploy the explorer against **Stellar Testnet** as a second environment:
`Explorer-testnet-*` stacks built from the same source, a `testnet` database on
the shared ClickHouse box, `testnet.sorobanscan.rumblefish.dev`. Testnet reads
ledgers from SDF's public data lake, not from a Galexie of its own.
[ADR 0052](../../../2-adrs/0052_testnet-as-second-environment-and-staging.md)
fixed the shape; this task builds it.

## Status — 2026-09-30

| PR                         | Scope                                                                                                                                                                                                                                                             | State                                                                                            |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| A #543                     | ledger source, prefix, database from env; network guard                                                                                                                                                                                                           | merged                                                                                           |
| B #550                     | RPC pool into env config, `envName` widened                                                                                                                                                                                                                       | merged (Makefile `ENV=` and Galexie `START` not moved — not needed while testnet has no Galexie) |
| C #553                     | `testnet_reader` (SELECT `testnet.*`, `prices.*`) and `testnet_writer` (SELECT, INSERT `testnet.*`), quotas copied from `prices_read` / `prices_write`, existing profiles; `dev_read` reads `testnet.*`; no testnet admin — backfill uses the operator write cert | merged; needs a ClickHouse recreate                                                              |
| D1 #558 `[structure only]` | `ledgerSource: galexie \| public-lake`, optional prefix / database / `provisionChDns`; no VPC, bucket, Galexie or Galexie alarms with `public-lake`; test move in backfill-runner                                                                                 | open                                                                                             |
| D2 #559                    | genesis partition counted from ledger 2; indexer refuses a lake folder beside its own bucket; passphrase trimmed once (API and local server)                                                                                                                      | draft on D1                                                                                      |
| D3                         | sidecar applies `init.sql` to `default` and `testnet`; migration runbooks repeat each step on `testnet`                                                                                                                                                           | to do                                                                                            |
| D4                         | `testnet.json`, `bin/testnet.ts`, doorbell, stall alarm, low API Gateway throttle, docs, ADR 0052                                                                                                                                                                 | to do                                                                                            |
| E                          | TESTNET marker in the SPA                                                                                                                                                                                                                                         | later                                                                                            |

Decided 2026-09-30: Lambdas use their own certs mapped to the `testnet_*`
users — one project, but a misconfigured testnet cannot touch mainnet data,
and cert names already carry the environment. API host
`api-testnet-sorobanscan.rumblefishdev.com` behind Cloudflare with the same
Turnstile widget as mainnet and its own edge secret (each environment
generates one; corrected 2026-10-02); testnet alarms in their own Slack
channel.

## Patch register — sweep at the end of the epic

The epic ships with patches; its last step rebuilds each one in the shape we
would build from scratch — [notes/S-patch-register.md](notes/S-patch-register.md):
ledger source as scattered ifs in three infra files; optional config fields that
only make sense together; two env vars that can contradict plus a guard; a
network check that assumes pubnet for an own bucket.

## Acceptance Criteria

- [ ] `Explorer-testnet-*` deploys from `infra/envs/testnet.json` with no code
      branching by network, and `cdk diff` on production is empty after the
      refactor — the parameterisation must not move prod.
- [ ] Ledgers flow data lake → indexer → `testnet` database; a testnet
      transaction resolves on the testnet SPA under the hash Stellar RPC
      reports for it (passphrase → `network_id` is right).
- [ ] `testnet_*` users cannot read `default.*`, and `testnet_writer` cannot
      read `prices.*` (the 0314 RBAC check, repeated); both run on their own
      quotas.
- [ ] No testnet Lambda consults a mainnet RPC or archive (env audit of all
      three functions), and the SPA is visibly marked TESTNET.
- [ ] `backfill-runner` reads the testnet data lake and archive when run
      for testnet (items 8–9), and the `testnet` database holds the full
      history from the current genesis.
- [ ] `testnet-ingestion-stall` exists in CloudWatch
      (`aws cloudwatch describe-alarms`), is OK once the indexer runs, fires
      on a simulated stall (indexer paused, or the prefix pointed at a folder
      that no longer grows), and both state changes reach the testnet Slack
      channel.
- [ ] Patch sweep done: every item of the patch register rebuilt or
      explicitly kept with its reason.
- [ ] The reset runbook was exercised once end to end.
- [ ] The API is public like mainnet's (`docs/deployment.md` § Testnet, steps
      1–6): ACM cert, Terraform record (workspace `testnet`), rf-domains Transform
      Rule + Turnstile hostname, `enableCloudflareApiDomain`, `enableEdgeSecretLock`,
      `enableAuthLayer` all `true`; direct execute-api and lockless calls refused.
- [ ] **Docs updated** —
      `docs/architecture/infrastructure/infrastructure-overview.md` §7.1
      (environment model), `docs/architecture/security/clickhouse-rbac.md`
      (tenant matrix), `docs/deployment.md` ("No staging" + a testnet recipe),
      per [ADR 0032](../../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
      ADR 0052 moves `proposed → accepted` when this ships.
- [ ] **API types regenerated** — run if `crates/api/**` is touched (the
      archive prefix becoming env-driven); the diff is expected to be empty.

## Notes

- [R-mainnet-assumptions-in-code](notes/R-mainnet-assumptions-in-code.md) — the 12 places that said mainnet, reset cadence, lake folders
- [S-address-and-pr-split](notes/S-address-and-pr-split.md) — subdomains; the A-E split
- [R-ledger-source-and-doorbell](notes/R-ledger-source-and-doorbell.md) — lake vs Galexie evidence, 2 s doorbell, stall alarm
- [S-pr-a-verification-and-review](notes/S-pr-a-verification-and-review.md) — local testnet run, post-merge review of #543
- [I-implementation-phases](notes/I-implementation-phases.md) — ADR phases, protocol lead, notes
- [S-patch-register](notes/S-patch-register.md)
