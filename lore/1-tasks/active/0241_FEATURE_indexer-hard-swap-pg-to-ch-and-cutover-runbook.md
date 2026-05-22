---
id: '0241'
title: 'FEATURE: Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation'
type: FEATURE
status: active
related_adr: ['0044', '0045', '0047']
related_tasks: ['0206', '0228', '0233', '0239', '0240', '0242']
blocked_by: ['0228', '0239']
tags:
  [
    priority-high,
    effort-large,
    layer-indexer,
    layer-data,
    clickhouse,
    hetzner,
    live-ingest,
    cutover,
    hard-swap,
  ]
milestone: 1
links:
  - crates/indexer/src/handler/persist/mod.rs
  - crates/db-clickhouse/src/persist.rs
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Closes the D1 AC #2 gap
      ("ledgers table no gaps through current tip") after the PG→CH pivot.
      Currently `crates/indexer/Cargo.toml` does not depend on `db-clickhouse`
      — the indexer Lambda writes to PG only. Decision: hard swap CH-only
      (single PR cutover, no dual-write transition). Task covers code change
      in the indexer crate + operator runbook + empirical cutover validation.
  - date: '2026-05-20'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active in prep-work mode. Real blockers (0228 parallel-backfill
      merge in flight, 0239 AWS-side cutover backlog) prevent deploy, but Part A
      (code change in crates/indexer/) is doable solo against the local CH pilot
      Docker stack (per 0204). Part B (runbook) and Part C (empirical validation)
      remain gated on blocker resolution. PR will be opened as draft with a
      "do not merge until 0228 + 0239 ready" marker.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Scope expanded: 0241 now also covers the operator-side AWS-cutover steps
      that 0239 leaves on the table (Part D below). 0239 delivers only the
      CDK / TypeScript refactor — bootstrap of eu-central-1, issuance of mTLS
      client certs, upload to Secrets Manager, `CLICKHOUSE_CN_USER_MAP`
      update + ansible replay, and the end-to-end smoke per Lambda + Galexie
      are absorbed here. Reasoning: 0239 deploy without indexer Rust on CH
      would crash on cold-start (no RDS endpoint to connect to), so the
      first realistic deploy of the new region is when 0241 Part A lands.
      Bundling the operator steps with the cutover keeps "first prod deploy"
      a single coordinated event instead of two.
---

# Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation

## Summary

After the pivot to Hetzner ClickHouse as the prod data store (ADR 0044/0045 +
ADR 0047), the indexer Lambda must write to CH instead of PG. Team decision
(2026-05-20): hard swap, no dual-write. Task covers (A) code change in
`crates/indexer/`, (B) operator runbook for the cutover, (C) empirical
validation against the live pipeline.

## Context

D1 AC #2 requires "ledgers table no gaps through current tip". Currently:

- `crates/indexer/Cargo.toml` does not include a `db-clickhouse` dep → the
  indexer Lambda writes to PG only.
- `crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()` is a
  15-step PG flow inside a single BEGIN/COMMIT transaction.
- `crates/db-clickhouse/src/persist.rs` exposes `PartitionWriter`
  (production-grade) and `persist_ledger_clickhouse` (legacy/test wrapper).
- 0228 (parallel-backfill merge) ends historical at `L_last_closed` — no
  mechanism in place for ledgers `[L_last_closed + 1, current_tip]`.

The API continues to read PG (sqlx) after deploying 0241 — that is acceptable.
The API "stale window" lasts until M2 task 0243 (API feature flag). M1 =
write-path correctness, D2 = read-path correctness.

## Implementation Plan

### Part A — code (`crates/indexer/`)

1. **Cargo.toml**: add `db-clickhouse = { path = "../db-clickhouse" }` and
   `clickhouse = { workspace = true }`.
2. **`crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()`** —
   replace the 15-step PG flow with a call into
   `db_clickhouse::persist::PartitionWriter` (production interface). Gap
   analysis in the PR: does `PartitionWriter` cover all 15 PG writers
   (`upsert_accounts`, `insert_ledger`, …, `recompute_asset_aggregates` per
   `persist/mod.rs:223`).
3. **Idempotency**: CH retry strategy for HTTP errors (timeout / 5xx) —
   backoff schedule [50, 200, 800] ms (mirrors the PG retry shape). Replay
   safety via `ReplacingMergeTree(version)` semantics
   (version = `(ledger_seq, ingest_ts)`).
4. **Error handling**: CH unreachable = **fail loud** (Lambda returns an
   error, S3 retry handles re-delivery). No PG fallback.
5. **mTLS client config**: read cert + key + ca from Secrets Manager
   (depends on 0239 Phase 2). Env var `CH_PROD_DOMAIN` + mounted secret
   bundle.
