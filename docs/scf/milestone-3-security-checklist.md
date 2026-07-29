# Milestone 3 — Security Checklist (Sign-off)

Project: Soroban Block Explorer  
Team: Rumble Fish  
Deliverable: Milestone 3 - Mainnet Launch

Every control below is in place and was verified in code and infrastructure, and
confirmed against the running production stack — the AWS API, the public edge,
and the ClickHouse host. File references point to the public repository.

## Controls

| #   | Control                                     | How it is satisfied                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | Status |
| --- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| 1   | Least-privilege IAM — no wildcard actions   | Every CDK IAM policy statement names its actions; no `actions: ['*']`, no `service:*`, and no `AdministratorAccess`/`PowerUserAccess` anywhere in `infra/src`. The only action wildcards are CDK grant-helper read prefixes (`s3:GetObject*`, `s3:List*`) scoped to a single bucket. The AWS-managed policies in play are `AWSLambdaBasicExecutionRole` on the Lambdas — the CDK default, which grants CloudWatch Logs write only — and `CloudWatchReadOnlyAccess` on the Slack notification role. Neither grants broad or administrative access. | ✅     |
| 2   | Managed rule sets on public ingress         | The data API is proxied through the Cloudflare edge, which applies managed WAF, rate-limit and Managed Challenge rule sets scoped to the API hostname, plus unmetered DDoS mitigation (ADR 0048). Those rule sets belong to the DNS zone, managed in a separate private repository; `infra/cloudflare/README.md` documents the split.                                                                                                                                                                                                             | ✅     |
| 3   | Request throttling                          | API Gateway stage + usage-plan rate/burst limits — **50 rps / 100 burst** in production (`envs/production.json`), confirmed live on the `production` stage — with Cloudflare rate limiting in front of it (control 2). Throttles are lifted only inside a load-test window gated by `LOAD_TESTING`, which is **off** in production (verified: the variable is absent from the API Lambda's environment).                                                                                                                                          | ✅     |
| 4   | No public datastore endpoint                | Defence in depth: in production ClickHouse's ports are published to **loopback only** (`127.0.0.1:8123` / `:9000`, `docker-compose.prod.yml` `ports: !override`); the host firewall admits only ports 22/80/443 (`hetzner_firewall_rules`); and Caddy fronts it on 443 with **mTLS client-certificate** authentication (client-CN → ClickHouse-user mapping, `allowed_client_cns`).                                                                                                                                                               | ✅     |
| 5   | Secrets in AWS Secrets Manager              | mTLS client material, the Cloudflare API token, and the CloudFront origin secret are stored in Secrets Manager (`lib/mtls.ts`, `stacks/cloudflare-bootstrap-stack.ts`), together with the Turnstile secret, the JWT signing key and the API keys. The production Lambdas hold no database password: they authenticate to ClickHouse with a **client certificate** fetched at runtime from Secrets Manager (`db-clickhouse/src/mtls.rs`, `client_from_lambda_env`). The `CH_PASSWORD` path in the repository exists only under `#[cfg(test)]`.     | ✅     |
| 6   | TLS end-to-end                              | HTTPS at the Cloudflare / CloudFront edge → API Gateway → Lambda (AWS-internal, encrypted); Caddy enforces **TLS 1.3** and mTLS on the ClickHouse leg (`Caddyfile`).                                                                                                                                                                                                                                                                                                                                                                              | ✅     |
| 7   | Server-side input validation                | Every endpoint validates inputs through typed extractors and rejects malformed input with `400` (invalid IDs, pagination, malformed/expired cursors — `crates/api/src/**/handlers.rs`). A read-only ClickHouse RBAC profile additionally rejects per-query setting overrides.                                                                                                                                                                                                                                                                     | ✅     |
| 8   | Encryption at rest                          | The public ledger bucket uses SSE-S3 / AES256 (`stacks/ledger-bucket-stack.ts`, deliberately not SSE-KMS — the contents are public on-chain XDR). ClickHouse holds only public, fully re-derivable chain data.                                                                                                                                                                                                                                                                                                                                    | ✅     |
| 9   | Backups & recovery                          | Automated **weekly** off-box Borg backups of ClickHouse to a Hetzner Storage Box, retain 4 (task 0236). Each run takes a consistent ClickHouse `FREEZE` snapshot rather than copying live files. Verified on the box 2026-07-27: cron entry, four weekly archives, and a clean run log — see § Evidence below.                                                                                                                                                                                                                                    | ✅     |
| 10  | Browser-session gate on the data API        | The public explorer obtains a Cloudflare **Turnstile** (managed mode) token, exchanges it at `POST /auth/session` for a short-lived **session JWT**, and carries that JWT as `Authorization: Bearer` on every call (`crates/api/src/auth/{turnstile.rs,jwt.rs}`, `web/src/api/session.ts`). Anonymous requests to the data endpoints return `401` (verified live). Non-browser and reviewer access uses a separate `x-api-key`. The reference documentation (Swagger UI, OpenAPI JSON) is deliberately left open.                                 | ✅     |
| 11  | Origin lock — the API answers only the edge | A shared secret is stamped on requests at the Cloudflare edge and required by the API before any route runs; anything without it is rejected with `403` (`crates/api/src/common/edge_lock.rs`, secret in Secrets Manager as `soroban/production/cloudflare/edge-secret`, ADR 0048). Verified live: the raw API Gateway URL answers `403 forbidden`, so reaching the origin directly does not skip the edge.                                                                                                                                       | ✅     |

## OWASP Top 10 (2021) — coverage

| Category                               | How it is addressed                                                                                                                                                                                                                                                                         |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A01 Broken Access Control              | No user accounts, no per-user data, no writes to the datastore. Access is gated by Turnstile → session JWT or `x-api-key`, both behind the edge origin lock; least-privilege IAM; the datastore is unreachable from the internet.                                                           |
| A02 Cryptographic Failures             | TLS on every hop, TLS 1.3 on the ClickHouse leg; SSE-S3 (AES256) at rest; secrets in Secrets Manager.                                                                                                                                                                                       |
| A03 Injection                          | Free text and identifiers are bound parameters; interpolated values are typed integers or character-whitelisted strings, unit-tested against injection payloads. A read-only ClickHouse profile (`readonly=1`, `allow_ddl=0`) rejects setting overrides.                                    |
| A04 Insecure Design                    | Datastore boundary: mTLS with a client-CN → user map that fails closed on an unmapped certificate; ClickHouse binds to loopback behind a 22/80/443 firewall. Capacity validated by an open-model load test.                                                                                 |
| A05 Security Misconfiguration          | No public datastore endpoint; firewalled host; no unbounded IAM. The AWS WAF was replaced by Cloudflare managed rule sets and rate limiting at the edge (ADR 0048).                                                                                                                         |
| A06 Vulnerable & Outdated Components   | Small, single-purpose Rust services; dependency versions pinned via `Cargo.lock`. No advisory-database scanning is wired.                                                                                                                                                                   |
| A07 Identification & Auth Failures     | No user accounts to compromise. Browser sessions use a Turnstile-verified HS256 JWT with the algorithm pinned at verification (`crates/api/src/auth/jwt.rs`); non-browser callers use `x-api-key`; the datastore uses mTLS client certificates.                                             |
| A08 Software & Data Integrity Failures | Infrastructure-as-code (CDK + Ansible); production deploys are run manually from an operator workstation (`docs/deployment.md`).                                                                                                                                                            |
| A09 Logging & Monitoring Failures      | CloudWatch dashboard, eight Slack-wired alarms and X-Ray tracing over ingestion lag, error rate and latency.                                                                                                                                                                                |
| A10 SSRF                               | No request parameter supplies a URL. `ipfs://` resolves through a fixed gateway; an on-chain `https://` URI is fetched as declared, under HTTPS-only, literal-IP rejection, per-hop host re-validation, a size cap and a timeout; the guards are unit-tested in `crates/enrichment-shared`. |

## Evidence — control 9, backups

Captured read-only on the production ClickHouse host on 2026-07-27. The Storage
Box account and hostname are redacted; everything else is verbatim.

The schedule — field five is the weekday, `0` = Sunday:

```text
$ sudo cat /etc/cron.d/ch-backup
#Ansible: ch-backup
30 3 * * 0 root /usr/local/bin/ch-backup >> /var/log/ch-backup.log 2>&1
```

The archives actually present in the repository — four, every Sunday, seven days
apart, which is both the weekly cadence and the retain-4 policy:

```text
$ sudo borg list "$BORG_REPO"
ch-20260705T035828Z    Sun, 2026-07-05 03:58:33
ch-20260712T035324Z    Sun, 2026-07-12 03:53:30
ch-20260719T035534Z    Sun, 2026-07-19 03:55:40
ch-20260726T033815Z    Sun, 2026-07-26 03:38:22
```

The most recent run, start to finish:

```text
$ sudo tail -40 /var/log/ch-backup.log
2026-07-26T03:38:09Z [ch-backup] Starting
2026-07-26T03:38:16Z [ch-backup] Freezing default tables (marker 'ledgers' first) as ch20260726T033815Z
2026-07-26T03:38:21Z [ch-backup] Pushing frozen snapshot ch-20260726T033815Z to ssh://<redacted>.your-storagebox.de:23/./backups/clickhouse
Archive name: ch-20260726T033815Z
Time (start): Sun, 2026-07-26 03:38:22
Time (end):   Sun, 2026-07-26 04:31:48
Duration: 53 minutes 26.46 seconds
Number of files: 15059
                       Original size      Compressed size    Deduplicated size
This archive:                1.00 TB            888.75 GB             15.47 GB
All archives:                4.41 TB              3.91 TB              1.18 TB
2026-07-26T04:31:56Z [ch-backup] Unfreezing ch20260726T033815Z
2026-07-26T04:32:02Z [ch-backup] Pruning old snapshots (keep 0d 4w 0m)
2026-07-26T04:34:57Z [ch-backup] Compacting repository
2026-07-26T04:37:36Z [ch-backup] Done
```

**On recovery.** The restore procedure is documented and drill-tested locally,
end to end (`docs/backups.md`, `infra-hetzner/README.md § Disaster recovery`).

## Note on the original RDS-specific wording

The approved checklist named KMS-at-rest and point-in-time recovery — both
specific to the PostgreSQL-on-RDS datastore that was retired (task 0239) in
favour of ClickHouse on Hetzner. They are satisfied here by
architecture-appropriate equivalents: SSE-S3 (AES256) on the public ledger
bucket, and automated weekly off-box backups of the ClickHouse store, which
holds only public, fully re-derivable chain data. No RDS instance exists, so
"RDS has no public endpoint" holds trivially.

## Sign-off

Every control above was verified in code and against the running production
stack, and is signed off on that basis.

Signed off: Rumble Fish  
Date: 2026-07-29
