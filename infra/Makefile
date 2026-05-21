STAGING_APP := node dist/bin/staging.js
PRODUCTION_APP := node dist/bin/production.js
BASTION_STAGING_APP := node dist/bin/bastion-staging.js
BASTION_PRODUCTION_APP := node dist/bin/bastion-production.js
CICD_APP := node dist/bin/cicd.js

build:
	cd .. && npx nx build @rumblefish/soroban-block-explorer-aws-cdk

# --- Staging ---

synth-staging: build
	npx cdk --app "$(STAGING_APP)" synth

diff-staging: build
	npx cdk --app "$(STAGING_APP)" diff

deploy-staging: build
	npx cdk --app "$(STAGING_APP)" deploy --all --require-approval broadening

destroy-staging: build
	npx cdk --app "$(STAGING_APP)" destroy --all --force

deploy-staging-network: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Network --require-approval broadening

destroy-staging-network: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Network --force

deploy-staging-rds: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Rds --require-approval broadening

destroy-staging-rds: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Rds --force

deploy-staging-ledger-bucket: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-LedgerBucket --require-approval broadening

destroy-staging-ledger-bucket: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-LedgerBucket --force

deploy-staging-migration: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Migration --require-approval broadening

destroy-staging-migration: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Migration --force

deploy-staging-partition: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Partition --require-approval broadening

destroy-staging-partition: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Partition --force

deploy-staging-compute: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Compute --require-approval broadening

destroy-staging-compute: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Compute --force

deploy-staging-ingestion: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Ingestion --require-approval broadening

destroy-staging-ingestion: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Ingestion --force

deploy-staging-delivery: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Delivery --require-approval broadening

destroy-staging-delivery: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Delivery --force

deploy-staging-apigateway: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-ApiGateway --require-approval broadening

destroy-staging-apigateway: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-ApiGateway --force

deploy-staging-observability: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-Observability --require-approval broadening

destroy-staging-observability: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-Observability --force

deploy-staging-cloudwatch: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-CloudWatch --require-approval broadening

destroy-staging-cloudwatch: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-CloudWatch --force

deploy-staging-bastion: build
	npx cdk --app "$(BASTION_STAGING_APP)" deploy --require-approval broadening

destroy-staging-bastion: build
	npx cdk --app "$(BASTION_STAGING_APP)" destroy --force

synth-staging-hetzner-dns: build
	npx cdk --app "$(STAGING_APP)" synth Explorer-staging-HetznerDns

diff-staging-hetzner-dns: build
	npx cdk --app "$(STAGING_APP)" diff Explorer-staging-HetznerDns

deploy-staging-hetzner-dns: build
	npx cdk --app "$(STAGING_APP)" deploy Explorer-staging-HetznerDns --require-approval broadening

destroy-staging-hetzner-dns: build
	npx cdk --app "$(STAGING_APP)" destroy Explorer-staging-HetznerDns --force

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

deploy-production-rds: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Rds --require-approval broadening

destroy-production-rds: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Rds --force

deploy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-LedgerBucket --require-approval broadening

destroy-production-ledger-bucket: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-LedgerBucket --force

deploy-production-migration: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Migration --require-approval broadening

destroy-production-migration: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Migration --force

deploy-production-partition: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-Partition --require-approval broadening

destroy-production-partition: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-Partition --force

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

deploy-production-bastion: build
	npx cdk --app "$(BASTION_PRODUCTION_APP)" deploy --require-approval broadening

destroy-production-bastion: build
	npx cdk --app "$(BASTION_PRODUCTION_APP)" destroy --force

synth-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" synth Explorer-production-HetznerDns

diff-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" diff Explorer-production-HetznerDns

deploy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" deploy Explorer-production-HetznerDns --require-approval broadening

destroy-production-hetzner-dns: build
	npx cdk --app "$(PRODUCTION_APP)" destroy Explorer-production-HetznerDns --force

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
