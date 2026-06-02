# Provider configuration.
#
# Cloudflare API token is supplied via the CLOUDFLARE_API_TOKEN environment
# variable, which the provider reads automatically — it is therefore NOT a
# Terraform variable and never lands in state or .tf. Source it from AWS
# Secrets Manager at apply time, e.g.:
#
#   export CLOUDFLARE_API_TOKEN=$(aws secretsmanager get-secret-value \
#     --secret-id soroban/production/cloudflare/api-token \
#     --query SecretString --output text)
#
# The token MUST be zone-scoped, least-privilege (Zone:Edit, DNS:Edit,
# Zone Settings:Edit, SSL and Certificates:Edit, Page Rules / Rulesets) —
# never the Global API Key.

provider "cloudflare" {
  # api_token intentionally omitted — read from CLOUDFLARE_API_TOKEN.
}

provider "aws" {
  region = var.aws_region
  # Credentials from the operator's AWS profile (same one used for CDK).
}