6. **Cleanup**: remove the `sqlx` dep from `crates/indexer/Cargo.toml` if no
   other module in the crate needs it after the refactor.

### Part B — runbook (`docs/runbooks/live-tail-cutover.md`)

Step-by-step operator instructions:

1. **Pre-flight checks (T-0)**:
   - Verify 0228 merge complete: `clickhouse-client -q "SELECT max(sequence) FROM ledgers"`
     → expect = `L_last_closed`
   - Verify CH endpoint reachable: `curl -k https://ch-prod.../ping` → 200
   - Verify Lambda 0241 deployed: `aws lambda get-function ...` → expect post-0241 version
   - Verify mTLS cert in Secrets Manager: `aws secretsmanager get-secret-value ...`
2. **Cutover (T+0)**:
   - Enable indexer Lambda S3 trigger
   - Watch CloudWatch metric `ledger_processed_count` for 5 min — expect monotonic
3. **Verification (T+30 min)**:
   - Gap check: `SELECT count(*) FROM ledgers WHERE sequence BETWEEN ... AND ...`
   - Dedup check: 1 row per sequence
   - `MAX(sequence)` matches stellarchain.io tip within 30 s
4. **Rollback (if needed)**:
   - Disable Lambda trigger
   - Roll back to pre-0241 Lambda version
   - Document manual re-replay from the S3 backlog before the next attempt
5. **Monitoring 24 h post-cutover**:
   - CloudWatch alarms (GalexieLagAlarm, custom CH-write-error alarm)
   - CH disk usage growth rate — expect ~linear
   - Ledger lag metric — expect <30 s steady state

### Part C — empirical validation

- Execute the cutover on staging-Hetzner (or directly on prod if single-shot).
- Capture observations in the runbook (timings, surprises, edge cases) —
  mirrors the task 0233 pattern of "best executed alongside the first real
  cutover".
- Write lessons learned back into the runbook post-execution.

### Part D — AWS-side first-deploy operator steps (absorbed from 0239)

0239 delivered the CDK code (Lambdas out-of-VPC, Galexie public subnet,
mTLS wiring, region `eu-central-1`, RDS/bastion stacks dropped) but no
prod has ever been deployed in AWS for this project (per task 0249
archive — `validateConfig` blocked deploy with `hostedZoneId: CHANGE_ME`).
The first realistic deploy is the live-tail cutover this task covers,
so the operator prerequisites land here:

#### D-1 — CDK bootstrap of eu-central-1 (one-time)

Use the production AWS account ID — extract it from
`infra/envs/production.json:cloudFrontCertificateArn` (the third segment
of the ARN). Substitute it for `<account-id>` below.

```bash
aws sts get-caller-identity     # confirm it matches <account-id>
cd infra
npx cdk bootstrap aws://<account-id>/eu-central-1
```

#### D-2 — ECR image for Galexie in eu-central-1 (one-time)

The Galexie ECR repo is created by the first `make deploy-production-ingestion`.
After that, push the production Galexie image (matching `galexieImageTag`
in `infra/envs/production.json`, default `"latest"`) to it. Use the CI
pipeline once available; for the first deploy, push manually from a
local build or mirror from the prior us-east-1 ECR (if image still
exists locally).

#### D-3 — SSM parameter for Hetzner box IPv4

```bash
aws ssm put-parameter \
  --region eu-central-1 \
  --name /soroban/production/ch-ip \
  --value <hetzner-box-ipv4> \
  --type String
```

`HetznerDnsStack` reads this at deploy time and creates the Route 53
A record for `ch.sorobanscan.rumblefish.dev`.

#### D-4 — Issue + upload mTLS client certs (6 services)

Per `infra-hetzner/ca/README.md`. Linux-only because the script uses
`/dev/shm` (tmpfs) for the CA key.

