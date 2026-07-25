# Milestone 3 — Security Checklist (Sign-off)

Project: Soroban Block Explorer  
Team: Rumble Fish  
Deliverable: Milestone 3 - Mainnet Launch

Every control below is in place and was verified in code and infrastructure, and
confirmed against the running production stack via read-only checks (AWS API +
edge). File references point to the public repository.

## Controls

| #   | Control                                   | How it is satisfied                                                                                                                                                                                                                                                                                                                                                                 | Status |
| --- | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| 1   | Least-privilege IAM — no wildcard actions | Every CDK IAM policy statement scopes `actions` to named operations; no `actions: ['*']`, no `service:*`, and no `AdministratorAccess`/`PowerUserAccess` anywhere in `infra/src`. The only attached AWS-managed policy is read-only (`CloudWatchReadOnlyAccess`).                                                                                                                   | ✅     |
| 2   | AWS WAF on public ingress                 | A regional WAF associated with the API Gateway stage (`CfnWebACLAssociation`, `api-gateway-stack.ts`) and a CloudFront-scoped WAF, each running the AWS managed rule groups `CommonRuleSet`, `KnownBadInputsRuleSet`, and `AmazonIpReputationList`, plus a rate-limit rule (`constructs/waf-web-acl.ts`).                                                                           | ✅     |
| 3   | Request throttling                        | API Gateway stage + usage-plan rate/burst limits — **50 rps / 100 burst** in production (`envs/production.json`) — plus a WAF rate-limit rule. Throttles are lifted only inside a load-test window gated by `LOAD_TESTING`, which is **off** in production (verified).                                                                                                              | ✅     |
| 4   | No public datastore endpoint              | Defence in depth: in production ClickHouse's ports are published to **loopback only** (`127.0.0.1:8123` / `:9000`, `docker-compose.prod.yml` `ports: !override`); the host firewall admits only ports 22/80/443 (`hetzner_firewall_rules`); and Caddy fronts it on 443 with **mTLS client-certificate** authentication (client-CN → ClickHouse-user mapping, `allowed_client_cns`). | ✅     |
| 5   | Secrets in AWS Secrets Manager            | mTLS client material, the Cloudflare API token, and the CloudFront origin secret are stored in Secrets Manager (`lib/mtls.ts`, `stacks/cloudflare-bootstrap-stack.ts`); the ClickHouse password is injected from the environment, never hard-coded.                                                                                                                                 | ✅     |
| 6   | TLS end-to-end                            | HTTPS at the Cloudflare / CloudFront edge → API Gateway → Lambda (AWS-internal, encrypted); Caddy enforces **TLS 1.3** and mTLS on the ClickHouse leg (`Caddyfile`).                                                                                                                                                                                                                | ✅     |
| 7   | Server-side input validation              | Every endpoint validates inputs through typed extractors and rejects malformed input with `400` (invalid IDs, pagination, malformed/expired cursors — `crates/api/src/**/handlers.rs`). A read-only ClickHouse RBAC profile additionally rejects per-query setting overrides.                                                                                                       | ✅     |
| 8   | Encryption at rest                        | The public ledger bucket uses SSE-S3 / AES256 (`stacks/ledger-bucket-stack.ts`, deliberately not SSE-KMS — the contents are public on-chain XDR). ClickHouse holds only public, fully re-derivable chain data.                                                                                                                                                                      | ✅     |
| 9   | Backups & recovery                        | Automated **weekly** off-box Borg backups of ClickHouse to a Hetzner Storage Box, retain 4 (task 0236).                                                                                                                                                                                                                                                                             | ✅     |

## OWASP Top 10 (2021) — coverage

| Category                               | How it is addressed                                                                                                                                                                                                                                                               |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A01 Broken Access Control              | Public UI gated by a Cloudflare Turnstile (managed) challenge → server-verified session JWT (`/auth/session`, `crates/api/src/auth`); direct/reviewer API access via `x-api-key`; Cloudflare origin lock (ADR 0048); least-privilege IAM; datastore behind mTLS + read-only RBAC. |
| A02 Cryptographic Failures             | TLS end-to-end; SSE-S3 at rest; secrets in Secrets Manager.                                                                                                                                                                                                                       |
| A03 Injection                          | Typed request extractors + server-side validation; read-only ClickHouse RBAC profile that rejects setting overrides.                                                                                                                                                              |
| A04 Insecure Design                    | Edge origin-lock + mTLS trust boundary; capacity validated by an open-model load test.                                                                                                                                                                                            |
| A05 Security Misconfiguration          | WAF managed rule groups; no public datastore endpoint; firewalled host; no wildcard IAM.                                                                                                                                                                                          |
| A06 Vulnerable & Outdated Components   | Dependencies pinned via `Cargo.lock`; small, single-purpose Rust services.                                                                                                                                                                                                        |
| A07 Identification & Auth Failures     | Turnstile → session JWT for the UI and `x-api-key` for direct API access, both behind the Cloudflare origin lock; mTLS client-certificate auth for the datastore.                                                                                                                 |
| A08 Software & Data Integrity Failures | Infrastructure-as-code (CDK + Ansible); CI/CD via GitHub OIDC (`stacks/cicd-stack.ts`).                                                                                                                                                                                           |
| A09 Logging & Monitoring Failures      | CloudWatch dashboard + Slack-wired alarms + X-Ray tracing.                                                                                                                                                                                                                        |
| A10 SSRF                               | Read-time external fetches resolve on-chain-declared metadata URIs through a fixed gateway / archive, not arbitrary request-supplied URLs.                                                                                                                                        |

## Note on the original RDS-specific wording

The approved checklist named KMS-at-rest and point-in-time recovery — both
specific to the PostgreSQL-on-RDS datastore that was retired (task 0239) in
favour of ClickHouse on Hetzner. They are satisfied here by
architecture-appropriate equivalents: SSE-S3 (AES256) on the public ledger
bucket, and automated weekly off-box backups of the ClickHouse store, which
holds only public, fully re-derivable chain data. No RDS instance exists, so
"RDS has no public endpoint" holds trivially.

## Sign-off

Every control above was verified in code and against the running production stack
via read-only checks, and is signed off on that basis.

Signed off: Rumble Fish  
Date: 2026-07-25
