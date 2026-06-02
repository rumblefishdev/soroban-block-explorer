# The Cloudflare zone for the delegated subdomain.
#
# Managed as a RESOURCE so its assigned Cloudflare name servers are exposed
# (see outputs.tf) — those NS values are what the owner of the parent
# `rumblefish.dev` zone must set to delegate authority (task 0277 Step 1
# sign-off / Step 4 cutover). This is an NS delegation change, NOT a
# registrar change.
#
# If the zone already exists in the account (created in the dashboard),
# import it instead of recreating:
#   terraform import cloudflare_zone.this <zone_id>

resource "cloudflare_zone" "this" {
  account = {
    id = var.cloudflare_account_id
  }
  name = var.zone_name
  type = "full" # full setup (Cloudflare authoritative for the subdomain)
}
