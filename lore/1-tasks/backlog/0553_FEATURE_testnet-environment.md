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
history:
  - date: '2026-09-14'
    status: backlog
    who: stkrolikiewicz
    note: >
      Filed from ADR 0052 (proposed 2026-07-14), whose Implementation section
      ends in "own task, TBD". The seven phases below are its phases, after
      checking the repo for what is already configuration and what still
      hardcodes mainnet.
---

# Testnet environment

## Summary

Deploy the explorer against **Stellar Testnet** as a second environment:
`Explorer-testnet-*` stacks built from the same source, a `testnet` database on
the shared ClickHouse box, `testnet.sorobanscan.rumblefish.dev`. It is the
pre-mainnet tier we have not had since staging was retired (0249, [[0390]]),
and the thing developers building on Soroban testnet keep asking for.
[ADR 0052](../../2-adrs/0052_testnet-as-second-environment-and-staging.md)
fixed the shape; this task builds it.

## Context

ADR 0052 decides three things: testnet is **configuration, not a code fork**;
its ClickHouse is a **`testnet` database on `ch-prod-01`**, isolated by RBAC
and quota exactly like the `prices` tenant ([[0314]]); **branch = environment**
(`develop → testnet → master`), with "deploy testnet straight from `develop`"
allowed as the first step.

**Already configuration** (checked 2026-09-14): the passphrase flows from
`stellarNetworkPassphrase` into `STELLAR_NETWORK_PASSPHRASE` on all three
Lambdas (`compute-stack.ts:112`) and into Galexie's `network = "testnet"`
(`ingestion-stack.ts:174-175`); indexer and API derive `network_id` from that
env (`crates/indexer/src/handler/process.rs:118`, `crates/api/src/main.rs:201`)
— every remaining `MAINNET_PASSPHRASE` use is a test. Stack names, log groups,
buckets and SSM paths all derive from `envName` (`infra/src/lib/app.ts:34`).
`init.sql` and the queries never qualify `default.` (0 hits) and
`CLICKHOUSE_DATABASE` is plumbed (`crates/db-clickhouse/src/lib.rs:51`), so a
second database needs no schema edit.

**Still says mainnet** — each one is a step below, not a surprise for later:

1. `EnvironmentConfig.envName: 'production'` is a literal type (`types.ts:13`)
   and every `infra/Makefile` target is production-only.
2. `cicd-stack.ts:65` mints one deploy role, bound to GitHub environment
   `production`. The leftover GitHub environment `staging` is not a testnet
   slot — nothing reads it (`docs/deployment.md`, "No staging").
3. The API Lambda gets a hardcoded mainnet `SOROBAN_RPC_URLS`
   (`compute-stack.ts:297-302`); the code defaults are mainnet too
   (`enrichment-shared/src/nft_token_uri/client.rs:43`,
   `api/src/runtime_enrichment/wasm_code.rs:36`). Testnet is
   `https://soroban-testnet.stellar.org`.
4. Runtime enrichment reads ledgers from `aws-public-blockchain` under
   `v1.1/stellar/ledgers/pubnet` (`stellar_archive/mod.rs:31`) — hardcoded.
5. `HetznerDnsStack` creates the `chDomainName` A record from
   `/soroban/<env>/ch-ip`; a second env naming the same host collides. Either a
   `ch-testnet.` alias (Caddy then needs a certificate for it) or reuse the
   production name and skip the stack.
6. The SPA build bakes `cloudflareApiDomainName` + `turnstileSiteKey`
   (`infra/Makefile:109-110`). The Cloudflare-fronted API hostname lives in the
   private `rf-domains` repo, and the Turnstile widget allows hostnames
   explicitly — testnet needs its own of both.
7. The SPA has no notion of network (`VITE_API_BASE_URL`,
   `VITE_TURNSTILE_SITE_KEY` are its only inputs). Without a visible TESTNET
   marker a testnet page is indistinguishable from a mainnet one.

## Implementation

Phases from the ADR, in dependency order — 1 first, 2 before anything connects.

