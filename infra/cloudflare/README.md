# Cloudflare edge — Terraform

Cloudflare edge (WAF / DDoS / Managed Challenge / rate limit) + origin
lockdown for the Soroban Block Explorer. Implements the Cloudflare side of
**[ADR 0048](../../lore/2-adrs/0048_cloudflare-edge-over-aws-waf.md)** /
**task 0277**. The AWS side (API Gateway mTLS, the `X-Origin-Secret`
CloudFront Function) lives in the CDK app under `infra/`.

> Run **locally by an operator** to start (migrate to CI later). Provider
> pinned to **Cloudflare v5.x** (verified against v5.19.1).

## What this manages

| File               | Resources                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------------ |
| `zone.tf`          | The Cloudflare zone (exposes the NS to delegate).                                          |
| `dns.tf`           | SPA + API records (proxied), `ch` (DNS-only). Gated by `create_dns_records`.               |
| `zone-settings.tf` | SSL = **Full (strict)**, Always Use HTTPS, min TLS 1.2.                                    |
| `origin-lock.tf`   | Zone-level Authenticated Origin Pulls (API mTLS) + `X-Origin-Secret` Transform Rule (SPA). |
| `security.tf`      | Free Managed WAF ruleset, scoped Managed Challenge, single rate-limit rule on the API.     |

## Secrets — never in git or .tf

- **Cloudflare API token** → `CLOUDFLARE_API_TOKEN` env var, sourced from
  Secrets Manager at apply time. Never enters state. Must be **zone-scoped,
  least-privilege** (never the Global API Key).
- **Origin secret** (`X-Origin-Secret`) → read from Secrets Manager
  (`soroban/production/cloudflare/origin-secret`) via a data source. It is a
  required Transform-Rule attribute, so it lands in **state** — which is why
  state is private + encrypted + versioned.
- **mTLS client key** → `./certs/cf-client.key` (gitignored). Also lands in
  state; prefer Secrets Manager for production.

```sh
export CLOUDFLARE_API_TOKEN=$(aws secretsmanager get-secret-value \
  --secret-id soroban/production/cloudflare/api-token \
  --query SecretString --output text)
```

## One-time backend bootstrap

State uses S3 with the **native S3 lockfile** (`use_lockfile`, no DynamoDB;
Terraform ≥ 1.10). The state bucket and the `X-Origin-Secret` are provisioned
**by CDK** (not by hand) — set `provisionCloudflareBootstrap: true` in
`infra/envs/production.json` and deploy the `Explorer-<env>-CloudflareBootstrap`
stack. Then read the bucket name from its output:

```sh
aws cloudformation describe-stacks \
  --stack-name Explorer-production-CloudflareBootstrap \
  --query "Stacks[0].Outputs[?OutputKey=='CloudflareTfStateBucketName'].OutputValue" \
  --output text --region eu-central-1 --profile soroban-explorer

cp backend.hcl.example backend.hcl             # bucket = the name above
cp terraform.tfvars.example terraform.tfvars   # fill real values
terraform init -backend-config=backend.hcl
```

Only the **Cloudflare API token** (external) and the **mTLS client cert/key**
(`openssl`) are created out-of-band; everything else is CDK or Terraform.

> **Do NOT pre-create the `…/cloudflare/origin-secret`** by hand — CDK owns it
> (auto-generated). If a same-named secret already exists (manual create, or a
> RETAIN'd secret still inside its 7–30 day deletion window), the stack deploy
> fails `ResourceExistsException`; delete/restore or `cdk import` it first.

## Rollout (lock before cutover)

`create_dns_records=false` by default so you can stand up everything
**without moving traffic**:

```sh
# 1. Zone settings + AOP cert/lock + rulesets (no traffic moved):
terraform apply

# 2. Verify the AWS-side lockdown (CDK: API mTLS + CloudFront secret fn) and
#    the negative-test matrix (task 0277 Step 7).

# 3. Cut over DNS only when ready:
terraform apply -var=create_dns_records=true
```

`terraform output cloudflare_name_servers` prints the NS values to hand to
the owner of the parent `rumblefish.dev` zone for the delegation
(task 0277 Step 1 sign-off).

## ⚠️ Verify with `terraform plan` (v5 schema caveats)

Confirm against a real `plan`/`apply`:

1. **Zone-level AOP `config` shape** (`origin-lock.tf`): v5 folds zone-level
   and per-hostname into one `config` list. We use `[{ cert_id, enabled }]`
   with `hostname` omitted (verified against the v5 docs as the zone-level
   form); confirm against your zone.
2. **Free Managed Ruleset ID** (`security.tf`): `77…` is a Cloudflare
   account-side constant — confirm it matches your account.

(Rate-limit `characteristics` ships `["ip.src", "cf.colo.id"]` — `cf.colo.id`
is required by the API for count-by-IP rules, verified; no longer a caveat.)

## Not here (by design)

- DNS authority flip = **NS records in the parent `rumblefish.dev` zone**
  (owned outside this repo) — a human handoff, not Terraform.
- AWS-side origin lockdown (API GW mTLS truststore, CloudFront
  `X-Origin-Secret` Function) — CDK app in `infra/`.
- `ch.sorobanscan` stays DNS-only (mTLS + ACME) — see ADR 0048 accepted risk.
