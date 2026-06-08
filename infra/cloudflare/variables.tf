# Non-secret inputs. Real values go in terraform.tfvars (gitignored); see
# terraform.tfvars.example. No secret VALUES here — the API token is read from
# CLOUDFLARE_API_TOKEN (providers.tf) and the mTLS cert/key from ./certs/
# (gitignored, origin-lock.tf).

# ── Cross-repo reference: the zone owned by rf-domains ─────────────────
#
# The zone `rumblefishdev.com` is created and owned by the rf-domains repo
# (model A, D9/D10). We reference it by id rather than create it. Copy the
# value from rf-domains' `zone_id` output (or the Cloudflare dashboard) into
# terraform.tfvars. The zone id is an identifier, not a secret.
#
# Ordering: rf-domains must apply (zone exists) BEFORE this module's first
# apply. (Alternative: a `data "cloudflare_zone"` lookup by name — avoided here
# to keep a single, explicit cross-repo contract and a narrower token scope.)
variable "cloudflare_zone_id" {
  description = "Cloudflare zone id of rumblefishdev.com (from rf-domains' zone_id output)."
  type        = string

  # Cheap guardrail: a Cloudflare zone id is 32 lowercase hex chars. Catches an
  # empty value, the REPLACE_WITH_… placeholder, or a typo before it silently
  # writes the API record/AOP into the wrong (or nonexistent) zone. It does NOT
  # prove the id is the RIGHT zone — copy it from rf-domains' zone_id output.
  validation {
    condition     = can(regex("^[0-9a-f]{32}$", var.cloudflare_zone_id))
    error_message = "cloudflare_zone_id must be a 32-char hex Cloudflare zone id (copy from rf-domains' zone_id output)."
  }
}

variable "api_hostname" {
  description = "API hostname, e.g. api.sorobanscan.rumblefishdev.com."
  type        = string
}

# ── Origin target (where Cloudflare forwards) ──────────────────────────
# The API Gateway REGIONAL custom-domain target — read from the CDK
# ApiGateway stack output (the regional domain name of the custom domain),
# e.g. dYYYY.execute-api.eu-central-1.amazonaws.com.
variable "api_origin_target" {
  description = "API Gateway REGIONAL custom-domain target (CDK output)."
  type        = string
}

# ── Rollout gating ─────────────────────────────────────────────────────

variable "create_dns_record" {
  description = <<-EOT
    Gate the proxied API DNS cutover. Keep FALSE until the AWS-side origin
    lockdown (API GW mTLS + disableExecuteApiEndpoint) is verified. Flip to TRUE
    only at the actual cutover (task 0277 Step 4).
  EOT
  type        = bool
  default     = false
}

variable "enable_api_mtls_aop" {
  description = <<-EOT
    Enable the API mTLS leg: per-host Authenticated Origin Pulls with our own
    client cert. Keep FALSE until ./certs/cf-client.{pem,key} exist — Terraform
    evaluates file() eagerly at plan time (even under -target), so an
    unconditional reference would block apply. Flip to TRUE once the certs are
    generated (task 0277 Step 2). Exact AOP scope (per-host vs zone-level on
    Free) is confirmed in the staging dry-run (Step 3).
  EOT
  type        = bool
  default     = false
}