1. **Config.** `infra/envs/testnet.json` + `infra/src/bin/testnet.ts` (mirror
   of `production.ts`); widen `envName`; parameterise the Makefile by
   environment; move the RPC URL list and the archive prefix into config.
   Galexie at 1 vCPU / 4 GB — Stellar's own testnet sizing, not the 13 GiB
   mainnet figure (captive-core memory scales with network state, not cadence).
2. **ClickHouse tenant.** `CREATE DATABASE testnet`; `apply_init_sql` with
   `CLICKHOUSE_DATABASE=testnet`; `testnet_writer` / `testnet_reader` in
   `users.d/services.xml` with grants `ON testnet.*`, profile and quota copies
   in `profiles.xml` / `quotas.xml` (the `prices_*` blocks are the template);
   CN pairs added to `CLICKHOUSE_CN_USER_MAP` (`group_vars/all.yml:104`,
   [[0240]]); client certs under `soroban/testnet/mtls/*`. A `users.d` change
   applies only on `docker compose up -d --force-recreate clickhouse` — the
   bind-mounted files are inode-pinned (0314). Quota: a bounded `read_rows`
   cap, remembering it is a hard error when tripped (0290), not a throttle.
3. **Ingestion.** `Explorer-testnet-{Network,LedgerBucket,Ingestion}`. Decide
   the start point: live-only, or from the current testnet genesis — resets are
   roughly quarterly, so the history is small either way.
4. **Compute.** `Explorer-testnet-{Compute,ApiGateway,CloudWatch}`; alarms go
   to the same Slack topic and must say which environment fired.
5. **SPA.** `Explorer-testnet-Delivery` at `testnet.sorobanscan.rumblefish.dev`,
   a TESTNET marker in the shell, the API hostname either via `rf-domains` or
   the legacy Route 53 path.
6. **CI.** `deploy-testnet.yml` from the dispatch/tag template of [[0390]];
   GitHub environment `testnet` with its own role from `cicd-stack.ts`.
   Auto-deploy from `develop` first; the `testnet` branch comes when a stable
   promotion point is wanted (ADR §3).
7. **Runbook.** Testnet reset: `DROP DATABASE testnet`, re-init, restart
   Galexie from the new genesis. Lives in `docs/runbooks/`.

Cost, estimated 2026-09-01 from the production baseline: **~$80/month net, no
new hardware** — Galexie at 1 vCPU / 4 GB is ~$51 of it. Testnet closes ledgers
on the same ~5 s cadence, so S3 request and Lambda invocation counts barely
fall; only object sizes do. Do not re-estimate it from "testnet has few
transactions".

## Acceptance Criteria

- [ ] `Explorer-testnet-*` deploys from `infra/envs/testnet.json` with no code
      branching by network, and `cdk diff` on production is empty after the
      refactor — the parameterisation must not move prod.
- [ ] Ledgers flow Galexie → S3 → indexer → `testnet` database; a testnet
      transaction resolves on the testnet SPA under the hash Stellar RPC
      reports for it (passphrase → `network_id` is right).
- [ ] `testnet_*` users cannot read `default.*` or `prices.*` (the 0314 RBAC
      check, repeated), and the quota trips a testnet query before it can
      starve production.
- [ ] No testnet Lambda consults a mainnet RPC or archive (env audit of all
      three functions), and the SPA is visibly marked TESTNET.
- [ ] The reset runbook was exercised once end to end.
- [ ] **Docs updated** —
      `docs/architecture/infrastructure/infrastructure-overview.md` §7.1
      (environment model), `docs/architecture/security/clickhouse-rbac.md`
      (tenant matrix), `docs/deployment.md` ("No staging" + a testnet recipe),
      per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
      ADR 0052 moves `proposed → accepted` when this ships.
- [ ] **API types regenerated** — run if `crates/api/**` is touched (the
      archive prefix becoming env-driven); the diff is expected to be empty.

## Notes

- Testnet is **functional** staging, not performance staging (ADR caveat): a
  handful of accounts against mainnet's 22M reproduces none of the 0357-class
  scale problems. Mainnet byte-identical verification stays.
- Testnet runs protocol upgrades weeks ahead of pubnet — [[0548]] found it on
  protocol 28 on 2026-08-27, before the pubnet vote. A testnet explorer is
  therefore also the early warning for the XDR and Galexie breakages that
  0368 and 0548 handled after the fact.
