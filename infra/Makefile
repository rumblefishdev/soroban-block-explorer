PRODUCTION_APP := node dist/bin/production.js
CICD_APP := node dist/bin/cicd.js

build:
	cd .. && npx nx build @rumblefish/soroban-block-explorer-aws-cdk

# --- Production ---

synth-production: build
	npx cdk --app "$(PRODUCTION_APP)" synth

diff-production: build
	npx cdk --app "$(PRODUCTION_APP)" diff

deploy-production: build
	npx cdk --app "$(PRODUCTION_APP)" deploy --all --require-approval broadening

destroy-production: build
	npx cdk --app "$(PRODUCTION_APP)" destroy --all --force

deploy-production-network: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Network --require-approval broadening

destroy-production-network: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Network --force

deploy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-LedgerBucket --require-approval broadening

destroy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-LedgerBucket --force

deploy-production-compute: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Compute --require-approval broadening

destroy-production-compute: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Compute --force

deploy-production-ingestion: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Ingestion --require-approval broadening

destroy-production-ingestion: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Ingestion --force

deploy-production-delivery: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Delivery --require-approval broadening

destroy-production-delivery: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Delivery --force

deploy-production-apigateway: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-ApiGateway --require-approval broadening

destroy-production-apigateway: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-ApiGateway --force

deploy-production-observability: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Observability --require-approval broadening

destroy-production-observability: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Observability --force

deploy-production-cloudwatch: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-CloudWatch --require-approval broadening

destroy-production-cloudwatch: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-CloudWatch --force

synth-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" synth Explorer-production-HetznerDns

diff-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" diff Explorer-production-HetznerDns

deploy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-HetznerDns --require-approval broadening

destroy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-HetznerDns --force

# --- Cloudflare bootstrap (TF-state bucket for infra/cloudflare/) ---
# Requires "provisionCloudflareBootstrap": true in envs/production.json.

deploy-production-cloudflare-bootstrap: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-CloudflareBootstrap --require-approval broadening

destroy-production-cloudflare-bootstrap: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-CloudflareBootstrap --force

# --- Frontend SPA (build + S3 sync + CloudFront invalidation) ---
# Requires Explorer-production-Delivery already deployed (make deploy-production-delivery).
# VITE_API_BASE_URL is baked from cloudflareApiDomainName — the SPA calls the API
# through the Cloudflare-fronted host (task 0277), NOT the legacy apiDomainName.

build-production-web:
	cd .. && VITE_API_BASE_URL=https://$$(jq -r '.cloudflareApiDomainName' infra/envs/production.json) \
		VITE_TURNSTILE_SITE_KEY=$$(jq -r '.turnstileSiteKey // ""' infra/envs/production.json) \
		npx nx build @rumblefish/soroban-block-explorer-web

deploy-production-web: build-production-web
	@BUCKET=$$(aws cloudformation describe-stacks --region eu-central-1 \
		--stack-name Explorer-production-Delivery \
		--query "Stacks[0].Outputs[?OutputKey=='SpaBucketName'].OutputValue" --output text); \
	DIST=$$(aws cloudformation describe-stacks --region eu-central-1 \
		--stack-name Explorer-production-Delivery \
		--query "Stacks[0].Outputs[?OutputKey=='DistributionId'].OutputValue" --output text); \
	echo "SPA bucket=$$BUCKET  distribution=$$DIST"; \
	cd .. && aws s3 sync web/dist/ "s3://$$BUCKET/" --delete && \
	aws cloudfront create-invalidation --distribution-id "$$DIST" --paths "/*"

# --- CI/CD (shared, deploy once per account) ---

deploy-cicd: build
	npx cdk --app "$(CICD_APP)" deploy --require-approval broadening

destroy-cicd: build
	npx cdk --app "$(CICD_APP)" destroy --all --force

diff-cicd: build
	npx cdk --app "$(CICD_APP)" diff

# --- Shared ---

bootstrap:
	npx cdk bootstrap
