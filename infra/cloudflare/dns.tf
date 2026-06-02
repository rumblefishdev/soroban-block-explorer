# DNS records. Gated by var.create_dns_records so the rest of the config
# (zone settings, AOP lock, rulesets) can be applied WITHOUT cutting over
# traffic. Flip create_dns_records=true only at the actual cutover.
#
# Proxy (orange-cloud) the SPA + API so Cloudflare's edge (WAF, DDoS,
# challenge, rate limit) and the origin locks apply. Keep ch.* DNS-only
# (grey) — Cloudflare's proxy would terminate TLS and break Caddy's
# client-cert mTLS, and the box needs a public IP + DNS for ACME HTTP-01.
#
# proxied=true requires ttl=1 (automatic). v5: resource is
# cloudflare_dns_record and the value attribute is `content`.

# SPA — apex CNAME to CloudFront (Cloudflare flattens apex CNAMEs), proxied.
resource "cloudflare_dns_record" "spa" {
  count = var.create_dns_records ? 1 : 0

  zone_id = cloudflare_zone.this.id
  name    = var.spa_hostname
  type    = "CNAME"
  content = var.spa_origin_target
  proxied = true
  ttl     = 1
}

# API — CNAME to the API Gateway regional custom-domain target, proxied.
resource "cloudflare_dns_record" "api" {
  count = var.create_dns_records ? 1 : 0

  zone_id = cloudflare_zone.this.id
  name    = var.api_hostname
  type    = "CNAME"
  content = var.api_origin_target
  proxied = true
  ttl     = 1
}

# ClickHouse — A record to the Hetzner box, DNS-only (grey). NOT proxied.
resource "cloudflare_dns_record" "ch" {
  count = var.create_dns_records ? 1 : 0

  zone_id = cloudflare_zone.this.id
  name    = var.ch_hostname
  type    = "A"
  content = var.ch_origin_ip
  proxied = false
  ttl     = 300
}
