# Secret material read from AWS Secrets Manager (canonical store, alongside
# the mTLS bundles). NEVER hardcode these values.
#
# NOTE: a data source's result is persisted in Terraform STATE. That is why
# the S3 state backend is private + encrypted + versioned. The Cloudflare
# API token deliberately does NOT go through here — it is read from the
# CLOUDFLARE_API_TOKEN env var (providers.tf) so it never enters state. The
# origin secret, however, is a required attribute of the Transform Rule
# resource, so it is in state regardless; reading it from the canonical
# Secrets Manager entry is the cleanest, self-documenting source.

data "aws_secretsmanager_secret_version" "origin_secret" {
  secret_id = var.origin_secret_secret_id
}

locals {
  # The X-Origin-Secret value injected by the Transform Rule and checked by
  # the CloudFront viewer-request Function. Wrapped in sensitive() so it is
  # redacted in plan/console/CI output regardless of the source attribute's
  # implicit sensitivity (defense-in-depth for a public-repo project).
  origin_secret = sensitive(
    data.aws_secretsmanager_secret_version.origin_secret.secret_string
  )
}
