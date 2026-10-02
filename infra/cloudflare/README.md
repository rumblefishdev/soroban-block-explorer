# `infra/cloudflare/` — sorobanscan slice of the Cloudflare edge

Terraform for the **sorobanscan-specific** Cloudflare resources only
(task 0277 / ADR 0048). Part of a deliberate **repo split** (D9):

| Lives here (`soroban-explorer`)                         | Lives in `rf-domains` (private)                             |
| ------------------------------------------------------- | ----------------------------------------------------------- |
| `api-sorobanscan.rumblefishdev.com` DNS record (orange) | the `cloudflare_zone` `rumblefishdev.com`                   |
| API origin lock — per-host **AOP** (mTLS)               | company DNS records + zone settings                         |
| AWS side of the lock (CDK: API GW mTLS)                 | zone-level **edge rulesets** (WAF / rate-limit / challenge) |
| own TF-state bucket (`*-cf-tfstate`)                    | its own, separate TF-state bucket                           |

This module **does not own the zone** — it references it by id
(`var.cloudflare_zone_id`, from rf-domains' `zone_id` output).

## Ruleset ownership — model A (D10)

The zone's WAF / rate-limit / Managed Challenge / Transform rulesets are
**per-(zone, phase) singletons**, so only one Terraform state may own each
phase. They are owned by **rf-domains** (the zone owner); each rule is
`http.host`-scoped to `api-sorobanscan.rumblefishdev.com`. Rulesets only act on
**proxied** traffic and only the API record is orange, so this is conflict-free.

Reversible to single-tenant **model C** (rulesets pulled into this repo) via
`terraform state rm` (rf-domains) + `terraform import` (here) — no destroy /
recreate, no downtime. Keep the provider version in lockstep so the move plans
clean.

## Prerequisites (ordering)

1. `rf-domains` applied first → the zone `rumblefishdev.com` exists; copy its
   `zone_id` output into `terraform.tfvars`.
2. CDK `CloudflareBootstrapStack` deployed → the state bucket exists
   (`make -C infra deploy-production-cloudflare-bootstrap`).
3. Zone-scoped, least-privilege `CLOUDFLARE_API_TOKEN` exported (DNS:Edit +
   SSL and Certificates:Edit on rumblefishdev.com; see `providers.tf`).

## Usage

```bash
cp backend.hcl.example backend.hcl            # fill bucket/key
cp terraform.tfvars.example terraform.tfvars  # fill zone id + origin target
export CLOUDFLARE_API_TOKEN=$(aws secretsmanager get-secret-value \
  --secret-id soroban/production/cloudflare/api-token \
  --query SecretString --output text)

terraform init -backend-config=backend.hcl
terraform plan      # gates default false → provisions nothing destructive
```

### Rollout gates

- `create_dns_record` — flip `true` only at the actual cutover (Step 4), after
  the AWS-side lock is verified.
- `enable_api_mtls_aop` — flip `true` once `./certs/cf-client.{pem,key}` exist
  (operator `openssl`); confirm per-host vs zone-level AOP on Free in the
  staging dry-run (Step 3).

## Testnet

The testnet API host (task 0553) is the same module with its own state, in
the Terraform workspace `testnet` — same backend file, same bucket; S3 keeps
the workspace's state under `env:/testnet/`. Production stays in `default`.

```bash
cp terraform.testnet.tfvars.example testnet.tfvars   # fill zone id + origin target
terraform workspace select -or-create testnet
terraform plan -var-file=testnet.tfvars
terraform workspace select default                   # back to production
```

`workspace-guard.tf` stops a plan whose host does not belong to the selected
workspace (a testnet host outside `testnet`, or production's inside it), so a
forgotten `workspace select` cannot touch production's API record. Its
`terraform_data` resource shows up once as "1 to add" in each workspace; it
creates nothing in Cloudflare.

The origin target is the `CloudflareApiRegionalTarget` output of
`Explorer-testnet-ApiGateway`, which exists after the testnet deploy with
`enableCloudflareApiDomain: true` (`docs/deployment.md` § Testnet).

## Secrets / safety

- **Never commit** `backend.hcl`, `terraform.tfvars`, `testnet.tfvars`, `*.tfstate`, `certs/`
  (see `.gitignore`). State can carry the mTLS private key → bucket stays
  private + encrypted.
- API token: zone-scoped, least-privilege, from Secrets Manager — never the
  Global API Key, never in `.tf` or committed state.
- SSL/TLS mode (Full strict) is a **zone setting → owned by rf-domains**.
