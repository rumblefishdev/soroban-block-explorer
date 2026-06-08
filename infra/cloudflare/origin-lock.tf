# Origin lockdown — Cloudflare side, API only (task 0277 D12, ADR 0048 Step 2).
#
# mTLS via Authenticated Origin Pulls with OUR OWN client cert (unique — not
# the global shared cert). AWS API Gateway mTLS (CDK side: api-gateway-stack.ts
# truststore + disableExecuteApiEndpoint) verifies this cert at the TLS
# handshake, so the origin accepts ONLY Cloudflare.
#
# Why mTLS and not a secret header under the repo split (D12): a secret-header
# lock would put the injecting Transform Rule into a zone-level ruleset (owned
# by rf-domains, D10) AND require the secret to be shared across two repos/states.
# mTLS keeps the whole lock self-contained in this repo: the client cert here +
# the API GW truststore in CDK.
#
# GATED by var.enable_api_mtls_aop (default false). The cert + key are placed by
# the operator under ./certs/ (gitignored) — they do NOT exist when the record
# is first stood up. Terraform evaluates file() EAGERLY at plan time for every
# resource (even under -target), so an unconditional reference would block the
# initial apply. count=0 leaves file() unevaluated; the try(file(), "") idiom
# also satisfies schema validation (count=0 still validates Required attrs,
# where null fails but "" passes). Flip the flag once the certs are generated.
#
# The PRIVATE KEY is sensitive and ends up in state (resource attribute) — this
# is why the backend bucket is private/encrypted. For production prefer sourcing
# the key from Secrets Manager rather than a file on disk.

resource "cloudflare_authenticated_origin_pulls_certificate" "client" {
  count = var.enable_api_mtls_aop ? 1 : 0

  zone_id     = var.cloudflare_zone_id
  certificate = try(file("${path.module}/certs/cf-client.pem"), "")
  private_key = try(file("${path.module}/certs/cf-client.key"), "")
}

# Per-host AOP scoped to the API hostname (not zone-level) so this lock only
# affects our origin and never touches other (future) proxied hosts in the
# shared zone. VERIFY in the staging dry-run (Step 3): per-host AOP availability
# on Free and the exact v5 `config` shape (hostname + cert_id + enabled). If
# per-host proves unavailable on Free, zone-level AOP moves to rf-domains (the
# zone owner) and this module just supplies the cert.
resource "cloudflare_authenticated_origin_pulls" "api" {
  count = var.enable_api_mtls_aop ? 1 : 0

  zone_id = var.cloudflare_zone_id

  config = [{
    hostname = var.api_hostname
    cert_id  = cloudflare_authenticated_origin_pulls_certificate.client[0].certificate_id
    enabled  = true
  }]
}
