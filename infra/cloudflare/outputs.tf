output "zone_id" {
  description = "Cloudflare zone ID."
  value       = cloudflare_zone.this.id
}

output "cloudflare_name_servers" {
  description = <<-EOT
    The Cloudflare-assigned name servers. THESE are the values the owner of
    the parent `rumblefish.dev` zone must set as the NS records for the
    delegated `sorobanscan` subdomain to make Cloudflare authoritative
    (task 0277 Step 1 sign-off / Step 4 cutover). NS delegation, not a
    registrar change.
  EOT
  value       = cloudflare_zone.this.name_servers
}
