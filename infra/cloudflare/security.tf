# Edge security — WAF managed ruleset, a scoped Managed Challenge, and the
# single Free-plan rate-limit rule on the API (ADR 0048 Step 5).
#
# Free-plan constraints designed around: one rate-limit rule, the Free
# Managed Ruleset only, count-by-IP. Pro upgrade later is a plan flip
# (full Cloudflare Managed + OWASP, a 2nd rate-limit rule) — no re-migration.

# ── Managed WAF (Free Managed Ruleset) ─────────────────────────────────
resource "cloudflare_ruleset" "managed_waf" {
  zone_id = cloudflare_zone.this.id
  name    = "Managed WAF"
  kind    = "zone"
  phase   = "http_request_firewall_managed"

  rules = [{
    description = "Execute Cloudflare Free Managed Ruleset"
    expression  = "true"
    action      = "execute"
    action_parameters = {
      # Cloudflare-side constant — confirm against the account (flagged in
      # research). Override-able once on Pro to the full Managed + OWASP IDs.
      id = var.free_managed_ruleset_id
    }
  }]
}

# ── Custom firewall: Managed Challenge for suspicious API traffic ──
#
# IMPORTANT: Cloudflare RETIRED the Threat Score (cf.threat_score is always 0
# since 2024-09-30), and Free has no attack/bot score, so there is NO built-in
# "suspicious" signal to scope a challenge on. The Free anti-bot lever is
# **Bot Fight Mode** (enabled in the dashboard / zone settings), not a score
# expression. This custom rule is therefore OPT-IN: it is created only when
# `api_managed_challenge_expression` is set to a concrete expression you have
# verified will NOT challenge the SPA's fetch()/XHR or the x-api-key partner
# path (task 0277 Step 5). Empty (default) → no rule, no dead/no-op match.
resource "cloudflare_ruleset" "custom_firewall" {
  count = var.api_managed_challenge_expression != "" ? 1 : 0

  zone_id = cloudflare_zone.this.id
  name    = "Custom firewall"
  kind    = "zone"
  phase   = "http_request_firewall_custom"

  rules = [{
    description = "Managed Challenge for suspicious API traffic"
    expression  = var.api_managed_challenge_expression
    action      = "managed_challenge"
  }]
}

# ── Rate limiting (single Free-plan rule, on the API) ──────────────────
resource "cloudflare_ruleset" "api_ratelimit" {
  zone_id = cloudflare_zone.this.id
  name    = "API rate limit"
  kind    = "zone"
  phase   = "http_ratelimit"

  rules = [{
    description = "Limit requests to the API host"
    expression  = "(http.host eq \"${var.api_hostname}\")"
    action      = "block"
    ratelimit = {
      # cf.colo.id is REQUIRED by the Cloudflare API for count-by-IP rules
      # (the dashboard injects it silently; the API/Terraform path does not).
      # Verified against Cloudflare rate-limiting docs.
      characteristics     = ["ip.src", "cf.colo.id"]
      period              = var.api_rate_limit_period
      requests_per_period = var.api_rate_limit_requests
      mitigation_timeout  = var.api_rate_limit_mitigation_timeout
    }
  }]
}
