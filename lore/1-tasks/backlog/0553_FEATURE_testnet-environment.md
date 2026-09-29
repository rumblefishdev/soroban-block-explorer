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

**Found 2026-09-28** (re-check on `develop`; the seven above still hold):

8. Galexie's live task starts at a hardcoded mainnet ledger,
   `START: '63230777'` (`ingestion-stack.ts:263`), and the backfill task
   defaults to `START: '50457424'` (`:356`). Testnet's tip is ~4.9 M, so live
   ingest would wait forever. The comment above `:263` ("TEST VALUE… REVERT
   to '63230777'") is stale — the value already is the revert target.
9. `backfill-runner` keeps its own copies, separate from item 4:
   `partition.rs:11` `ROOT_PREFIX = "v1.1/stellar/ledgers/pubnet"`,
   `snapshot/archive.rs:37` `PUBNET_ARCHIVE` (used with no override by the
   snapshot seed and reconciliation), `snapshot/report.rs:23`
   `LEDGER_FLOOR = 50_457_424` (on testnet every missing row would be filed as
   expected dormancy). Unchanged, a testnet backfill writes MAINNET ledgers into
   the `testnet` database.
10. Nothing detects a reset. Every table is keyed on the ledger sequence
    (`ledgers` `ORDER BY sequence`, entities `ReplacingMergeTree(<ledger>)`):
    after a reset ledger N silently replaces the old N, and entity rows keep
    whichever pre- or post-reset version has the higher ledger. The runbook
    (phase 7) only helps once someone notices.
11. USD prices are safe only while the testnet reader has **no** grant on
    `prices.*`. Without it the price read errors and the API degrades to NULL
    fields (`liquidity_pools/handlers.rs:341`) — correct, but one error log
    line per pool request. With it, native legs map to `("native","XLM","")`
    and testnet XLM pools show mainnet-priced volume and fees: plausible and
    wrong.
12. Degrades, not breaks: SEP-1 enrichment resolves almost nothing (issuers'
    `stellar.toml` lists mainnet assets); federation `name*domain` lands on
    mainnet accounts (`web/src/search/federation.ts:135`); the report-issue
    link hardcodes the production host (`web/src/pages/contracts/ContractCode.tsx:59`).

**Reset cadence (2026-09-29):** the Stellar networks docs say resets happen
"2-4 times per year at 17:00 UTC", announced at least two weeks ahead, and name
the next one: **2026-12-16**. Recent resets: 2025-03-19, 2025-08-14 (with
protocol 23), 2025-12-17; none so far in 2026. Testnet RPC on 2026-09-28:
ledger 4,920,864, protocol 28.

**The data lake keeps one folder per genesis**, under
`s3://aws-public-blockchain/v1.1/stellar/ledgers/testnet/`, and the naming is
undocumented. The live chain sits in `2025-12-18/`, one day after its reset;
`2025-12-17/` is an abandoned stub (ledgers 0-14,933), and `2025-06-18/` holds
only a slice. So the prefix changes on every reset, must be configuration, and
the right folder is the one with a `.config.json` that keeps growing — not the
one dated like the reset.

## Address — decided 2026-09-29

Testnet lives on subdomains of the same domain (`testnet.sorobanscan…` + an
API hostname), as its own deployment. Rejected: a path (`/testnet`) or a query
parameter (`?cluster=testnet`) on the mainnet site — both share one SPA build
across networks, so testnet could no longer run a newer frontend than mainnet
(the staging role), and both need router/cache changes in the SPA. Matches how
Stellar's own software is built: one instance and one database per network.

## PR split — decided 2026-09-29

| PR  | Scope                                                                                                                                                               | Production acts after merge                                               |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| A   | Ledger source, key prefix, bucket region and ClickHouse database from env; refuse a ledger folder of another network (API, indexer, backfill-runner)                | nothing — defaults are today's values                                     |
| B   | Infra refactor: `envName` widened, Makefile takes `ENV=`, RPC URLs and Galexie `START` into `envs/*.json`                                                           | nothing — `cdk diff` on production is empty                               |
| C   | ClickHouse tenant: `testnet_*` users, profile, quota and concurrency caps, Caddy CN map                                                                             | ClickHouse recreate (maintenance window)                                  |
| D   | Testnet environment on B: `testnet.json`, data-lake source, scheduled doorbell, stall alarm on its own channel, reset runbook, ADR 0052 accepted, architecture docs | deploy testnet, backfill from genesis — after the lake measurement passes |
| E   | TESTNET marker in the SPA shell (network switcher later)                                                                                                            | before the public start                                                   |

C is its own PR because production must act between it and D; B because a
mechanical move next to new infra would bury it, and it is proven by one
command. Stack depth two at most (B → D).

## PR A built and verified locally — 2026-09-29

Branch `feat/0553-ledger-source-config` (uncommitted at the time of writing).
Env knobs, each defaulting to today's production value: `CLICKHOUSE_DATABASE`
(all three Lambdas — they had `default` hardcoded as `PROD_DATABASE`, missed by
the list above; the env var only reached the CLIs), `LEDGER_KEY_PREFIX` and
`LEDGER_BUCKET_REGION` (indexer), `PUBLIC_ARCHIVE_PREFIX` (API, backfill-runner).
API, indexer and `backfill-runner run` refuse to start when the ledger folder's
network (`pubnet` / `testnet` segment) disagrees with
`STELLAR_NETWORK_PASSPHRASE` — ledger meta carries no network, so a mismatch
would otherwise hash silently wrong.

Local run (repo `timeouts.xml` mounted; a container without it fails with
`channel closed`, the documented 30 s `http_receive_timeout` trap):
2,000 testnet ledgers (4,862,000–4,863,999, 33,746 transactions) into a
`testnet` database. Ledger hash of 4,863,000 and 5/5 transaction hashes,
application orders and statuses equal testnet RPC. The native-XLM SAC our code
derives from the passphrase is the contract emitting 66,978 events in the
window — the mainnet derivation would name a contract that emits none there.
The three refreshable MVs bind to `testnet.*`. The guard refuses a mainnet
passphrase with the testnet folder.

## Ledger source for testnet — decided 2026-09-29

**Mainnet stays on its own Galexie. Testnet reads the public data lake**
(`aws-public-blockchain/v1.1/stellar/ledgers/testnet/<genesis>/`) instead of
running a second Galexie — but only once a week-long measurement confirms the
lake is fresh and gap-free. After testnet goes live, a standing measurement
compares the lake with our mainnet Galexie for ~2 months.

Why it is feasible with no indexer logic change: the indexer treats the S3
event as a doorbell only — it walks `max(sequence)+1` in `BUCKET_NAME` by HEAD
(`crates/indexer/src/handler/mod.rs:236-266`). It needs a key prefix and the
lake's region (`us-east-2`); the doorbell becomes a schedule (the lake publishes
no notifications).

