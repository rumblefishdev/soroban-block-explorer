# Non-secret inputs. Real values go in terraform.tfvars (gitignored);
# see terraform.tfvars.example. No secret VALUES here — secrets are read
# from AWS Secrets Manager (see secrets.tf) or the CLOUDFLARE_API_TOKEN env
# var (see providers.tf).

variable "aws_region" {
  description = "AWS region for the Secrets Manager + state backend (matches CDK)."
  type        = string
  default     = "eu-central-1"
}

variable "cloudflare_account_id" {
  description = "Cloudflare account ID that owns the zone."
  type        = string
}

variable "zone_name" {
  description = "The delegated zone apex, e.g. sorobanscan.rumblefish.dev."
  type        = string
}

# ── Public hostnames (what browsers/partners hit) ──────────────────────

variable "spa_hostname" {
  description = "SPA hostname (zone apex), e.g. sorobanscan.rumblefish.dev."
  type        = string
}

variable "api_hostname" {
  description = "API hostname, e.g. api.sorobanscan.rumblefish.dev."
  type        = string
}

variable "ch_hostname" {
  description = "ClickHouse hostname, e.g. ch.sorobanscan.rumblefish.dev. Stays DNS-only (grey)."
  type        = string
}

# ── Origin targets (where Cloudflare forwards) ─────────────────────────
# These are AWS-side outputs the operator reads from CDK (CloudFront
# distribution domain, API Gateway regional domain) and SSM (Hetzner IP).

variable "spa_origin_target" {
  description = "CloudFront distribution domain for the SPA, e.g. dXXXX.cloudfront.net (CDK DistributionDomainName output)."
  type        = string
}

variable "api_origin_target" {
  description = "API Gateway REGIONAL custom-domain target, e.g. dYYYY.execute-api.eu-central-1.amazonaws.com (the regional domain name of the custom domain)."
  type        = string
}

variable "ch_origin_ip" {
  description = "Hetzner ClickHouse public IPv4 (same value as SSM /soroban/production/ch-ip). DNS-only record."
  type        = string
}

# ── Secret references (NAMES, not values) ──────────────────────────────

variable "origin_secret_secret_id" {
  description = "AWS Secrets Manager secret id holding the X-Origin-Secret value (canonical source; the CloudFront KVS is a copy)."
  type        = string
  default     = "soroban/production/cloudflare/origin-secret"
}

# ── Edge security tunables ─────────────────────────────────────────────

variable "free_managed_ruleset_id" {
  description = "Cloudflare Free Managed Ruleset ID (Cloudflare-side constant; confirm against your account)."
  type        = string
  default     = "77454a7344524356b1b6e1a2604bb3a4"
}

variable "api_managed_challenge_expression" {
  description = <<-EOT
    Cloudflare ruleset expression selecting which API requests get a Managed
    Challenge. EMPTY (default) = the rule is NOT created. Cloudflare retired
    cf.threat_score (always 0) and Free has no attack/bot score, so there is no
    built-in "suspicious" signal — rely on Bot Fight Mode (dashboard/zone) for
    bots and set a concrete expression here only when you have one verified not
    to challenge the SPA's fetch()/XHR or the x-api-key partner path.
  EOT
  type        = string
  default     = ""
}

variable "api_rate_limit_requests" {
  description = "Requests per period for the single Free-plan rate-limit rule on the API."
  type        = number
  default     = 100
}

variable "api_rate_limit_period" {
  description = "Rate-limit window in seconds. Free plan is effectively a fixed 10s window."
  type        = number
  default     = 10
}

variable "api_rate_limit_mitigation_timeout" {
  description = "Seconds a rate-limit block stays in effect after breach."
  type        = number
  default     = 60
}

# ── Rollout gating ─────────────────────────────────────────────────────

variable "create_dns_records" {
  description = <<-EOT
    Gate the proxied DNS cutover. Keep FALSE for the first applies so the
    zone settings, AOP cert/lock and rulesets are provisioned WITHOUT moving
    traffic. Flip to TRUE only at the actual cutover (task 0277 Step 4),
    after the AWS-side origin lockdown is verified.
  EOT
  type        = bool
  default     = false
}
