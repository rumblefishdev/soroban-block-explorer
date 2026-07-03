output "api_record_fqdn" {
  description = "The proxied API hostname this module manages (empty until create_dns_record=true)."
  value       = var.create_dns_record ? cloudflare_dns_record.api[0].name : ""
}

output "api_mtls_cert_id" {
  description = "Uploaded AOP client cert id (empty until enable_api_mtls_aop=true). Useful to cross-check against the API GW truststore."
  value       = var.enable_api_mtls_aop ? cloudflare_authenticated_origin_pulls_certificate.client[0].certificate_id : ""
}
