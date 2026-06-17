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
# The token MUST be zone-scoped to rumblefishdev.com, least-privilege — enough
# to manage ONE DNS record + AOP: DNS:Edit, SSL and Certificates:Edit. It does
# NOT need Zone:Edit or Rulesets (those belong to rf-domains). Never the Global
# API Key.

provider "cloudflare" {
  # api_token intentionally omitted — read from CLOUDFLARE_API_TOKEN.
}
