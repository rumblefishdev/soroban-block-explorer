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
  - date: '2026-10-02'
    status: backlog
    who: karolkow
    note: >
      Code merged except E (TESTNET marker); operator session done (certs,
      Slack, ClickHouse users, testnet database). Status rewritten.
---

# Testnet environment

## Summary

Deploy the explorer against **Stellar Testnet** as a second environment:
`Explorer-testnet-*` stacks built from the same source, a `testnet` database on
the shared ClickHouse box, `testnet.sorobanscan.rumblefish.dev`. Testnet reads
ledgers from SDF's public data lake, not from a Galexie of its own.
[ADR 0052](../../../2-adrs/0052_testnet-as-second-environment-and-staging.md)
fixed the shape; this task builds it.

## Status — 2026-10-02

| Step                                       | Scope                                                                                                                                                        | State  |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------ |
| A #543, B #550, C #553                     | ledger source and database from env; RPC pool in env config; `testnet_*` ClickHouse users and quotas                                                         | merged |
| D1 #558, D2 #559, D3 #562, D4 #566, F #567 | CDK ledger source; genesis from ledger 2; sidecar applies `init.sql` to both databases; `testnet.json`, keepalive, stall alarm, runbook; self-pacing indexer | merged |
| #592, #594 `[structure only]`, P2 #597     | moves; an empty lake database starts at ledger 2                                                                                                             | merged |
| E #600                                     | TESTNET marker: a "TESTNET ▾" pill beside the logo that is also the network menu; testnet tab title and icons                                                | merged |

**Operator session** — done 2026-10-02:
[notes/S-operator-session-2026-10-02.md](notes/S-operator-session-2026-10-02.md).

**Launch** — from 2026-10-06: backfill → deploy 1 → Terraform API record →
rf-domains rule + Turnstile → deploy 2 and web → resume indexer → checks.
Then P1–P5, tasks 0603 and 0609, the patch sweep, ADR 0052 → accepted.

Certs, API host and alarm channel decisions (2026-09-30):
[notes/S-address-and-pr-split.md](notes/S-address-and-pr-split.md).

## Patch register — sweep at the end of the epic

Each patch is rebuilt in its from-scratch shape at the end:
[notes/S-patch-register.md](notes/S-patch-register.md).

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
- [S-operator-session-2026-10-02](notes/S-operator-session-2026-10-02.md) — what the operator session changed, and its lessons
