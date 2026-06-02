# Zone settings. v5 uses ONE cloudflare_zone_setting resource per setting
# (the v4 cloudflare_zone_settings_override single-block form is gone).
#
# SSL/TLS = Full (strict) is MANDATORY (ADR 0048) — never Flexible. Full
# (strict) makes Cloudflare validate the origin cert, so the edge↔origin
# leg is authenticated, not just encrypted.

resource "cloudflare_zone_setting" "ssl" {
  zone_id    = cloudflare_zone.this.id
  setting_id = "ssl"
  value      = "strict" # = Full (strict)
}

resource "cloudflare_zone_setting" "always_use_https" {
  zone_id    = cloudflare_zone.this.id
  setting_id = "always_use_https"
  value      = "on"
}

resource "cloudflare_zone_setting" "min_tls_version" {
  zone_id    = cloudflare_zone.this.id
  setting_id = "min_tls_version"
  value      = "1.2"
}
