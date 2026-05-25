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
