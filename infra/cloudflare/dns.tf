# The sorobanscan API DNS record — the ONE record this repo owns in the
# rumblefishdev.com zone (D9). A single cloudflare_dns_record is its own
# resource, so it never conflicts with the company records owned by rf-domains.
#
# Gated by var.create_dns_record so the AOP origin lock can be provisioned
# WITHOUT cutting over traffic. Flip create_dns_record=true only at the actual
# cutover (task 0277 Step 4).
#
# proxied=true (orange) routes the API through the Cloudflare edge — this is
# what puts the WAF/DDoS/rate-limit/challenge rules (owned by rf-domains,
# http.host-scoped to this hostname) in the request path. proxied=true requires
# ttl=1 (automatic). v5: resource is cloudflare_dns_record, value attr `content`.
resource "cloudflare_dns_record" "api" {
  count = var.create_dns_record ? 1 : 0

  zone_id = var.cloudflare_zone_id
  name    = var.api_hostname
  type    = "CNAME"
  content = var.api_origin_target
  proxied = true
  ttl     = 1
}
