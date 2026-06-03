# Origin lockdown — Cloudflare side (ADR 0048 Step 2).
#
# Two independent legs:
#   API  → zone-level Authenticated Origin Pulls (mTLS) with OUR OWN client
#          cert (unique, not the global shared cert). AWS API Gateway mTLS
#          (CDK side) verifies this cert at the TLS handshake.
#   SPA  → a Transform Rule injects the X-Origin-Secret header; a
#          viewer-request CloudFront Function (CDK side) rejects requests
#          without it (CloudFront can't do viewer mTLS).

# ── API leg: zone-level Authenticated Origin Pulls (own cert) ──────────
#
# GATED by var.enable_api_mtls_aop (default false). The client cert + key are
# placed by the operator under ./certs/ (gitignored) — they do NOT exist when
# the zone is first stood up. Terraform evaluates file() EAGERLY at plan time
# for every resource (even under `-target`), so referencing the certs
# unconditionally would block the initial zone/settings/WAF apply. count=0
# leaves the file() unevaluated; flip the flag once the certs are generated
# (task 0277 Step 2 / KROK 5).
#
# The PRIVATE KEY is sensitive and ends up in state (resource attribute) —
# this is why state is private/encrypted. For production prefer sourcing the
# key from Secrets Manager rather than a file on disk.

resource "cloudflare_authenticated_origin_pulls_certificate" "client" {
  count = var.enable_api_mtls_aop ? 1 : 0

  zone_id     = cloudflare_zone.this.id
  certificate = file("${path.module}/certs/cf-client.pem")
  private_key = file("${path.module}/certs/cf-client.key")
}

resource "cloudflare_authenticated_origin_pulls" "zone" {
  count = var.enable_api_mtls_aop ? 1 : 0

  zone_id = cloudflare_zone.this.id

  # Zone-level (no `hostname` → applies to the whole zone). cert id comes
  # from the uploaded cert's read-only `certificate_id`.
  #
  # VERIFY WITH `terraform plan` (flagged in research): v5 folds the
  # zone-level and per-hostname forms into one `config` list; the exact
  # shape for the pure zone-level toggle is believed to be
  # [{ cert_id, enabled }] with `hostname` omitted, but the doc example
  # only shows the per-hostname form. Confirm before relying on it.
  config = [{
    cert_id = cloudflare_authenticated_origin_pulls_certificate.client[0].certificate_id
    enabled = true
  }]
}

# ── SPA leg: inject X-Origin-Secret toward the origin ──────────────────
#
# Phase http_request_late_transform = set origin-bound request headers
# after other processing. The value is the secret from Secrets Manager
# (secrets.tf); the matching CloudFront Function checks it.

resource "cloudflare_ruleset" "origin_secret_header" {
  zone_id = cloudflare_zone.this.id
  name    = "Inject origin secret"
  kind    = "zone"
  phase   = "http_request_late_transform"

  rules = [{
    description = "Add X-Origin-Secret on requests to the SPA origin"
    # Scope to the proxied SPA host so the header is only sent where the
    # CloudFront Function expects it.
    expression = "(http.host eq \"${var.spa_hostname}\")"
    action     = "rewrite"
    action_parameters = {
      headers = {
        "X-Origin-Secret" = {
          operation = "set"
          value     = local.origin_secret
        }
      }
    }
  }]
}