```bash
# 1. Fetch CA key from password manager into tmpfs
mkdir -p /dev/shm/soroban-ca && chmod 0700 /dev/shm/soroban-ca
# (paste `soroban-prod / ca-key` contents into /dev/shm/soroban-ca/ca.key)
chmod 0600 /dev/shm/soroban-ca/ca.key

# 2. Issue all 6 certs
cd infra-hetzner/ca
for cn in lambda-api-production \
          lambda-ingestion-production \
          lambda-partition-production \
          lambda-migration-production \
          lambda-enrichment-production \
          galexie-production; do
  ./issue-client-cert.sh "$cn"
done

# 3. Assemble {cert,key,ca} JSON bundle in memory and pipe directly
#    to AWS CLI — the bundle JSON itself never lands on disk
#    (file:///dev/stdin reads from the pipe). The per-cert PEM files
#    that issue-client-cert.sh emits to ${SCRIPT_DIR}/out/<cn>/ ARE
#    disk-backed however (script honours SCRIPT_DIR, not /dev/shm,
#    for its final output) — they get shredded in step 4 below.
#    Follow-up task 0253 should consider making OUT_DIR overridable
#    so per-cert keys never touch disk in the first place.
for cn in lambda-api-production \
          lambda-ingestion-production \
          lambda-partition-production \
          lambda-migration-production \
          lambda-enrichment-production \
          galexie-production; do
  python3 -c "
import json, sys
cn = sys.argv[1]
cert = open(f'out/{cn}/{cn}.crt').read()
key  = open(f'out/{cn}/{cn}.key').read()
ca   = open('ca.crt').read()
print(json.dumps({'cert': cert, 'key': key, 'ca': ca}))
  " "$cn" | aws secretsmanager create-secret \
    --region eu-central-1 \
    --name "soroban/production/mtls/$cn" \
    --secret-string file:///dev/stdin
done

# 4. Securely destroy on-disk artefacts. shred overwrites file blocks
#    before unlinking, mitigating forensic recovery from filesystem
#    journals / SSD wear-levelling reserves. The ca.key on tmpfs gets
#    shredded too (overkill on tmpfs but cheap defence-in-depth).
find out -type f -print0 | xargs -0 shred -u
find out -depth -type d -empty -delete
shred -u /dev/shm/soroban-ca/ca.key
rmdir /dev/shm/soroban-ca 2>/dev/null || true
```

#### D-5 — Register CNs on Hetzner box

Update `~/.config/soroban-prod.env` — append six entries to
`CLICKHOUSE_CN_USER_MAP`. Map per the task 0240 RBAC user matrix
(`docs/architecture/security/clickhouse-rbac.md`); current expected
shape:

```
lambda-api-production:api_reader,
lambda-ingestion-production:indexer,
lambda-partition-production:partition_writer,
lambda-migration-production:migration_full,
lambda-enrichment-production:enrichment_writer,
galexie-production:galexie
```

Then replay ansible with the narrow `caddy_reload` tag — this re-renders
the CN map snippet and reloads Caddy without touching the rest of the
container stack (smaller blast radius than `--tags app`):

```bash
source ~/.config/soroban-prod.env
cd infra-hetzner/ansible
ansible-playbook -i inventory.ini site.yml --tags caddy_reload
```

#### D-6 — Initial deploy of the new region

After all of the above plus Part A (indexer code on CH) and 0228
(parallel-backfill merge) are in place:

```bash
cd infra
make deploy-production
```

CDK applies stacks in dependency order: Network → LedgerBucket →
Migration → Partition → Compute → Ingestion → Delivery →
Observability → ApiGateway → CloudWatch → HetznerDns.

`MigrationStack` runs the CH schema migrations as a CloudFormation
custom resource; failure blocks deploy of the downstream stacks.

#### D-7 — End-to-end smoke per AWS service

For each of the 6 services exercise a real ClickHouse query through
the mTLS path and verify the corresponding CN appears in Caddy access
logs on the Hetzner box.

```bash
# API Lambda — invoke through API Gateway
curl https://api.sorobanscan.rumblefish.dev/ledgers?limit=1

# Indexer Lambda — upload a known ledger to the S3 trigger
aws s3 cp test.xdr.zst s3://production-stellar-ledger-data/test/

# Enrichment Worker — enqueue a test message
aws sqs send-message --queue-url <enrichment-queue-url> --message-body '...'

# Migration Lambda — already invoked by CDK custom resource at deploy
# Partition Lambda — already invoked by CDK custom resource at deploy
# Galexie — bumped `galexieDesiredCount` to 1 in production.json and redeployed
#           ingestion stack; check `aws logs tail /ecs/production/galexie-live`
```

On the Hetzner box, confirm each service shows up in Caddy logs with
its expected `X-Client-Subject: CN=<service>-production`:

```bash
ssh deploy@<hetzner-ip>
docker logs caddy 2>&1 | grep -oE 'CN=[^,"]+' | sort -u
# Expect at least:
#   CN=lambda-api-production
#   CN=lambda-ingestion-production
#   CN=lambda-partition-production
#   CN=lambda-migration-production
#   CN=lambda-enrichment-production
#   CN=galexie-production
```

#### D-8 — Negative test — off-allowlist CN gets 403

Issue a throwaway cert with a CN that is NOT in `CLICKHOUSE_CN_USER_MAP`,
attempt a connection, and verify Caddy returns 403 at the HTTP layer:

