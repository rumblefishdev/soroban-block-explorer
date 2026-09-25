# Deployment Guide

Single entry point for shipping changes to production. **"I changed X →
run Y → verify Z."**

Deep-dives live in the per-layer READMEs (linked below). This file does
**not** duplicate them — it ties them together and is the source of truth
for _which_ command ships _what_.

> **Important — there is no staging environment.** Production is the only one.
> A release is a `production-*` tag, which runs the CI deploy
> (`.github/workflows/deploy-production.yml`); the same ships can also be run
> **manually from an operator laptop**, which is the path for surgical,
> single-stack work. See
> [§ Releases and the CI deploy path](#releases-and-the-ci-deploy-path), and
> [§ No staging](#no-staging) before trusting any `staging` command you find
> elsewhere in the repo.

---

## TL;DR — what do I run?

| I changed…                                                                  | Run                                                                    | Plane      |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ---------- |
| API handler (`crates/api`)                                                  | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Indexer / Ledger Processor (`crates/indexer`, `xdr-parser`)                 | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Enrichment worker (`crates/…enrichment…`)                                   | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Frontend SPA (`web/`) — **content**                                         | `make -C infra deploy-production-web`                                  | AWS CDK    |
| Galexie version bump                                                        | edit `galexieImageTag` + mirror to ECR + `deploy-production-ingestion` | AWS CDK    |
| API Gateway (cache / throttle / mTLS lock)                                  | `make -C infra deploy-production-apigateway`                           | AWS CDK    |
| CloudWatch alarms / dashboards                                              | `make -C infra deploy-production-cloudwatch`                           | AWS CDK    |
| CloudFront / SPA delivery **infra** (not content)                           | `make -C infra deploy-production-delivery`                             | AWS CDK    |
| Route 53 record for the CH box (IP change)                                  | `make -C infra deploy-production-hetzner-dns`                          | AWS CDK    |
| Anything else in `infra/src` (multiple stacks)                              | `make -C infra deploy-production` (all) — but see gotchas              | AWS CDK    |
| ClickHouse box config / `docker-compose.prod.yml` / Caddy / users / backups | Ansible — see [§ The machine](#the-machine-hetzner-clickhouse)         | Hetzner    |
| Cloudflare edge (DNS / WAF / mTLS)                                          | Terraform — see [§ Cloudflare edge](#cloudflare-edge)                  | Cloudflare |

Always preview first: `make -C infra diff-production`.

Shipping a **release** — Compute + SPA together, as one act — is a different
thing from the per-change table above: see
[§ Releases and the CI deploy path](#releases-and-the-ci-deploy-path). Every
row above also has a tag form (`production-<date>-<N>-<StackName>`), which
runs the same deploy from CI instead of a laptop and leaves a full `cdk diff`
in the job log; the `make` targets remain the right tool when you want to
watch the diff before committing to it.

---

## The three deploy planes

Production is deployed across **three independent planes**. Planes 2 and 3 are
laptop-only; plane 1 additionally has a tag-driven CI path (§ Releases).

1. **AWS (CDK)** — the bulk: Lambdas (API / indexer / enrichment), Galexie
   (ECS), CloudFront + SPA, API Gateway, alarms, DNS. Driven by
   `make -C infra deploy-production-*`. This document + [`infra/README.md`](../infra/README.md).
2. **Hetzner ClickHouse box (Ansible)** — the production data plane (CH +
   Caddy mTLS + backups). Driven by `ansible-playbook`. Deep-dive:
   [`infra-hetzner/README.md`](../infra-hetzner/README.md).
3. **Cloudflare edge (Terraform)** — DNS record + WAF + API origin mTLS
   lock for the API host. Deep-dive: [`infra/cloudflare/README.md`](../infra/cloudflare/README.md).

They are decoupled: a backend fix touches only plane 1; a CH schema/config
change touches only plane 2; they are shipped separately.

---

## No staging

AWS-side staging was **retired by task 0249** — production is the only AWS
environment (`eu-central-1`). Its leftovers were removed by task 0390:
`.github/workflows/deploy-staging.yml` (it pointed at `bin/staging.js` and
`envs/staging.json`, both long gone) and `scripts/staging-deploy.sh` with the
`staging-*` git-tag trigger behind it (ADR 0009, now superseded).
`pnpm run infra:{deploy,diff,synth}:staging` and any `make deploy-staging*`
target **do not exist** and error immediately.

If you find a `staging` command in an old README or your shell history, it is
stale. A real pre-mainnet tier is proposed as the **testnet** environment
(ADR 0052) — not as a revived `staging`.

> The GitHub **environment** named `staging` is a different thing. It is a
> leftover from April 2026, superseded by `production` (below), and nothing
> reads it.

---

## Releases and the CI deploy path

**A release is a git tag.** Pushing `production-YYYY.MM.DD-N[-SELECTOR]` to
`master` runs
[`.github/workflows/deploy-production.yml`](../.github/workflows/deploy-production.yml):

```
build → cdk diff (ALL stacks) → cdk deploy <selected> → make -C infra deploy-production-web
      → smoke (API /health + frontend 200)
```

```bash
git fetch origin
git tag production-$(date -u +%Y.%m.%d)-1 origin/master
git push origin production-$(date -u +%Y.%m.%d)-1
```

Tag `origin/master`, not a local `master` (routinely stale), and push the tag
**by name** — `--tags` also pushes every stale local tag.

The tag **is** the release decision — there is no separate approval gate,
because a tag is deliberate in a way a merge is not. `-N` increments for a
second release on the same day.

**The optional selector puts the deploy's scope inside that same decision:**

| Tag                                  | CDK deploy                                  | SPA content |
| ------------------------------------ | ------------------------------------------- | ----------- |
| `production-2026.08.18-1`            | `Explorer-production-Compute --exclusively` | yes         |
| `production-2026.08.18-1-all`        | `--all` — every stack that differs          | yes         |
| `production-2026.08.18-1-CloudWatch` | that stack, `--exclusively`                 | no          |
| `production-2026.08.18-1-web`        | none                                        | yes         |

A selector other than `all` / `web` is appended to `Explorer-production-`
**verbatim**, so it carries the stack's own case (`CloudWatch`, `ApiGateway`,
`LedgerBucket`, `CloudflareBootstrap`, `HetznerDns`, …). The mapping lives in
[`infra/scripts/deploy-scope.sh`](../infra/scripts/deploy-scope.sh) — run it to
see what a tag will do, and note that it now **rejects** a malformed tag
outright, where the old `production-*` trigger would deploy the release set for
any name at all.

- **The standard release set is Compute + SPA content.** A release users cannot
  see is not shipped, so the SPA sync runs on every unselected tag.
- **`-all` means "everything that differs", not "everything"** — CDK skips
  unchanged stacks. It is also the only path that ships parked, unreviewed
  drift ([0312's CloudflareBootstrap delta](#gotchas--read-before-you-deploy)
  sat undeployed for weeks). Prefer naming the one stack you mean; typing
  `-all` is what keeps a full deploy a choice rather than a side effect, the
  same job the typed `yes` does in `infra/Makefile`.
- **The SPA step is not a Delivery deploy.** It reads that stack's
  `SpaBucketName` / `DistributionId` outputs and syncs S3. Changes to
  `delivery-stack.ts` itself need `-Delivery` or `-all`.
- **Surgical deploys can also stay on `workflow_dispatch`** — name the stack(s)
  in the `stacks` input (`--exclusively` on by default, `deploy_web` opt-in).
  Use it when the deploy should not mint a tag.
- **`cdk diff` covers ALL stacks, deliberately wider than the deploy** — it is
  printed into the job log, and it is how a delta parked in a stack this tag
  does not ship becomes visible at release time instead of being forgotten on
  someone's laptop. It runs with `--strict`, without which `cdk diff` silently
  hides entries containing non-ASCII characters.
- **Tag runs execute the workflow file _at the tagged commit_.** The workflow
  has to be on `master` before the first tag, and fixing the workflow means
  cutting a new tag, not re-running the old one.
- **Issues close at deploy, not at merge** — run `/issues` after a release.
- **Budget ~15 min per tag run; expect much less when the cache holds.**
  `cdk diff` builds all three Lambdas during synth (`cargo-lambda-cdk`'s
  `RustFunction` builds at synth time, not as a separate step). Until
  2026-08-18 it did so from a **cold Rust cache on every release** — measured
  2026-08-17: 10m35s / 10m58s of diff in 13-14-minute runs. The deploy job now
  runs on `ubuntu-24.04-arm` with rust-cache key `ci-rust-lambda` —
  deliberately the same runner arch and key as CI's `rust-lambda` job, which
  runs the identical `cargo lambda build --release --arm64` **on every push to
  `master`** and saves the cache there. Default-branch caches are readable
  from any ref, tags included, and the tag points at `master` HEAD, so the
  `Cargo.lock` hash matches exactly: dependencies restore, only the workspace
  crates compile. On x86 that cache was unreachable by construction (the rustc
  host triple is part of the key), which is why the old note said "expect no
  speed-up" — the premise that seeding would cost an extra build per merge
  turned out to be false, because CI was already paying for that exact build.

  Still budget the full ~15 min: the cache can be evicted (10 GB repo quota —
  the deploy job sets `save-if: false` so tag runs don't contribute to that
  pressure), and a cold run costs what it always did. The deploy step reuses
  the diff's synth output (`--app cdk.out`), so what deploys is byte-for-byte
  what was diffed and nothing is built twice. A `-web` tag skips the Rust/CDK
  path entirely (no drift record on those runs — every other tag shape prints
  one).

The job binds `environment: production`, so its OIDC subject is
`repo:<org>/<repo>:environment:production` — that string is what the deploy
role's trust policy matches, and it is why the environment binding is not
optional. The role itself is defined in
`infra/src/lib/stacks/cicd-stack.ts` and created by `make -C infra deploy-cicd`.

---

## Operator prerequisites (one-time laptop setup)

**AWS plane:**

- AWS CLI with a named profile that has deploy rights:
  ```bash
  aws configure --profile soroban-explorer
  export AWS_PROFILE=soroban-explorer
  ```
  Region is read from `infra/envs/production.json → awsRegion`
  (`eu-central-1`); you do not pass `--region`.
- Node `22.22.0` (`.nvmrc`) + `pnpm install --frozen-lockfile` at the repo root (pnpm version pinned by `packageManager` in `package.json`).
- **Rust toolchain + `cargo-lambda` + `zig`** — the API / indexer /
  enrichment Lambdas are Rust, cross-compiled at CDK synth time. Without
  these, `deploy-production-compute` fails at build:
  ```bash
  rustup toolchain install stable
  pip install cargo-lambda        # or: brew install cargo-lambda
  ```
- **First time per AWS account** (not per deploy):
  ```bash
  make -C infra bootstrap          # CDK bootstrap (account + region)
  make -C infra deploy-cicd        # shared OIDC / CICD roles (envs/cicd.json)
  ```

**Hetzner plane:** `ansible-core`, `openssl`, `hcloud` (pip), plus the
password-manager entries and your mTLS cert — full list in
[`infra-hetzner/README.md` § Prerequisites](../infra-hetzner/README.md#prerequisites).

**Cloudflare plane:** Terraform + a zone-scoped `CLOUDFLARE_API_TOKEN` —
see [`infra/cloudflare/README.md`](../infra/cloudflare/README.md).

---

## AWS CDK — stacks and commands

The production app (`node dist/bin/production.js`, config
`infra/envs/production.json`) deploys these stacks. All commands run from
the repo root as `make -C infra <target>` (or `cd infra && make <target>`).

| Stack (`Explorer-production-…`) | Contains                                                             | Make target                              |
| ------------------------------- | -------------------------------------------------------------------- | ---------------------------------------- |
| `Network`                       | Minimal VPC, public subnet (post-0239, no NAT/RDS)                   | `deploy-production-network`              |
| `LedgerBucket`                  | S3 `stellar-ledger-data` (Galexie → indexer)                         | `deploy-production-ledger-bucket`        |
| `Compute`                       | **API + Indexer + Enrichment Lambdas** + SQS queues/DLQ              | `deploy-production-compute`              |
| `Ingestion`                     | Galexie ECS Fargate task + ECR repo                                  | `deploy-production-ingestion`            |
| `Delivery`                      | CloudFront distribution + SPA S3 bucket                              | `deploy-production-delivery`             |
| `ApiGateway`                    | REST API Gateway, caching, throttle, mTLS/edge lock                  | `deploy-production-apigateway`           |
| `Observability`                 | X-Ray / logging config                                               | `deploy-production-observability`        |
| `CloudWatch`                    | Alarms + dashboards                                                  | `deploy-production-cloudwatch`           |
| `HetznerDns`                    | Route 53 A record → CH box IP (from SSM `/soroban/production/ch-ip`) | `deploy-production-hetzner-dns`          |
| `CloudflareBootstrap`           | TF-state bucket for `infra/cloudflare/`                              | `deploy-production-cloudflare-bootstrap` |

Frontend **content** is separate: `deploy-production-web`
(build → S3 sync → CloudFront invalidation).

### Gotchas — read before you deploy

- **Any `ALTER` on a table the indexer writes can stop ingestion — even an
  ADD.** clickhouse-rs 0.15 checks the row struct against `DESCRIBE TABLE`
  before every insert: a table column the struct does not name must have a
  `DEFAULT` (`Nullable` alone does not count), and a struct field must exist in
  the table. The indexer running in production is the OLD build, so:

  - **`ADD COLUMN` always with an explicit `DEFAULT`**, for a nullable column
    `DEFAULT NULL`. Then "column first, code later" is safe. Without it, every insert
    from the running indexer fails until the new build ships (task 0548: 31
    minutes of frozen ingestion).
  - **`DROP COLUMN` of a no-DEFAULT column** only together with the build that
    stopped writing it (task 0310, and the Soroban-AMM gotcha below).
  - **Before handing over the DDL**, run it on a local ClickHouse and insert
    with the struct from the commit that is deployed, not from the branch.
  - **After the `ALTER`, recycle the indexer's execution environment.** The
    driver caches `DESCRIBE` for the life of the environment, and a failed
    insert does not reset it, so even a corrected schema keeps failing until
    Lambda replaces it. A no-op configuration change does that:

    ```bash
    aws lambda update-function-configuration --region eu-central-1 \
      --function-name production-soroban-explorer-indexer \
      --description "recycle: refresh cached ClickHouse schema $(date -u +%FT%TZ)"
    ```

    Pass `--region` — the function lives in `eu-central-1`, and a shell
    defaulting elsewhere answers `ResourceNotFoundException`. Confirm with a
    fresh `DESCRIBE` in `system.query_log` and `max(sequence)` on `ledgers`
    advancing. A `make deploy-production-compute` recycles too.

- **Soroban-AMM (task 0374): DDL BEFORE the indexer, or ingest stops.** The
  clickhouse-rs 0.15 client refuses an insert when the target table still has
  a no-DEFAULT column the row struct dropped, or is missing entirely — a
  `SchemaError` aborts the whole partition (the 0310 outage class). The 0374
  indexer's soroban arm runs unconditionally, so deploying it against an
  un-migrated schema is an outage, not a degradation. Order:

  0. **Pause the indexer first** (`indexerLambdaConcurrency = 0`): the
     RUNNING (pre-0374) writer still names `tvl`/`volume`/`fee_revenue` in
     its snapshot inserts (verified on `origin/master`
     `LiquidityPoolSnapshotRow`), so dropping them under a live writer
     fails every snapshot insert — doorbell retries drain toward the DLQ
     until the new code deploys. Doorbells accumulate durably while paused
     (see §3.1); the reconcile catches up after unpause. `CREATE TABLE` x2
     and the `share_token_id` drop need no pause (nothing running writes or
     reads them — verified against master's structs and SQL), but batch
     everything inside the one pause window anyway.
  1. **DDL next** (prod is mid-migration: the `liquidity_pools` soroban
     columns are already ALTERed in — verified live 2026-09-02, five
     columns present with correct defaults; the rest is not):
     ```sql
     -- new tables: run the definitions VERBATIM from
     -- crates/db-clickhouse/schema/init.sql (the source of truth — do not
     -- retype them). Extract with:
     --   awk '/CREATE TABLE IF NOT EXISTS pool_state_changes/,/^ORDER BY/; /CREATE TABLE IF NOT EXISTS pool_instance_state/,/^ORDER BY/' \
     --     crates/db-clickhouse/schema/init.sql
     CREATE TABLE IF NOT EXISTS pool_state_changes (...);   -- from init.sql
     CREATE TABLE IF NOT EXISTS pool_instance_state (...);  -- from init.sql
     -- load-bearing drops (no-DEFAULT columns the new structs dropped)
     ALTER TABLE liquidity_pool_snapshots
       DROP COLUMN tvl, DROP COLUMN volume, DROP COLUMN fee_revenue;
     -- DEFAULT 0, no hard ordering constraint — batch it here anyway
     ALTER TABLE liquidity_pools DROP COLUMN share_token_id;
     ```
  2. **Then the indexer** (Galexie recipe below).
  3. **Then the catch-up backfills and the window-closure check** — see
     "Soroban-AMM pool passes" in [backfills.md](./backfills.md).

- **Canonical event location (task 0541): no `production-*` tag between the
  merge and the window.** The task-0541 writer keys `soroban_events` by the
  stellar-rpc event id (`transaction_index`, `operation_index`, `event_index`,
  `application_order`) and no longer names `transaction_id`; it stops writing
  `asset_transfers.event_index`; and it writes a new table,
  `contract_transactions`. Against the tables as they stand before the window,
  the clickhouse-rs 0.15 client refuses all three inserts — the 0310 outage
  class, on every ledger. `EXCHANGE TABLES` renames `soroban_events` only; it
  does nothing for the other two. So once the task-0541 code is on `master`,
  nothing ships until its window runs, in this order (the task's rollout plan,
  phase 4, carries the gates and the rollback):

  0. **`contract_transactions` exists and its history is filled** through the
     partitions filled before the window. Created verbatim from `init.sql`;
     filled in-DB, slice by slice after the `soroban_events` rekey of the same
     slice ([backfills.md](./backfills.md), "Canonical event location fill").
     An empty table is a working indexer and a contract transaction list with
     no past — fill it, do not just create it. This must return `1`:
     ```bash
     chq "EXISTS TABLE default.contract_transactions"
     ```
  1. **`asset_transfers.event_index` has a DEFAULT — read it, do not assume
     it.** This must return `DEFAULT`:
     ```bash
     chq "SELECT default_kind FROM system.columns WHERE database = 'default' AND table = 'asset_transfers' AND name = 'event_index'"
     ```
     If it does not, the operator runs
     `ALTER TABLE asset_transfers MODIFY COLUMN event_index DEFAULT 0` — a
     metadata change, safe under the running writer, which still names the
     column.
  2. **Pause durably:** `indexerLambdaConcurrency = 0`, deployed from a
     checkout at the last `production-*` tag (a disabled trigger alone is
     re-enabled by the next Compute deploy).
  3. **Fill the tail** up to the paused head — `soroban_events_staging_canonical`
     first, then `contract_transactions` for the same slices — and gate both.
  4. **Deploy the new code, still at concurrency 0.**
  5. **Swap:** `EXCHANGE TABLES soroban_events AND soroban_events_staging_canonical`.
  6. **Resume:** concurrency back to `1` — the value production runs, not the
     unset default — then deploy Compute and Web.

  **After the swap the hold turns around:** only code that carries task 0541
  may deploy Compute. The earlier writer still names `transaction_id`, which the
  new `soroban_events` does not have, so a Compute deploy from `master` or a
  `production-*` tag that predates the change stops ingest on its first ledger —
  a hotfix included. The window deploys from `develop`; the hold lasts until
  `master` carries the change and a `production-*` tag is cut from it.

  Remove this item once that tag exists.

- **Change a table's shape as a parallel change, not a swap window**
  (decided in task 0580). The clickhouse-rs 0.15 client checks every insert
  against `DESCRIBE`, so a writer and the table it names must change together;
  swapping a table under the same name forces an indexer pause and a gap in
  which readers fail (task 0575: ingest stood 51 minutes). Instead: create the
  new table under a **new name**, deploy a writer that writes both, fill the
  history in ClickHouse, switch the readers in a later deploy, then stop
  writing the old table and drop it. Every step is an ordinary deploy, and
  until the drop the rollback is the previous deploy. Drop the old table
  only after the deploy that stops writing it — the earlier writer still
  inserts, and a drop before that deploy stops ingest on the next ledger.
  The server refuses to drop a table over 50 GB unless told:
  `DROP TABLE <old> SETTINGS max_table_size_to_drop = 0`.

- **Operations by transaction position (task 0372), step 1 of that pattern.**
  The indexer writes `transaction_operations` and `pool_operation_amounts`
  beside `operations_appearances` and `lp_operation_amounts`, and stops
  writing `operation_pools`. Create both tables on production **before** the
  Compute deploy — without them the client refuses the insert on every
  ledger. The command prints the two statements; run each through `chw`:

  ```bash
  for t in transaction_operations pool_operation_amounts; do awk "/CREATE TABLE IF NOT EXISTS $t \\(/,/^ORDER BY/" crates/db-clickhouse/schema/init.sql; done
  ```

  Then deploy Compute; then `DROP TABLE operation_pools` (no reader since task
  0491, no writer after this deploy; prices-api check recorded in task 0372);
  then fill the history
  ([backfills.md](./backfills.md), "Operations by transaction position"). No
  pause; the readers still use the old tables.

- **Operations by transaction position (task 0372), step 2: the readers.**
  The API reads `transaction_operations` and `pool_operation_amounts` only.
  Deploy Compute **after the history fill has passed its gates in every
  partition**, head included: a range the fill has not reached lists
  transactions with empty `operation_types`, drops them from the
  operation-type filter and hides their pool activity. No operator step. The
  operation-type filter of `/transactions` and pool activity now page on the
  transaction's position, so a cursor minted before the deploy answers 400
  `invalid_cursor` once.

- **Operations by transaction position (task 0372), step 3: stop the old
  writes.** The indexer writes `transaction_operations` and
  `pool_operation_amounts` only; `operations_appearances` and
  `lp_operation_amounts` leave `init.sql`. Deploy Compute after step 2's
  deploy (the running API must no longer read the old tables — check
  `system.query_log`), then drop both. Before each drop, record the prices-api
  check in task 0372. `operations_appearances` is over the 50 GB drop guard:

  ```sql
  DROP TABLE operations_appearances SETTINGS max_table_size_to_drop = 0;
  DROP TABLE lp_operation_amounts;
  ```

  A drop before this deploy stops ingest on the next ledger: the earlier writer
  still inserts into both.

- **Contract activity (task 0586), step 1 of that pattern.** The indexer
  writes `contract_activity` beside `contract_transactions` and
  `soroban_invocations_appearances`. Create the table on production **before**
  the Compute deploy — without it the client refuses the insert on every
  ledger. The command prints the statement; run it through `chw`:

  ```bash
  awk '/CREATE TABLE IF NOT EXISTS contract_activity \(/,/^ORDER BY/' crates/db-clickhouse/schema/init.sql
  ```

  Then deploy Compute; then fill the history ([backfills.md](./backfills.md),
  "Contract activity"). No pause; the readers still use the old tables.

- **Contract activity (task 0586), step 2: the readers.** The API reads
  `contract_activity` only: the contract invocation stats, the Invocations
  tab, the transaction page's invocations and the `/transactions` contract
  filter. Deploy Compute **after the history fill has passed its gates in
  every partition**, head included: a range the fill has not reached would
  show no invocations and drop out of the contract filter. No operator step.
  The Invocations tab now pages on the transaction's position, so a cursor
  minted before the deploy answers 400 `invalid_cursor` once.

- **Presence tables by position (task 0575): no `production-*` tag between
  the merge and the window.** The task-0575 writer names `application_order`
  instead of `transaction_id` in `transaction_participants` and
  `operation_asset_appearances`, and its API reads the same. Against the tables
  as they stand before the window, the clickhouse-rs 0.15 client refuses both
  inserts on every ledger (the 0310 outage class). A `DEFAULT` cannot bridge
  it: `transaction_id` is in the sort key, so only a swap removes it. Order:

  0. **Both staging tables exist and every whole partition below the head is
     filled and gated** ([backfills.md](./backfills.md), "Presence tables by
     transaction position"). Created from `init.sql` under the staging name —
     extract, do not retype:
     ```bash
     for t in transaction_participants operation_asset_appearances; do
       awk "/CREATE TABLE IF NOT EXISTS $t \\(/,/^ORDER BY/" crates/db-clickhouse/schema/init.sql \
         | sed "s/EXISTS $t (/EXISTS ${t}_staging_position (/"
     done
     ```
  1. **Read benchmark on the staging tables** (read-only): the account and
     asset transaction lists pointed at `*_staging_position`, on the hottest
     account, native XLM and one mid asset — each statement under 1 s and
     1 GiB. A failure stops here; the staging tables are dropped.
  2. **Pause durably:** `indexerLambdaConcurrency = 0`, deployed from a
     checkout at the last `production-*` tag.
  3. **Fill the tail** from the first unfilled ledger up to the paused head
     (`A-B` with `B = max(sequence) + 1` from `ledgers`) for both tables, gated.
  4. **Deploy the new code, still at concurrency 0.**
  5. **Swap both:**
     `EXCHANGE TABLES transaction_participants AND transaction_participants_staging_position`,
     then the same for `operation_asset_appearances`. Between step 4 and this
     step the account and asset transaction lists answer errors; keep it short.
  6. **Resume:** concurrency back to `1`, then deploy Compute. Check that
     `ledgers` advances, the DLQ stays empty, and every row the new writer adds
     names a transaction that exists (a `NOT IN` against `transactions` over
     the new ledgers returns 0).
  7. **Drop the old tables** — now under the staging names — only after the
     checks and the production pages look right:
     `DROP TABLE <table>_staging_position SETTINGS max_table_size_to_drop = 0`
     (the server keeps the default 50 GB limit). Until then, rollback is the
     reverse `EXCHANGE` plus a Compute deploy of the previous code.

  **After the swap the hold turns around:** only code that carries task 0575
  may deploy Compute — an earlier writer still names `transaction_id`. The hold
  lasts until a `production-*` tag carries the change. Remove this item then.

- **A SPA build without the Turnstile site key takes production down for
  users.** With `enableAuthLayer: true` the API rejects unauthenticated
  requests; a bundle built without `VITE_TURNSTILE_SITE_KEY` ships an
  un-armed SPA whose every API call answers 401 — the site renders and
  shows no data. `build-production-web` bakes the key from
  `envs/production.json`, but the **Nx build cache can serve a stale
  no-key bundle** even when the env is correct. For any isolated/first
  production web build, add `--skip-nx-cache`, and after `deploy-production-web`
  verify from a clean browser that `/auth/session` answers 200 and data
  renders. (This happened live; the outage looked like an API failure
  while the defect was in the shipped bundle.)

- **Every new stack MUST tag its resources** with
  `Project=soroban-block-explorer` (+ `Environment`, `ManagedBy`) via
  `cdk.Tags.of(this).add(...)` — the account is shared with
  `stellar-prices-api` and the `Project` tag is the only cost-attribution
  dimension (task 0449; the July 2026 cost investigation took a day
  because untagged spend cannot be attributed after the fact). Cost
  allocation tag activation lives in the **organization management
  account**, not here.
- **`make deploy-production` deploys `--all`.** It will push every pending
  change across _every_ stack. For a single change, prefer the per-stack
  target. Since task 0455 the target first prints the full `cdk diff
--strict` and requires a literal `yes` before deploying, so parked deltas
  (the 0312 stowaway class) are seen, not shipped blind; `FORCE=1` skips the
  prompt for non-interactive use. `--strict` matters on its own: without it
  `cdk diff` silently hides entries containing non-ASCII characters.
- **Per-stack `make` targets also deploy that stack's dependencies**
  (CDK default). If a dependency stack has an unrelated pending change, it
  ships too. To deploy **exactly one stack** and nothing else, run raw with
  `--exclusively`:
  ```bash
  make -C infra build     # compile CDK first
  cd infra && pnpm exec cdk --app "node dist/bin/production.js" \
      deploy Explorer-production-CloudWatch --exclusively --require-approval broadening
  ```
  (Real lesson: shipping a CloudWatch-only alarm change without
  `--exclusively` dragged in a half-finished Compute lambda change.)
- **CDK does not delete a stack you removed from the app.** Deleting a stack's
  code makes it vanish from `cdk ls` and from `deploy --all`, while the real stack
  keeps existing in CloudFormation and keeps billing. Worse, `cdk destroy <name>`
  resolves names from the **synthesized app**, so once the code is gone CDK reports
  no matching stack and cannot delete it either. The only route is raw
  CloudFormation, and if the stack exported anything, in this order:

  1. deploy the consumer stack(s) first, so nothing references the export any more;
  2. confirm the export parameter is released — a cross-region export writer
     refuses to remove a parameter a consumer still claims, and the whole stack
     delete fails with it. The parameter lives in the **consuming** region, not the
     producing one:
     ```bash
     aws ssm get-parameters-by-path --region eu-central-1 --path /cdk/exports/ --recursive
     ```
  3. then delete:
     ```bash
     aws cloudformation delete-stack --region us-east-1 --stack-name <StackName>
     ```

  (Real case: `Explorer-production-CloudFrontWaf` in `us-east-1`, task 0302. Its
  WebACL ARN was consumed by `Delivery` in `eu-central-1`.)

- **`--require-approval broadening`** — the deploy pauses for confirmation
  if the change broadens IAM or security-group rules. Review, then approve.
- **Preview:** `make -C infra diff-production`, or raw `cdk diff <stack>`.

---

## Component recipes (the ones with nuance)

### Backend Lambdas — Compute stack

All three Rust Lambdas (API, Ledger Processor/indexer, type-1 enrichment
worker) live in `Explorer-production-Compute`:

```bash
make -C infra deploy-production-compute
```

**Pausing the indexer or the enrichment worker.** Both consume an SQS
queue via an event-source-mapping (ESM); pausing = stopping that
consumption. Messages keep landing in the queue and are **not dropped**
(the S3→SQS notification is not gated on the pause), so either lever below
is safe and fully reversible. Pick by how long the pause needs to last:

- **Quick / temporary — disable the ESM (no deploy).** The fastest lever:
  flip the SQS trigger off and the Lambda stops parsing (takes effect in
  under a minute). Console: Lambda → the function → **Configuration →
  Triggers** → the SQS trigger → **Disable**. CLI:

  ```bash
  aws lambda list-event-source-mappings \
    --function-name production-soroban-explorer-indexer \
    --query 'EventSourceMappings[].UUID' --output text
  aws lambda update-event-source-mapping --uuid <uuid> --no-enabled   # resume: --enabled
  ```

  (enrichment worker: `production-soroban-explorer-enrichment-worker`.)

  > ⚠️ **Not durable.** `production.json` still says `concurrency: 1`, so
  > the **next `make deploy-production-compute` re-enables the ESM** (CDK
  > reconciles it back to `enabled`). Use this for a short, watched pause —
  > not to park a worker across deploys.

- **Durable — set concurrency to `0` + redeploy Compute.** Puts the paused
  state in code so it survives future deploys. The ESM is created **only
  when concurrency > 0** (`compute-stack.ts`), so:

  - `indexerLambdaConcurrency: 0` → indexer ESM not created at all.
  - `enrichmentWorkerLambdaConcurrency: 0` → same for the enrichment worker.

  Set back to `1` + redeploy to resume. **A redeploy alone does not toggle
  this** — the `production.json` value is the switch.

### Galexie (live ingestion) — Ingestion stack

The Galexie version is **pinned by ECR image digest** in
`infra/envs/production.json → galexieImageTag`. CDK resolves it through
`ContainerImage.fromEcrRepository(repo, galexieImageTag)`, which calls
`repositoryUriForTagOrDigest` — a `sha256:…` value is therefore treated as a
**digest** (`repoUri@sha256:…`), not a tag. The image must already be in the
`production-galexie` ECR repo.

Bump procedure — **pull → tag → push → sha**:

1. **Mirror the image Docker Hub → ECR:**

   ```bash
   REPO=$(aws ssm get-parameter --region eu-central-1 \
     --name /soroban-explorer/production/ecr-galexie-repo-uri \
     --query Parameter.Value --output text)
   aws ecr get-login-password --region eu-central-1 | \
     docker login --username AWS --password-stdin "${REPO%%/*}"

   docker pull stellar/stellar-galexie:<version>          # or @sha256:<hub-digest>
   docker tag  stellar/stellar-galexie:<version> "$REPO:<version>"
   docker push "$REPO:<version>"        # ← note the digest ECR prints back
   ```

2. **Put the ECR digest in `production.json → galexieImageTag`.** If you missed
   what `docker push` printed:

   ```bash
   aws ecr describe-images --region eu-central-1 \
     --repository-name production-galexie --image-ids imageTag=<version> \
     --query 'imageDetails[0].imageDigest' --output text
   ```

   > ⚠️ **Pin the digest ECR reports, never the Docker Hub one.** When Hub
   > serves a multi-arch manifest list, pushing to ECR rewrites the manifest
   > and the two digests differ: the 27.0.0 pin is Hub `sha256:81a9e829…` but
   > ECR `sha256:91eae7af…`, and copying the Hub digest across yields an image
   > ECS cannot pull. A single-architecture image pulled by digest keeps its
   > digest (28.0.1: `sha256:1d511631…` on both), so the two may also match —
   > which is why the rule is "read it back", not "expect a difference".

3. **Roll the ECS task:**

   ```bash
   make -C infra deploy-production-ingestion
   ```

4. **Verify:** ECS service healthy + the Galexie ingestion-lag alarm quiet
   (a stalled Galexie = an ingestion outage; the lag alarm is your signal).

> **`GALEXIE_IMAGE_DIGEST` (GitHub `staging` Environment) is not part of this.** > **Nothing reads it any more** — its only consumer, `deploy-staging.yml`, was
> removed by task 0390 (see [§ No staging](#no-staging)), and the release
> workflow does not look at it. `production.json` is the pin. Task 0367 updated
> the variable as bookkeeping; treat it as a record, not a lever.

> Do **not** flip `assignPublicIp` on the Galexie task — it is the only
> egress path (no NAT GW post-0239). There is a CODEOWNERS-flagged inline
> comment in `ingestion-stack.ts` guarding this.

### Frontend SPA — Delivery stack + `deploy-production-web`

`Delivery` provisions the CloudFront distribution and the SPA bucket. The
**content** is a separate step that builds the SPA (baking `VITE_API_BASE_URL`
from `cloudflareApiDomainName` and `VITE_TURNSTILE_SITE_KEY` from
`turnstileSiteKey`, both read out of `production.json` — no shell env needed),
syncs to S3, and invalidates CloudFront:

```bash
make -C infra deploy-production-web
```

> **Arming guard.** With `enableAuthLayer: true`, the build step greps the
> emitted bundle for the Turnstile site key and **aborts before any S3 sync** if
> it is missing. This is the fix for the 0437 incident: an SPA built without the
> key attaches no session token, so every API call 401s. The Nx build cache does
> not hash env vars, which is why a build "with" the key could silently reuse a
> cached bundle built without it — `web/package.json` now declares both `VITE_*`
> vars as build inputs so a key change busts the cache.

### CloudWatch alarms / API Gateway / DNS

- Alarms & dashboards: `make -C infra deploy-production-cloudwatch`
  (use the `--exclusively` raw form if other stacks have pending changes), or
  from CI with a `production-<date>-<N>-CloudWatch` tag, which is
  `--exclusively` already and skips the SPA sync.
- API Gateway cache / throttle / edge-lock toggles live in
  `production.json` (`apiGatewayCache*`, `apiGatewayThrottle*`,
  `enableApiMtls`, `enable*Lock`): `make -C infra deploy-production-apigateway`.
- CH box IP changed (box replacement): update the SSM parameter, then
  `make -C infra deploy-production-hetzner-dns` (see
  [`infra/README.md` § HetznerDnsStack](../infra/README.md#hetznerdnsstack--route-53-record-for-the-production-clickhouse-box)).

---

## The machine (Hetzner ClickHouse)

The production data plane runs on the Hetzner dedicated server
`ch-prod-01`: ClickHouse + Caddy (mTLS) + Borg backups, all via
`docker-compose.prod.yml`. **Deploy is Ansible from a laptop — not CI.**

```bash
source ~/.config/soroban-prod.env                   # secrets from password manager
cd infra-hetzner/ansible
ansible-playbook -i inventory.ini site.yml          # full run (idempotent)
ansible-playbook -i inventory.ini site.yml --tags app   # after a docker-compose.prod.yml / .env change
```

`--check --diff` for a dry run; `--tags {app,security,hetzner,storagebox,users}`
to scope a re-run.

Everything else about the box — **first-time setup, disaster recovery
(box loss / restore from Borg), CH & Borg password rotation, adding/removing
a developer, the `--force-recreate clickhouse` log-rotation gotcha, and
post-deploy verification** — is fully documented and **not duplicated
here**:

➡️ **[`infra-hetzner/README.md`](../infra-hetzner/README.md)**

Machine access = your personal SSH key + mTLS client cert, both distributed
via the team password manager (see that README).

---

## Cloudflare edge

DNS record + WAF + API-origin mTLS lock for `api-sorobanscan.rumblefishdev.com`.
Terraform, run from a laptop, gated behind `terraform plan` flags:

➡️ **[`infra/cloudflare/README.md`](../infra/cloudflare/README.md)**

---

## Verify after deploy

| Surface      | Check                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------- |
| Frontend     | https://sorobanscan.rumblefish.dev loads (HTTP 200, `text/html`)                                                |
| API (legacy) | `curl -f https://api.sorobanscan.rumblefish.dev/health`                                                         |
| API (via CF) | `curl -f https://api-sorobanscan.rumblefishdev.com/health`                                                      |
| API docs     | https://api.sorobanscan.rumblefish.dev/api-docs (Swagger UI)                                                    |
| CH box       | see [`infra-hetzner/README.md` § Post-deploy verification](../infra-hetzner/README.md#post-deploy-verification) |
| Alarms       | CloudWatch → `Explorer-production-CloudWatch` dashboards; no alarms in ALARM                                    |

---

## Rollback

- **AWS stack:** check out the last-good commit, `make -C infra build`,
  redeploy the affected stack. CDK has no app-level rollback; a _failed_
  deploy auto-rolls-back at the CloudFormation layer, but a _successful_
  bad deploy is undone by redeploying the previous code.
- **Via CI:** `workflow_dispatch` the release workflow from the last-good ref —
  it checks that ref out and redeploys the stack you name. There is no "re-tag"
  rollback: rolling back is always deploying the previous code forward.
- **Frontend:** `make -C infra deploy-production-web` from the good commit — the
  rebuild picks the Turnstile key up from `production.json`, and the arming
  guard refuses to sync a bundle that is missing it.
- **Galexie:** set `galexieImageTag` back to the previous digest +
  `deploy-production-ingestion`.
- **Machine:** `ansible-playbook … --tags app` from a good checkout; data
  restore is Borg (see infra-hetzner DR).
- **After a protocol vote, "the previous code" has a floor.** Every build
  before the `stellar-xdr` bump for the new protocol fails to decode its
  ledgers — the indexer dead-letters (task 0368), the API loses the archive
  block of new transactions, and `backfill-runner` cannot re-ingest the
  range. Roll Compute and `backfill-runner` back no further than the first
  build carrying the matching `stellar-xdr` pin, and never roll Galexie back
  past the vote (a pre-vote core stops exporting, task 0367). Before each
  vote, record which commit is that floor, so a rollback does not have to
  work it out under pressure.

---

## Config vs secrets

- **Non-secret config** — `infra/envs/production.json` (domains, sizes,
  thresholds, Galexie digest, concurrency knobs). Committed; changing it +
  redeploying is how you tune prod.
- **Secrets** — AWS Secrets Manager (per-service mTLS bundles) + the team
  password manager (box env). **Never** in the repo, CDK context, or
  workflow YAML.
- **CI deploy secrets** — the GitHub `production` environment carries only
  `AWS_DEPLOY_ROLE_ARN` (+ `AWS_ACCOUNT_ID`) for OIDC role assumption. No
  application secret and no `VITE_*` value belongs there: the SPA build reads
  the API domain and the (public) Turnstile **site** key out of
  `production.json`.
