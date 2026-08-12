PRODUCTION_APP := node dist/bin/production.js
CICD_APP := node dist/bin/cicd.js

# --- Deploy ledger (lore-0455) ---
# Every successful production mutation records WHICH commit shipped, as the
# last line of its recipe (make aborts on the first failing line, so the
# record can only follow a successful deploy). One SSM parameter whose
# built-in version history IS the ledger — query it read-only:
#   aws ssm get-parameter-history --region eu-central-1 \
#     --name /production/deploy/git-sha \
#     --query 'Parameters[].[LastModifiedDate,Value]' --output table
# Value: "<full-sha>[-dirty] <scope>". "-dirty" = the working tree carried
# uncommitted changes, i.e. the sha alone does NOT reproduce what shipped.
# Deliberately NOT a git tag (a second ledger drifts against the first) and
# NOT CloudFormation stack tags (tag churn touches every resource on every
# deploy, polluting the diffs the deploy guard depends on). --region is
# explicit because operator admin sessions have defaulted to us-east-1
# before (0449 gotcha).
#
# Two honest limits: (1) a FAILED `deploy-production` (--all) may still have
# shipped SOME stacks before the failure — that partial deploy records
# nothing; CloudFormation stack events are the source of truth for partials.
# (2) A failed LEDGER write does not fail make — the deploy already
# happened, and a bookkeeping error must not masquerade as a deploy error —
# it screams instead and tells you the manual command.
define record_deploy
	@sha="$$(git rev-parse HEAD)"; \
	if [ -n "$$(git status --porcelain)" ]; then sha="$$sha-dirty"; fi; \
	if aws ssm put-parameter --region eu-central-1 \
		--name /production/deploy/git-sha \
		--type String --overwrite \
		--value "$$sha $(1)" >/dev/null; then \
		echo "deploy ledger: $$sha $(1)"; \
	else \
		echo "!!! DEPLOY SUCCEEDED but the ledger write FAILED — record it manually:"; \
		echo "    aws ssm put-parameter --region eu-central-1 --name /production/deploy/git-sha --type String --overwrite --value \"$$sha $(1)\""; \
	fi
endef

build:
	cd .. && npx nx build @rumblefish/soroban-block-explorer-aws-cdk

# --- Production ---

synth-production: build
	npx cdk --app "$(PRODUCTION_APP)" synth

# --strict: without it cdk diff silently hides entries containing
# non-ASCII characters, so a clean-looking diff is not proof of a clean
# diff (measured during the 0312 investigation).
diff-production: build
	npx cdk --app "$(PRODUCTION_APP)" diff --strict

# Deploying --all ships EVERY pending delta, including changes parked by
# other tasks: 0312's undeployed CloudflareBootstrap slim-down sat in
# production for weeks, and any unrelated `make deploy-production` would
# have shipped it silently as a stowaway. So: show the full diff first,
# then require a literal "yes". Non-interactive bypass: FORCE=1.
deploy-production: build
	npx cdk --app "$(PRODUCTION_APP)" diff --strict
	@if [ "$(FORCE)" != "1" ]; then \
		printf '\nThe deploy is --all: EVERY delta above ships now, parked or not.\nType yes to continue: '; \
		read ans; [ "$$ans" = "yes" ] || { echo "aborted — nothing deployed"; exit 1; }; \
	fi
	npx cdk --app "$(PRODUCTION_APP)" deploy --all --require-approval broadening
	$(call record_deploy,all)

destroy-production: build
	npx cdk --app "$(PRODUCTION_APP)" destroy --all --force

deploy-production-network: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Network --require-approval broadening
	$(call record_deploy,network)

destroy-production-network: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Network --force

deploy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-LedgerBucket --require-approval broadening
	$(call record_deploy,ledger-bucket)

destroy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-LedgerBucket --force

deploy-production-compute: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Compute --require-approval broadening
	$(call record_deploy,compute)

destroy-production-compute: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Compute --force

deploy-production-ingestion: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Ingestion --require-approval broadening
	$(call record_deploy,ingestion)

destroy-production-ingestion: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Ingestion --force

deploy-production-delivery: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Delivery --require-approval broadening
	$(call record_deploy,delivery)

destroy-production-delivery: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Delivery --force

deploy-production-apigateway: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-ApiGateway --require-approval broadening
	$(call record_deploy,apigateway)

destroy-production-apigateway: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-ApiGateway --force

deploy-production-observability: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Observability --require-approval broadening
	$(call record_deploy,observability)

destroy-production-observability: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Observability --force

deploy-production-cloudwatch: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-CloudWatch --require-approval broadening
	$(call record_deploy,cloudwatch)

destroy-production-cloudwatch: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-CloudWatch --force

synth-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" synth Explorer-production-HetznerDns

diff-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" diff Explorer-production-HetznerDns

deploy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-HetznerDns --require-approval broadening
	$(call record_deploy,hetzner-dns)

destroy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-HetznerDns --force

# --- Cloudflare bootstrap (TF-state bucket for infra/cloudflare/) ---
# Requires "provisionCloudflareBootstrap": true in envs/production.json.

deploy-production-cloudflare-bootstrap: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-CloudflareBootstrap --require-approval broadening
	$(call record_deploy,cloudflare-bootstrap)

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
	$(call record_deploy,web)

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
