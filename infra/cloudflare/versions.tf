# Terraform + provider pins and remote state backend for the Cloudflare
# edge (task 0277 / ADR 0048).
#
# Cloudflare provider is pinned to v5.x — the v5 rewrite renamed many
# resources and switched nested blocks to nested attributes (assigned with
# `=`). Do NOT downgrade to v4 syntax. Verified against v5.19.1.
#
# State lives in S3 with the NATIVE S3 lockfile (use_lockfile, Terraform
# >= 1.10 — no DynamoDB table needed). The state bucket is versioned +
# encrypted and holds secrets (origin secret, mTLS key as resource
# attributes) — it MUST stay private and is never committed. See README.md
# for the one-time bucket bootstrap.

terraform {
  required_version = ">= 1.10.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.19"
    }
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  # Partial backend config — real values are supplied at init time via
  #   terraform init -backend-config=backend.hcl
  # (backend.hcl is gitignored; see backend.hcl.example). Keeping the
  # bucket/key out of source avoids hardcoding env-specific names.
  backend "s3" {}
}
