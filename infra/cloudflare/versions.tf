# Terraform + provider pins and remote-state backend for the sorobanscan
# slice of the Cloudflare edge (task 0277 / ADR 0048).
#
# SCOPE (task 0277 D9): this module manages ONLY sorobanscan-specific
# Cloudflare resources — the `api.sorobanscan.rumblefishdev.com` DNS record
# and the Cloudflare side of the API origin lock (per-host AOP). The zone
# `rumblefishdev.com`, the company DNS records and the zone-level edge
# rulesets are owned by the private `rf-domains` repo (model A, D10). This
# module REFERENCES the zone by id (var.cloudflare_zone_id) — it never owns it.
#
# Cloudflare provider pinned to v5.x (the v5 rewrite renamed resources and
# switched nested blocks to nested attributes assigned with `=`). Verified
# against v5.19.1. Keep this in lockstep with rf-domains' provider version so
# any resource ever moved between the two states (D10 reversibility) plans
# clean.
#
# State lives in S3 with the NATIVE S3 lockfile (use_lockfile, Terraform
# >= 1.10 — no DynamoDB). Backend bucket is the CDK-provisioned
# `<env>-soroban-explorer-cf-tfstate` (CloudflareBootstrapStack) — SEPARATE
# from rf-domains' bucket (D11). State may carry the mTLS client key as a
# resource attribute, so the bucket stays private + encrypted, never committed.

terraform {
  required_version = ">= 1.10.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.19"
    }
  }

  # Partial backend config — real values supplied at init time via
  #   terraform init -backend-config=backend.hcl
  # (backend.hcl is gitignored; see backend.hcl.example).
  backend "s3" {}
}