```bash
cd infra-hetzner/ca
./issue-client-cert.sh test-rogue-cert    # NOT added to the map

curl --cert out/test-rogue-cert/test-rogue-cert.crt \
     --key  out/test-rogue-cert/test-rogue-cert.key \
     --cacert ca.crt \
     https://ch.sorobanscan.rumblefish.dev/ping
# Expected: HTTP 403, NOT 200.

# Clean up
mv out/test-rogue-cert .trash/ 2>/dev/null || rm -rf out/test-rogue-cert
```

#### D-9 — Close task 0239

After Part D is done end-to-end, `0239`'s acceptance criteria can be
ticked off (NAT GW removed, RDS stack removed, etc. are already true
in code; smoke items become satisfied by D-7 / D-8). Then `git mv
lore/1-tasks/active/0239_*.md lore/1-tasks/archive/`, status →
completed, history entry pointing at the cutover date.

## Acceptance Criteria

- [ ] `cargo check -p indexer` clean with no `sqlx` dep
- [ ] Lambda deploy with mTLS connection to Hetzner CH (env var `CH_PROD_DOMAIN` + mounted secret)
- [ ] Smoke test: ledger N writes to CH, query `SELECT * FROM ledgers WHERE sequence = N` returns the row
- [ ] 39 existing indexer tests rewritten or gated (CH-only test path)
- [ ] Replay safety: re-delivering an S3 event = no duplicates in CH (`ReplacingMergeTree(version)` verified)
- [ ] Error path: CH unreachable → Lambda fails, CloudWatch logs "ClickHouse unreachable", S3 retry kicks in (verified via toxiproxy or manual CH stop)
- [ ] `docs/runbooks/live-tail-cutover.md` authored and reviewed
- [ ] Cutover executed empirically: no ledger gap, no double-write corruption
- [ ] Monitoring: CloudWatch metric "ledger lag" <30 s post-cutover (matches D3 AC #1)
- [ ] Rollback path documented and test-runed
- [ ] Lessons learned written into the runbook (post-execution edit)
- [ ] **Docs updated** — `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      reflects the CH write path (replaces the PG write path description)
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**`,
      `Cargo.{toml,lock}` (root), or `libs/api-types/**`

### Part D acceptance criteria (absorbed from 0239)

- [ ] `cdk bootstrap aws://<account-id>/eu-central-1` complete
      (extract `<account-id>` from `infra/envs/production.json`)
- [ ] Galexie ECR image pushed to eu-central-1 ECR
- [ ] SSM parameter `/soroban/production/ch-ip` populated with the
      Hetzner box's public IPv4
- [ ] 6 mTLS client certs issued (`lambda-api-production`,
      `lambda-ingestion-production`, `lambda-partition-production`,
      `lambda-migration-production`, `lambda-enrichment-production`,
      `galexie-production`) and uploaded to AWS Secrets Manager under
      `soroban/production/mtls/<cn>`
- [ ] All 6 CNs registered in `CLICKHOUSE_CN_USER_MAP` on the Hetzner
      box and the Caddy snippet picked up the change (verified by
      `docker logs caddy 2>&1 | grep cn_user_map.snippet`)
- [ ] `make deploy-production` succeeds end-to-end (all 11 stacks
      CREATE_COMPLETE)
- [ ] Each AWS service successfully exercises a CH query and the
      expected `X-Client-Subject: CN=<service>-production` appears in
      Caddy access logs on the box
- [ ] Off-allowlist CN gets 403 at the HTTP layer (verified with a
      throwaway cert)
- [ ] Task 0239 moved to `archive/` with `status: completed` after
      Part D acceptance items are ticked

## Depends on

- **0239 Phase 2** (mTLS connection layer for AWS Lambdas → Hetzner CH) — technical blocker
- **0228** (historical CH ready as a baseline; cutover without a working merge has no value) — technical blocker
- **0233** (merge runbook — pairs with the live-tail runbook, complementary docs) — soft dependency
- **0242** — NOT a blocker. ADR ratification is post-factum documentation per
  the lore convention (`lore/2-adrs/CLAUDE.md`: "Written post-factum after
  implementation."). 0241 code can ship before 0242's ADR.

## Open questions

- **`PartitionWriter` vs `persist_ledger_clickhouse` wrapper**: the wrapper is
  "for legacy/test single-ledger callers"; production should drive
  `PartitionWriter`. May require a small refactor of the `db-clickhouse`
  interface.
- **CH replay semantics**: if `ReplacingMergeTree(version)` is not enough for
  idempotency (e.g. version collision on re-delivery), a sentinel design is
  needed: `INSERT IGNORE`, dedup table, or query-time dedup. Decision in PR.

## Notes

After deploying 0241, PG no longer receives new ledgers — by design ("hard
swap"). The API still reads PG until M2 (task 0243 feature flag). The "stale
window" is accepted by the team as a trade-off for skipping the dual-write
transition.