Evidence so far (2026-09-29, read-only):

- **Freshness, one night.** Every 15 s, newest file on each side. Mainnet, 620
  samples: lake equal to our Galexie 92.9%, one ledger ahead 6.6%, one behind
  0.5%, never two behind. Testnet, 995 samples over 6 h: lake 0–1 ledgers
  behind RPC (99.8%), 2 behind twice. Zero missing files in 25 partition counts.
- **S3 timestamps cannot measure the lake.** SDF rewrites the previous week's
  objects every Sunday (73k / 96k / 155k objects on 09-13 / 09-20 / 09-27), so
  `LastModified` is the rewrite time. Only live sampling measures freshness.
- **Content.** Ledger 64,670,659 from both sources, decompressed: same size,
  same transactions, results and events. Differences: the order of ledger-entry
  changes within an operation, and `core_metrics` timings in diagnostic events.
  The parser groups changes by key, never by position, and stores no
  `core_metrics`, so neither reaches a table. One ledger — a sample, not proof.
- **Our Galexie is ~86% of the explorer's tagged AWS spend** (Cost Explorer,
  2026-08-29 → 09-27; ECS is the single `production-galexie-live` service).
  It runs at 94–97% of its 13,312 MiB memory limit (Container Insights daily
  max, 09-18 → 09-28).

## Implementation

Phases from the ADR, in dependency order — 1 first, 2 before anything connects.

1. **Config.** `infra/envs/testnet.json` + `infra/src/bin/testnet.ts` (mirror
   of `production.ts`); widen `envName`; parameterise the Makefile by
   environment; move into config the RPC URL list, the Galexie `START`
   ledgers (item 8) and the archive prefix — for BOTH readers, the API's
   `stellar_archive` and `backfill-runner` (items 4, 9), including the
   history-archive URL and the ledger floor.
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
   **No grant on `prices.*`** for the testnet users (item 11).
3. **Ingestion.** `Explorer-testnet-{Network,LedgerBucket,Ingestion}`.
   **Decided 2026-09-28: from the current testnet genesis**, backfilled from
   the public data lake (`…/ledgers/testnet/<genesis-date>/`), then live. Cheap:
   one 64k-ledger partition measured 113 MB at genesis and 1.03 GB at the tip,
   so the whole history is tens of GB (estimate from those two) against
   ~13 GB per mainnet partition. Each reset then re-runs the backfill path.
4. **Compute.** `Explorer-testnet-{Compute,ApiGateway,CloudWatch}`; alarms go
   to the same Slack topic and must say which environment fired.
   **Decided 2026-09-28: a reset alarm** (item 10) — fires when the ingested
   ledger sequence goes backwards or the genesis hash changes; ingestion stops
   until the reset runbook has run.
5. **SPA.** `Explorer-testnet-Delivery` at `testnet.sorobanscan.rumblefish.dev`,
   a TESTNET marker in the shell, the API hostname either via `rf-domains` or
   the legacy Route 53 path.
6. **CI.** `deploy-testnet.yml` from the dispatch/tag template of [[0390]];
   GitHub environment `testnet` with its own role from `cicd-stack.ts`.
   Auto-deploy from `develop` first; the `testnet` branch comes when a stable
   promotion point is wanted (ADR §3).
7. **Runbook.** Testnet reset: `DROP DATABASE testnet`, re-init, point the
   archive prefix at the new genesis folder, backfill, restart Galexie from the
   new genesis. Triggered by the reset alarm (phase 4). Lives in
   `docs/runbooks/`.

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
- [ ] `backfill-runner` reads the testnet data lake and archive when run
      for testnet (items 8–9), and the `testnet` database holds the full
      history from the current genesis.
- [ ] The reset alarm fires on a simulated reset (sequence going backwards),
      and the testnet users hold no grant on `prices.*` (items 10–11).
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
- **Protocol lead, 2026-09-29.** Testnet upgraded 15–20 days before mainnet for
  P22–P28 (P24: one day). The protocol-27 freeze of the mainnet indexer (0368,
  2026-07-09) would have shown on testnet on 2026-06-18. Consequences: the
  stellar-xdr bump ships to testnet first and must still decode the older
  mainnet protocol; the testnet indexer stalls on each testnet upgrade until it
  does, so the stall alarm must say "testnet" and not page as a production
  incident. **Gap:** reading the data lake, testnet never exercises OUR Galexie,
  so a stale-core stall like 0367 is not caught. Cover it in the upgrade
  runbook: a one-off run of the new Galexie image against testnet before the
  mainnet vote.
