# Runbook: live-tail cutover (PG → CH)

**Task:** [0241 — Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation](../../lore/1-tasks/active/0241_FEATURE_indexer-hard-swap-pg-to-ch-and-cutover-runbook.md)
**Target:** Production `indexer` Lambda in eu-central-1, writing to Hetzner ClickHouse via Caddy mTLS reverse proxy (`ch.sorobanscan.rumblefish.dev`)
**Idempotent:** yes — `ReplacingMergeTree` collapses replays in the 17 RMT tables; `ledgers` is plain MergeTree by design (see Part B-2 `count(DISTINCT sequence)` note); per-ledger commit isolation means already-committed earlier ledgers in the batch stay committed. The in-band `[50, 200, 800] ms` envelope retries the failing ledger on **transient** errors only; on a hard failure the Lambda returns Err, S3 redelivers the whole batch, and `ReplacingMergeTree` dedupes the replayed rows
**Frequency:** one-shot (cutover) — Part D (first-time region setup) is true one-time; Parts B+C repeat only on a rollback + re-cut

---

## Sections

- [Operator pre-requisites](#operator-pre-requisites) — skills, access, comm plan, window
- [Part A — code summary (already merged)](#part-a--code-summary-already-merged)
- [Part D — AWS-side first-deploy operator steps](#part-d--aws-side-first-deploy-operator-steps) — execute BEFORE Parts B+C on a fresh region
- [Part B — cutover steps](#part-b--cutover-steps) — pre-flight, T+0, T+30, rollback
- [Part C — empirical validation + lessons learned](#part-c--empirical-validation--lessons-learned) — captured post-execution
- [Disaster scenarios](#disaster-scenarios) — out-of-band failures and recovery paths

---

## Operator pre-requisites

### Required skills

The operator running this cutover must be comfortable with:

- **Linux shell** on the operator workstation (the cert-issuance pass
  in D-4 uses `/dev/shm` tmpfs and is Linux-only).
- **AWS CLI v2** with credentials for the production account (IAM
  role assumed via SSO or static creds).
- **`ssh`** to the Hetzner box (`deploy@<hetzner-ip>`) with the keypair
  recorded in the team password manager.
- **`docker exec`** + **`clickhouse-client`** to query the CH instance
  through the running container on the box.
- **`ansible`** for the Caddy snippet reload in D-5
  (`infra-hetzner/ansible/site.yml`).
- **`make` + CDK** to drive `make deploy-production` in D-6.
- **`cargo-lambda`** to build the rollback package in B-3.0 (install
  via `cargo install cargo-lambda` if not already present).
- **`jq`** for the JSON parsing in B-0 pre-flight checks.
- **Rust toolchain** for emergency fix-forward builds.

### Required access

Before the cutover begins, confirm the operator has:

- **AWS production account** — IAM identity with permissions covering
  the 11 stacks in `infra/src/lib/stacks/` (Lambda, S3, Secrets
  Manager, SSM, ECR, CloudFormation, CloudWatch, Route 53, SNS).
- **Hetzner box** — SSH key on `deploy@<hetzner-ip>` (key location is
  documented in the team password manager under
  `soroban-prod / hetzner-deploy-ssh`).
- **Internal CA key** — stored in the team password manager under
  `soroban-prod / ca-key`. Required for D-4 cert issuance; never
  written to persistent storage (only `/dev/shm` tmpfs).
- **Team password manager** — the team uses 1Password under the
  `soroban-prod` vault. Cert / SSH / CA material lives there.
- **Stellar passphrase** — `Public Global Stellar Network ; September
2015` for mainnet (already in `infra/envs/production.json` under
  `stellarNetworkPassphrase`; the indexer Lambda reads this via the
  CDK-injected `STELLAR_NETWORK_PASSPHRASE` env var).

### Communication plan

For a production cutover, schedule with these participants on a live
call (e.g. Slack huddle in `#soroban-prod`):

- **Operator (lead)** — runs the runbook step by step.
- **Backend engineer** — code owner of `crates/indexer`, available
  for fix-forward triage.
- **Infrastructure engineer** — owns `infra/`, available to override
  CDK / IAM if D-1..D-7 surfaces a config issue.
- **On-call rotation member** — receives the SNS alarms and watches
  CloudWatch in parallel.
- **Optional**: a frontend / API engineer to confirm read-path stays
  green (until 0243 the API still reads PG, so the stale window is
  expected — confirm the frontend gracefully degrades).

Escalation channel: the SNS topic created by `cloudwatch-stack.ts`
publishes to `#soroban-alarms` via Chatbot; severe issues escalate to
the team lead via direct DM.

### Maintenance window

Recommended window for the cutover:

- **Day**: Tuesday–Thursday (avoid Monday morning / Friday afternoon).
- **Time**: 09:00–11:00 CET — low explorer traffic, on-call team
  available in office hours.
- **Duration**: budget 2 hours for B-0..B-2, plus 24 h post-cutover
  monitoring (B-4). Part D operator setup (D-1..D-9) can be scheduled
  on a separate calmer day before the cutover proper.

Freeze policy: no other prod-touching deploys (frontend, backfill,
infra) for the duration of B-0..B-2.

---

## Part A — code summary (already merged)

Task 0241 Part A merged the following before this runbook fires:

- `crates/indexer/Cargo.toml`: dropped `sqlx`, added `db-clickhouse =
{ features = ["aws-mtls"] }` + `clickhouse = "=0.15.0"`. The legacy
  PG persist tree is feature-gated behind `pg-persist` (consumed only
  by `backfill-runner` / `backfill-bench`); the production Lambda
  binary builds with default features and contains no PG code.
- `crates/indexer/src/handler/mod.rs::process_s3_object` — per-ledger
  loop calling `db_clickhouse::persist::persist_ledger_clickhouse`
  (the same one-shot wrapper backfill's `Sink::persist_ledger`
  fallback uses). Per-ledger commit isolation: ledger N fail does
  not roll back already-committed N-1..1 in the same batch.
- Retry envelope `[50, 200, 800] ms` per ledger. `Network` /
  `TimedOut` always retry. `BadResponse` uses a **denylist**: retry
  unless the error is definitively permanent (HTTP 4xx or a known CH
  semantic exception code — `UNKNOWN_TABLE`, `TYPE_MISMATCH`,
  `CANNOT_PARSE_*`, auth, …). The `clickhouse` crate carries the raw
  body verbatim (no separate HTTP status), so an allowlist prefix
  match would miss 5xx-with-body and trip the DLQ — defaulting to
  retry favours availability. Permanent errors fail loud → Lambda
  surfaces → S3 redelivery → sustained outage → DLQ.
- mTLS client: `db_clickhouse::mtls::client_from_lambda_env` reads
  `MTLS_SECRET_NAME` + `CH_DOMAIN` at cold start, fetches the `{cert,
key, ca}` bundle from the **AWS Parameters and Secrets Lambda
  Extension** (`localhost:2773`), builds a `hyper-util` client with a
  rustls connector pinned to the bundle CA, and assembles a
  `clickhouse::Client` for `https://${CH_DOMAIN}`. Caddy maps the cert
  CN to a CH user via `CLICKHOUSE_CN_USER_MAP` and injects
  `X-ClickHouse-User: <mapped-user>` — the client passes no
  credentials on its own.

The `enrichment_publish` SQS publisher is **stubbed** post-cutover —
the PG lookup queries cannot run against a frozen PG, and
`enrichment-worker` still UPDATE's PG. Re-enablement waits on the
paired CH-aware rewrite of producer + worker.

---

## Part D — AWS-side first-deploy operator steps

Required when deploying to a region where the explorer has never run
before (eu-central-1 today). Steps run on the **operator's Linux
laptop** with AWS credentials for the production account loaded.

> **Linux-only:** D-4 uses `/dev/shm` (tmpfs) to keep the CA key off
> persistent storage. macOS users: borrow a Linux box for the cert
> issuance pass, or run inside a Linux VM.

### D-1 — CDK bootstrap of eu-central-1 (one-time)

Extract the production AWS account ID from `infra/envs/production.json`
(third segment of `cloudFrontCertificateArn`).

```bash
aws sts get-caller-identity     # confirm it matches <account-id>
cd infra
npx cdk bootstrap aws://<account-id>/eu-central-1
```

Expected: a single `CDKToolkit` stack `CREATE_COMPLETE` in
eu-central-1.

### D-2 — ECR image for Galexie in eu-central-1 (one-time)

The Galexie ECR repo is created by the first `make
deploy-production-ingestion` (covered by D-6 below). After that repo
exists, push the production Galexie image.

> ⚠️ **Pin `galexieImageTag` to an immutable SHA before deploying
> production.** The default value in `infra/envs/production.json` is
> `"latest"`, which is fine for staging / dev but anti-pattern in
> prod (a re-push of `:latest` upstream can silently change the
> running image on next ECS task replacement). Replace `latest` with
> the exact `sha256:…` digest of the image you push, e.g.
> `"galexieImageTag": "sha256:abc123…"`. The B-0 pre-flight check
> below blocks the cutover if `galexieImageTag` is still `"latest"`.

Until CI mirrors images cross-region automatically, push manually:

```bash
# Source image — either re-pull from upstream or, if the prior
# us-east-1 ECR still has the image locally, retag and push.
docker pull stellar/stellar-core-galexie:<version>

# Tag with both an immutable SHA and the canonical name for the
# initial push; subsequent ECS task definitions reference the SHA.
docker tag stellar/stellar-core-galexie:<version> \
           <account-id>.dkr.ecr.eu-central-1.amazonaws.com/production-galexie:<version>

aws ecr get-login-password --region eu-central-1 \
  | docker login --username AWS --password-stdin <account-id>.dkr.ecr.eu-central-1.amazonaws.com

docker push <account-id>.dkr.ecr.eu-central-1.amazonaws.com/production-galexie:<version>

# Read back the immutable digest and update production.json before
# `make deploy-production` (D-6):
aws ecr describe-images \
  --region eu-central-1 \
  --repository-name production-galexie \
  --image-ids imageTag=<version> \
  --query 'imageDetails[0].imageDigest' --output text
# Paste the returned `sha256:…` into `galexieImageTag` in
# infra/envs/production.json.
```

### D-3 — SSM parameter for the Hetzner box IPv4

`HetznerDnsStack` reads this at deploy time and creates the Route 53
A record for `ch.sorobanscan.rumblefish.dev`.

```bash
aws ssm put-parameter \
  --region eu-central-1 \
  --name /soroban/production/ch-ip \
  --value <hetzner-box-ipv4> \
  --type String
```

Source for `<hetzner-box-ipv4>`: Hetzner Robot dashboard → AX52 →
public IPv4.

### D-4 — Issue + upload mTLS client certs (6 services)

Per `infra-hetzner/ca/README.md`. The script keeps the CA key on
tmpfs; this section adds the AWS Secrets Manager upload.

```bash
# 1. Fetch CA key from password manager into tmpfs.
mkdir -p /dev/shm/soroban-ca && chmod 0700 /dev/shm/soroban-ca
# (paste `soroban-prod / ca-key` contents into /dev/shm/soroban-ca/ca.key)
chmod 0600 /dev/shm/soroban-ca/ca.key

# 2. Issue all 6 certs.
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
#    to AWS CLI. The bundle JSON never lands on disk (file:///dev/stdin
#    reads from the pipe). The per-cert PEM files that
#    issue-client-cert.sh emits to ${SCRIPT_DIR}/out/<cn>/ ARE
#    disk-backed however — they get shredded in step 4.
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

# 4. Securely destroy on-disk artefacts. `shred` overwrites file
#    blocks before unlinking, mitigating forensic recovery from
#    filesystem journals / SSD wear-levelling reserves. The ca.key on
#    tmpfs gets shredded too (overkill on tmpfs but cheap
#    defence-in-depth).
find out -type f -print0 | xargs -0 shred -u
find out -depth -type d -empty -delete
shred -u /dev/shm/soroban-ca/ca.key
rmdir /dev/shm/soroban-ca 2>/dev/null || true
```

The JSON shape matches what
`db_clickhouse::mtls::fetch_bundle_from_extension` expects — `{"cert":
"...", "key": "...", "ca": "..."}` — no `ch_user` / `ch_password`
fields are needed since Caddy injects the CN-mapped CH user.

### D-5 — Register CNs on Hetzner box

Update `~/.config/soroban-prod.env` — append six entries to
`CLICKHOUSE_CN_USER_MAP` per the [task 0240 RBAC user matrix](../architecture/security/clickhouse-rbac.md):

```text
lambda-api-production:api_reader,
lambda-ingestion-production:indexer,
lambda-partition-production:partition_writer,
lambda-migration-production:migration_full,
lambda-enrichment-production:enrichment_writer,
galexie-production:galexie
```

Then replay ansible with the narrow `caddy_reload` tag — re-renders the
CN map snippet and reloads Caddy without touching the rest of the
container stack:

```bash
source ~/.config/soroban-prod.env
cd infra-hetzner/ansible
ansible-playbook -i inventory.ini site.yml --tags caddy_reload
```

Verify the snippet was picked up:

```bash
ssh deploy@$HETZNER_IP 'docker logs caddy 2>&1 | tail -20 | grep -i reload'
```

### D-6 — Initial deploy of the new region

Pre-condition: D-1 through D-5 complete AND `develop` carries Part A
(indexer code on CH) AND task 0228 (parallel-backfill merge) is
finalised on Hetzner.

```bash
cd infra
make deploy-production
```

CDK applies stacks in dependency order: Network → LedgerBucket →
Migration → Partition → Compute → Ingestion → Delivery → Observability
→ ApiGateway → CloudWatch → HetznerDns.

`MigrationStack` runs CH schema migrations as a CloudFormation custom
resource; a failure there blocks deploy of all downstream stacks.

### D-7 — End-to-end smoke per AWS service

For each of the 6 services exercise a real CH query through the mTLS
path and verify the corresponding CN appears in Caddy access logs on
the Hetzner box.

```bash
# API Lambda — invoke through API Gateway.
curl https://api.sorobanscan.rumblefish.dev/ledgers?limit=1

# Indexer Lambda — upload a known ledger to the S3 trigger.
aws s3 cp test.xdr.zst s3://production-stellar-ledger-data/test/

# Enrichment Worker — enqueue a test message.
aws sqs send-message --queue-url <enrichment-queue-url> --message-body '...'

# Migration Lambda — already invoked by CDK custom resource at deploy.
# Partition Lambda — already invoked by CDK custom resource at deploy.
# Galexie — bump `galexieDesiredCount` to 1 in production.json and
#           redeploy ingestion; check `aws logs tail /ecs/production/galexie-live`.
```

On the Hetzner box, confirm each service shows up in Caddy access logs
with its expected `X-Client-Subject: CN=<service>-production`:

```bash
ssh deploy@$HETZNER_IP
docker logs caddy 2>&1 | grep -oE 'CN=[^,"]+' | sort -u
# Expect at least:
#   CN=lambda-api-production
#   CN=lambda-ingestion-production
#   CN=lambda-partition-production
#   CN=lambda-migration-production
#   CN=lambda-enrichment-production
#   CN=galexie-production
```

### D-8 — Negative test — off-allowlist CN gets 403

Issue a throwaway cert with a CN that is NOT in
`CLICKHOUSE_CN_USER_MAP`, attempt a connection, and verify Caddy
returns 403 at the HTTP layer:

```bash
cd infra-hetzner/ca
./issue-client-cert.sh test-rogue-cert    # NOT added to the map

curl --cert out/test-rogue-cert/test-rogue-cert.crt \
     --key  out/test-rogue-cert/test-rogue-cert.key \
     --cacert ca.crt \
     https://ch.sorobanscan.rumblefish.dev/ping
# Expected: HTTP 403, NOT 200.

# Clean up.
mv out/test-rogue-cert .trash/ 2>/dev/null || true
```

### D-9 — Close task 0239

Concretely:

1. Open `lore/1-tasks/archive/0239_FEATURE_aws-side-cutover-mtls-to-hetzner.md`.
   The frontmatter already has `status: completed` (set when 0239
   went to archive during code merge) — leave it as is.
2. In the "## Acceptance Criteria" section, find the items marked
   `[⏸️]` ("Deferred to task 0241 Part D"). Tick them `[x]` for each
   one you executed in D-1..D-8 above.
3. Append a `history:` entry at the bottom of the frontmatter:
   ```yaml
   history:
     ...existing entries...
     - date: '<YYYY-MM-DD of cutover>'
       status: completed
       who: <operator-handle>
       note: >
         Part D operator steps (D-1..D-8 of live-tail-cutover.md)
         executed end-to-end on <YYYY-MM-DD>. Deferred AC items
         ticked: <list the AC IDs that flipped from [⏸️] to [x]>.
   ```
4. Commit the change with a `lore-0239` scope:
   ```bash
   git add lore/1-tasks/archive/0239_FEATURE_aws-side-cutover-mtls-to-hetzner.md
   git commit -m "docs(lore-0239): tick deferred AC items after 0241 Part D execution"
   ```
   No PR needed (lore docs commit straight to develop per project
   convention) unless your team prefers PR review.
5. Verify: `git log -1 --stat lore/1-tasks/archive/0239_*.md` should
   show the new history entry and ticked checkboxes.

---

## Part B — cutover steps

> **Helper variables** — set once at the top of your operator shell;
> Part B steps reference `$HETZNER_IP` instead of `<hetzner-ip>`:
>
> ```bash
> export HETZNER_IP=$(aws ssm get-parameter \
>   --region eu-central-1 \
>   --name /soroban/production/ch-ip \
>   --query 'Parameter.Value' --output text)
> echo "HETZNER_IP=$HETZNER_IP"
> export INDEXER_FN=production-soroban-explorer-indexer
> ```

### B-0 — Pre-flight checks (T-0, run 30 min before cutover)

All checks must return their expected values. Any miss blocks the
cutover.

```bash
# 1. Historical backfill landed on Hetzner — last sealed ledger is
#    L_last_closed (62,527,999 per task 0228 archive).
ssh deploy@$HETZNER_IP 'docker exec clickhouse clickhouse-client \
  -q "SELECT max(sequence) FROM ledgers"'
# Expected: 62527999 (or whatever the merged backfill terminates at).

# 2. CH endpoint reachable from operator laptop via mTLS.
curl --cert out/lambda-api-production/lambda-api-production.crt \
     --key  out/lambda-api-production/lambda-api-production.key \
     --cacert infra-hetzner/ca/ca.crt \
     https://ch.sorobanscan.rumblefish.dev/ping
# Expected: HTTP 200, body "Ok.\n"

# 3. Indexer Lambda is deployed at the 0241 code version.
aws lambda get-function \
  --region eu-central-1 \
  --function-name production-soroban-explorer-indexer \
  --query 'Configuration.LastModified'
# Expected: timestamp ≥ 0241 merge commit.

# 3b. Indexer Lambda code matches the expected commit SHA — `LastModified` only
#     proves "something was deployed", not which build. CI tags every build with
#     its short SHA in the function description (e.g. "indexer@abc1234"); compare
#     against the SHA you intend to cut over from.
EXPECTED_SHA=<short-git-sha-of-the-build-you-want-to-cut-over>
aws lambda get-function-configuration \
  --region eu-central-1 \
  --function-name "$INDEXER_FN" \
  --query 'Description'
# Expected: description containing $EXPECTED_SHA (or fall back to comparing
# `CodeSha256` against your CI artifact's recorded base64 SHA256 if your
# pipeline records it instead of using the description).

# 4. mTLS secret is present and readable by the Lambda IAM role.
aws secretsmanager describe-secret \
  --region eu-central-1 \
  --secret-id soroban/production/mtls/lambda-ingestion-production \
  --query 'ARN'
# Expected: ARN matching `…secret:soroban/production/mtls/lambda-ingestion-production-XXXXXX`.

# 5. Galexie is live and producing ledger files into S3.
aws s3 ls s3://production-stellar-ledger-data/ledgers/ \
  --recursive | tail -5
# Expected: 5 files with mtime within the last few minutes.

# 6. CW alarm for Galexie ingestion lag is in OK state. The alarm's CW
#    name is set by CDK from the env-templated string in
#    `cloudwatch-stack.ts::GalexieLagAlarm` — for production that
#    resolves to "production-galexie-ingestion-lag", NOT the construct
#    ID. Use the CW name, otherwise `describe-alarms --alarm-names`
#    returns an empty list and `[0].StateValue` is null.
aws cloudwatch describe-alarms \
  --region eu-central-1 \
  --alarm-names production-galexie-ingestion-lag \
  --query 'MetricAlarms[0].StateValue'
# Expected: "OK".

# 7. Production prerequisite hygiene — fail the cutover if any block.
#    `jq -r '.foo'` on a missing field prints the literal string
#    "null"; the `case` form below catches null / missing / empty
#    explicitly so we don't silently pass when the JSON drifts.

VAL=$(jq -r '.galexieImageTag // empty' infra/envs/production.json)
case "$VAL" in
  ""|null|latest)
    echo "FAIL: galexieImageTag must be a sha256:... pin per D-2 (got: '$VAL')"
    exit 1 ;;
esac

VAL=$(jq -r '.hostedZoneId // empty' infra/envs/production.json)
case "$VAL" in
  ""|null|CHANGE_ME)
    echo "FAIL: hostedZoneId is unset or sentinel (got: '$VAL')"
    exit 1 ;;
esac

VAL=$(jq -r '.indexerLambdaConcurrency // empty' infra/envs/production.json)
[ "$VAL" = "0" ] && echo "INFO: indexerLambdaConcurrency=0 — S3 trigger not wired yet; flip to nonzero in D-6 deploy."

# 8. Rollback package for the current HEAD~1 commit exists in S3 — see
#    B-3.0. If absent, build + stage it BEFORE proceeding (the package
#    is the only fast-rollback path; `cdk synth` rebuilds take
#    10-30 min, unacceptable under incident).
PREV_SHA=$(git rev-parse --short HEAD~1)
aws s3 ls "s3://production-deploy-artifacts/indexer-rollback/${PREV_SHA}.zip" \
  --region eu-central-1 \
  || { echo "FAIL: rollback package indexer-rollback/${PREV_SHA}.zip not staged — run B-3.0 first"; exit 1; }
```

If any check fails: do **not** proceed. Open an incident, root-cause
the gap, retry after fix.

### B-1 — Cutover (T+0)

The indexer Lambda's S3 trigger is configured by CDK
(`compute-stack.ts:204-208`) — it is wired the moment
`reservedConcurrentExecutions > 0`. The cutover, then, is the act of
_enabling_ the Lambda. CDK currently sets this for prod; the cutover
moment is the deploy that flips the indexer code to the 0241 binary.

For a controlled cutover **without** code redeploy (e.g. running 0241
binary already deployed but with reservedConcurrentExecutions = 0):

```bash
# Flip reservedConcurrentExecutions to its production value.
aws lambda put-function-concurrency \
  --region eu-central-1 \
  --function-name production-soroban-explorer-indexer \
  --reserved-concurrent-executions <prod-concurrency>
```

Watch the first 5 minutes:

```bash
aws logs tail \
  /aws/lambda/production-soroban-explorer-indexer \
  --since 5m --follow
```

Expected log patterns:

- `"indexer cold start — building mTLS ClickHouse client"` — first invocation.
- `"indexer ready — starting Lambda runtime"` — within ~3–5 s of cold start.
- `"processing S3 record"` + `"S3 record processed"` per ledger batch.
- No `"failed to process S3 record"` errors.

Also watch the CW custom metric:

```bash
aws cloudwatch get-metric-statistics \
  --region eu-central-1 \
  --namespace SorobanBlockExplorer/Indexer \
  --metric-name LastProcessedLedgerSequence \
  --dimensions Name=Environment,Value=production \
  --start-time $(date -u -d '5 min ago' +%FT%TZ) \
  --end-time   $(date -u +%FT%TZ) \
  --period 60 --statistics Maximum
```

Expected: a monotonically increasing series, each datapoint ≥ previous.

### B-2 — Verification (T+30 min)

```bash
ssh deploy@$HETZNER_IP 'docker exec clickhouse clickhouse-client -q "
SELECT
    min(sequence)               AS first_post_cutover_seq,
    max(sequence)               AS last_post_cutover_seq,
    count(DISTINCT sequence)    AS distinct_rows,
    max(sequence) - min(sequence) + 1 AS expected
  FROM ledgers
  WHERE sequence > 62527999  -- L_last_closed; bump per actual backfill terminus
"'
```

Expected: `distinct_rows = expected` — zero gaps in the post-cutover range.

**Note on `count(DISTINCT sequence)` (vs plain `count()`):** `ledgers` is a
plain `MergeTree`, not `ReplacingMergeTree`. The commit-marker semantics
(`writer.rs` writes the `ledgers` insert last per batch) cover the common
failure modes, but a Lambda timeout that lands exactly between server-side
commit of the `ledgers` part and the client-side `Insert::end()` ack can
leave a duplicate `sequence` after S3 retry. The duplicates are bit-identical
under a single parser version (`ExtractedLedger` is deterministic), so the
gap-check stays meaningful when expressed against `count(DISTINCT sequence)`;
plain `count()` could either hide a gap that duplicates compensated for, or
flag a false drift when no gap exists. Same `DISTINCT` discipline applies to
any post-cutover sanity query against `ledgers`. If duplicates accumulate
visibly, `OPTIMIZE TABLE ledgers FINAL DEDUPLICATE BY sequence` collapses
them — defer to a maintenance window since `OPTIMIZE FINAL` is heavy.

Dedup invariant — `SELECT count() FROM ledgers FINAL` must equal
`SELECT count() FROM ledgers` (RMT collapsed any replay rows). The
[0228 phase-6 validation](0228_phase6_validation.md) script
`verify-completeness` covers this exact check:

```bash
cd crates/backfill-runner
cargo run --release --bin backfill-runner -- \
  verify-completeness \
  --target clickhouse \
  --first-ledger 62528000 \
  --last-ledger $(ssh deploy@$HETZNER_IP 'docker exec clickhouse clickhouse-client -q "SELECT max(sequence) FROM ledgers"')
```

Stellar tip parity:

```bash
TIP=$(curl -fsS --max-time 10 'https://horizon.stellar.org/ledgers?order=desc&limit=1' \
       | jq -r '._embedded.records[0].sequence // empty')
if [ -z "$TIP" ]; then
  echo "WARN: horizon.stellar.org unreachable — skipping parity check (re-run after Horizon recovers)"
else
  CH_TIP=$(ssh deploy@$HETZNER_IP \
    'docker exec clickhouse clickhouse-client -q "SELECT max(sequence) FROM ledgers"')
  echo "horizon: $TIP   ch: $CH_TIP   lag: $((TIP - CH_TIP))"
fi
```

Expected: lag ≤ 6 ledgers (~30 s at 5 s ledger close time, allowing
Galexie + S3 propagation + Lambda processing).

### B-3 — Rollback (if needed)

**Important context — `cdk synth` cross-compiles every Rust Lambda
for aarch64 from scratch, ~10–30 min cold.** Under prod incident
that's not a viable rollback path. Pre-stage a known-good Lambda
package in S3 before cutover so rollback is a 30-second
`update-function-code` call.

#### B-3.0 — One-time prep (before cutover)

```bash
# 0. One-time bucket bootstrap (per AWS account / region). The
#    rollback artifacts bucket lives outside CDK by design — it has
#    to exist BEFORE any code deploy so a rollback during bootstrap
#    itself has somewhere to land. Skip this step if you've already
#    bootstrapped the region for previous cutovers.
aws s3api head-bucket \
  --region eu-central-1 \
  --bucket production-deploy-artifacts 2>/dev/null \
  || aws s3 mb s3://production-deploy-artifacts --region eu-central-1
aws s3api put-bucket-versioning \
  --region eu-central-1 \
  --bucket production-deploy-artifacts \
  --versioning-configuration Status=Enabled

# 1. Build the Lambda package locally for aarch64. `--output-format
#    Zip` is REQUIRED — without it cargo-lambda emits a raw `bootstrap`
#    binary and `update-function-code --s3-key …zip` later fails with
#    "Could not unzip uploaded file".
cd crates/indexer
cargo lambda build --release --arm64 --output-format Zip
#    Output: <repo-root>/target/lambda/indexer/bootstrap.zip
cd -

# 2. Upload to a rollback-staging S3 prefix (use a versioned name
#    keyed by the commit SHA so multiple rollback packages can coexist).
SHA=$(git rev-parse --short HEAD)
aws s3 cp target/lambda/indexer/bootstrap.zip \
  s3://production-deploy-artifacts/indexer-rollback/${SHA}.zip \
  --region eu-central-1

# 3. Record the SHA in operator notes. The B-0 pre-flight check #8
#    automatically verifies that the previous-commit package is in
#    place before the next cutover proceeds.
echo "Rollback package: s3://production-deploy-artifacts/indexer-rollback/${SHA}.zip"
```

Re-run before every cutover so the rollback package matches the code
state right before the change being deployed. The B-0 pre-flight
check #8 verifies the previous-commit package is staged.

#### B-3.1 — Stop the Lambda

```bash
aws lambda put-function-concurrency \
  --region eu-central-1 \
  --function-name production-soroban-explorer-indexer \
  --reserved-concurrent-executions 0
```

S3 events accumulate in the bucket (and the Lambda DLQ collects
failures). Galexie keeps producing.

#### B-3.2 — Pin to pre-staged rollback package

```bash
# Replace <SHA> with the value from B-3.0 step 3.
aws lambda update-function-code \
  --region eu-central-1 \
  --function-name production-soroban-explorer-indexer \
  --s3-bucket production-deploy-artifacts \
  --s3-key indexer-rollback/<SHA>.zip

# Wait for the update to complete (typically < 30 s).
aws lambda wait function-updated \
  --region eu-central-1 \
  --function-name production-soroban-explorer-indexer
```

**Caveat — there is no clean back-out to a working state.** The
pre-0241 Lambda code targets Postgres-on-RDS, which is decommissioned
(task 0239). Rolling back to an older 0241-era package gives a
Lambda that talks to CH but with whatever bug you're rolling away
from. The real recovery is **fix-forward**: identify the issue,
patch, rebuild, push a new rollback package, deploy. B-3.2 exists for
incidents where the operator needs the Lambda _quiet_ while
triaging.

#### B-3.3 — Drain the S3 backlog after fix-forward

New ledgers continue to arrive; if the Lambda was paused for `N`
minutes at Galexie cadence (≈ 12 ledgers/min on pubnet), expect
`N × 12` extra S3 events when concurrency is restored. Watch the CW
`Invocations` metric stabilise; if backlog grows faster than drain,
temporarily raise `reservedConcurrentExecutions` (after confirming
Hetzner CH can absorb the parallel write rate — see [the CH writer
docs](../../crates/db-clickhouse/src/persist/writer.rs)).

**Retry / DLQ topology (production config)**: `infra/envs/production.json`
sets `indexerLambdaRetryAttempts: 0` — the indexer Lambda is invoked
**asynchronously** by S3 and the EventInvokeConfig has zero retries
configured, so any hard failure goes straight to the DLQ. Built-in
S3 notification redelivery still applies (S3 retries with
exponential backoff for hours before giving up), so transient CH
errors that exceed the in-band retry envelope (`[50, 200, 800] ms`)
re-deliver via the S3 path, not via Lambda's own retry config. Drain
rate after concurrency restore: `reservedConcurrentExecutions × (60 /
avg_processing_ms_per_ledger)`. With typical p50 ~500 ms per ledger
and `reservedConcurrentExecutions = 1`, that's ~120 ledgers/min —
plenty of headroom over the 12/min steady-state. For a 30-min
outage, ~360 events queue up; clear in ~3 min at single-concurrency
or instantly at 2-3. **Do not raise concurrency above ~3 without
checking CH `system.merges` count first** — concurrent writers
inflate part counts faster than the merger consolidates.

### B-4 — Monitoring 24 h post-cutover

Operator on-call until T+24h. Watch:

- `<env>-galexie-ingestion-lag` — must stay OK (alarm `alarmName` set
  by `cloudwatch-stack.ts::GalexieLagAlarm`, NOT the construct ID).
  For production: `production-galexie-ingestion-lag`.
- `<env>-indexer-ch-write-failures` alarm (CDK
  `IndexerChWriteFailureAlarm` in `cloudwatch-stack.ts`) — a log
  MetricFilter on the indexer log group matching the exact JSON
  `$.fields.message` values `"failed to process S3 record"` (the
  terminal per-batch failure) and `"failed to build mTLS CH client"`
  (cold-start mTLS init failure). Fires when their summed count is
  `> 10` in a 5-minute window. **Counts only post-retry hard failures**
  — a 5xx burst that the in-band retry envelope recovers from emits no
  such line, so a recovered ledger does NOT increment this metric.
  Pages via the same SNS topic / Slack channel as the other alarms.
- `<env>-ledger-processor-error-rate` alarm — Lambda `Errors` /
  `Invocations` ratio; complements the log-pattern alarm above (the
  in-band retry envelope can succeed without spiking this rate even
  when individual ledgers retry several times).
- `<env>-ledger-processor-dlq-depth` alarm — any DLQ message = ledger
  permanently failed after all S3-redelivery attempts.
- CH disk usage growth — `df -h /var/lib/clickhouse` on the box.
  Expected: ~linear at steady-state pubnet ledger size (≈ 50–200 MiB /
  hour).
- Ledger lag derived metric:
  ```
  horizon_tip - CH_max(sequence)
  ```
  Expected: < 30 s. Sustained > 60 s for > 5 min → page.

#### Known stale fields post-cutover (NOT a regression)

- `assets.holder_count` and `assets.total_supply` are NULL or stale for
  every asset touched by a post-cutover ledger. Reason: PG had a per-
  ledger `recompute_asset_aggregates` step
  (`crates/indexer/src/handler/persist/write.rs`) that the CH writer
  does not yet mirror — flagged out-of-scope of task 0241 and tracked
  for a CH-port follow-up. The placeholder comment is in
  `crates/backfill-runner/src/bootstrap.rs:67-74`. The underlying
  data (`account_balances_current`, `assets`) is complete; only the
  precomputed aggregate is missing. Read-path consumers of these
  columns must either tolerate NULL or compute on demand until the
  CH-port lands. Do not page on this.

---

## Part C — empirical validation + lessons learned

> Filled in **after** the live cutover. Drop-in: append a dated section
> per attempt — `## YYYY-MM-DD attempt N`.

Recommended capture surface per attempt:

- T-0 → T+0 wall clock and exact L_last_closed at cutover.
- First post-cutover ledger sequence successfully landed in CH.
- Mean / p99 `persist breakdown` log timings from the indexer for the
  first ~30 minutes (`ledger saved to database` lines).
- Any retry escalations observed (search logs for `"batch hit
transient CH error — retrying"`).
- Drift between Horizon tip and CH tip at T+5, T+30, T+60, T+1440
  minutes.
- Caddy access-log slice from the Hetzner box for the indexer CN
  (`CN=lambda-ingestion-production`).
- Anything that surprised the operator — even if it was already in
  this runbook.

### Operator surprises file

If the runbook needs an edit (a step was wrong, a check was missing, a
gotcha shows up that wasn't here), file a PR against this file in the
same commit as the post-mortem notes — keep it the single source of
truth.

---

## Disaster scenarios

Out-of-band failures the cutover itself cannot mitigate. Each section
states the **trigger**, **impact**, and **recovery path** so the
on-call doesn't reinvent under pressure.

### DR-1 — Hetzner box becomes unreachable

**Trigger**: power outage, network partition, hardware failure on the
single AX52 hosting CH + Caddy.

**Impact**: every indexer Lambda invocation fails on the mTLS write
(`ClickHouse write failed` / `Network`). After
`indexerLambdaRetryAttempts` retries each event lands in the DLQ.
S3 bucket continues to accumulate Galexie's exports.

**Recovery**:

1. Disable the indexer Lambda concurrency (B-3.1) to stop the DLQ
   from flooding.
2. Triage Hetzner via Robot dashboard / Hetzner support. If the box
   is hard-down, restore via:
   - **A**: file a Robot support ticket for hardware replacement.
     RTO 4–24 h depending on Hetzner response.
   - **B**: provision a new box per `infra-hetzner/README.md`, restore
     CH state from the BX21 Borg backup (per task 0228 follow-up;
     ledger range up to last Borg snapshot ≈ 1 h RPO).
3. Once box is back, update SSM `/soroban/production/ch-ip` (D-3) if
   IPv4 changed; re-deploy Route 53 record via `make deploy-production`
   (HetznerDnsStack picks up the new IP).
4. Re-enable Lambda concurrency. S3 backlog drains per B-3.3.

**Open items**: no automated failover. Hot standby on a second box
is the eventual hardening (out of scope for 0241).

### DR-2 — Secrets Manager mTLS entry deleted or corrupted

**Trigger**: accidental `aws secretsmanager delete-secret`, key
rotation gone wrong, malformed JSON on update.

**Impact**: Lambda cold start fails at `client_from_lambda_env` with
`MtlsError::Fetch` or `MtlsError::BundleDecode`. Existing warm Lambda
containers keep working with the cached bundle until they reach end
of life (~15 min idle); new containers fail immediately. The
`<env>-indexer-ch-write-failures` alarm fires within 5 min of the
first cold start.

**Recovery**:

1. List the deleted secret recovery window if `delete-secret` was the
   trigger: `aws secretsmanager describe-secret --secret-id … --query
'DeletionDate'`. AWS retains for 7 days minimum; restore with
   `aws secretsmanager restore-secret`.
2. If the secret was overwritten with bad JSON, re-issue the cert per
   D-4 and re-upload using `update-secret` (not `create-secret`).
3. Force Lambda container rotation. **Use `publish-version`, NOT
   `update-function-configuration --environment`** — the latter
   REPLACES the entire `Variables` map and would silently drop every
   env var the Lambda needs (`MTLS_SECRET_NAME`, `CH_DOMAIN`,
   `STELLAR_NETWORK_PASSPHRASE`, `ENRICHMENT_QUEUE_URL`, `BUCKET_NAME`,
   …), turning the secret-recovery into a self-inflicted Init Errors
   storm. `publish-version` snapshots the current config + code into
   a new immutable version and that publication alone triggers cold-
   start on warm containers.
   ```bash
   aws lambda publish-version \
     --region eu-central-1 \
     --function-name $INDEXER_FN \
     --description "rotate after Secrets Manager recovery $(date -u +%FT%TZ)"
   ```
   If a forced cold start without version churn is genuinely needed,
   use the merge-then-update form (read current env first, append a
   single `CACHE_BUST` key, write back):
   ```bash
   CUR=$(aws lambda get-function-configuration --function-name $INDEXER_FN \
          --region eu-central-1 \
          --query 'Environment.Variables' --output json)
   NEW=$(echo "$CUR" | jq --arg ts "$(date -u +%s)" '. + {CACHE_BUST: $ts}')
   aws lambda update-function-configuration \
     --region eu-central-1 \
     --function-name $INDEXER_FN \
     --environment "Variables=$NEW"
   ```

### DR-3 — S3 backlog growing faster than drain

**Trigger**: Lambda paused for hours (DR-1 or extended fix-forward),
backlog now > 1 000 unprocessed `.xdr.zst` objects.

**Impact**: Galexie keeps producing at ~12 events / min (pubnet
cadence); the ledger lag metric drifts; the explorer's read-side data
freshness degrades.

**Recovery**:

1. Confirm Hetzner CH can absorb parallel writes — sample
   `clickhouse-client -q "SELECT count() FROM system.merges"` should
   stay < 10 even under bulk load.
2. Raise `reservedConcurrentExecutions` temporarily:
   ```bash
   aws lambda put-function-concurrency \
     --region eu-central-1 \
     --function-name production-soroban-explorer-indexer \
     --reserved-concurrent-executions 5    # up from steady-state value
   ```
3. Monitor `Invocations` and the lag metric. Drop concurrency back
   once lag < 60 s.

**Threshold rule of thumb**: raise concurrency 1× per 500 events of
accumulated backlog. Drain at ~30 events / min at concurrency=1
(empirical from staging — confirm with your own measurements).
