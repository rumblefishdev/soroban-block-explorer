# SCF Milestone 1 — Form Answers

## 1. Tranche Deliverables

**Deliverable 1 — Indexing Pipeline & Core Infrastructure** (as originally
approved).

What is live and verifiable today:

- **Galexie on AWS ECS Fargate** is running on mainnet and writing
  `LedgerCloseMeta` XDR files to S3. At submission time it is closing the
  remaining gap between the imported ClickHouse backfill and the live network
  head, so S3 can receive many files in quick succession; once caught up
  (expected within ~2 days), the same task settles into the normal mainnet
  cadence of one file every ~5–6 seconds.
- **Rust Ledger Processor Lambda** parses each file and writes ledgers,
  transactions, operations, account changes, Soroban invocations, and CAP-67
  events into our database.
- **Gap-free ledger history** from Soroban-mainnet activation to current tip
  (`max(sequence) − min(sequence) + 1 − count(DISTINCT sequence) = 0`).
- **Full-content CAP-67 Soroban events** stored as one decoded row per event
  (not raw XDR), queryable by contract or by transaction hash.
- **Infrastructure as code:** AWS CDK (`cdk deploy` from an operator's
  machine) plus an Ansible playbook for the Hetzner database host
  (clean-host execution, no manual one-off steps).
- **Monitoring:** CloudWatch dashboard plus production Galexie-lag,
  ClickHouse write-failure, API, and enrichment alarms; the production alarm
  set is currently healthy.
- **API foundation:** Rust (axum + utoipa) scaffold with eight feature
  modules and published OpenAPI specification.

**In-tranche scope refinement:** the production datastore was migrated
mid-tranche from PostgreSQL on AWS RDS to ClickHouse on Hetzner. The
deliverable scope is unchanged. The drivers were fit (columnar OLAP for
our read-heavy explorer workload, ~10× compression) and cost (~$126/mo
Hetzner vs $800+/mo RDS for the equivalent ~8 TB working set).

**Full evidence — acceptance criteria mapping, queries with current output,
AWS screenshots, architecture diagram, ADR references, and the complete pivot
rationale:**
https://drive.google.com/drive/folders/13r5itsEg4-DxN7qvet4qZYn9jrQ09Mk4

## 2. Deliverable Verification - Video

https://drive.google.com/drive/folders/13r5itsEg4-DxN7qvet4qZYn9jrQ09Mk4

## 3. Additional Deliverable Verification

**Evidence package (Google Drive):** https://drive.google.com/drive/folders/13r5itsEg4-DxN7qvet4qZYn9jrQ09Mk4 — contains
`milestone-1-evidence.pdf` (full acceptance-criteria walkthrough,
architecture diagram, AWS screenshots, current ClickHouse query outputs,
pivot rationale) and the demo video.

**Source code (public):**

- Repository: https://github.com/rumblefishdev/soroban-block-explorer
- Project task ledger (every M1 task with status + ADR links):
  https://rumblefishdev.github.io/soroban-block-explorer/

**Operational endpoints (private, available on request):** production
CloudWatch dashboard `production-soroban-explorer` (eu-central-1) and
production ClickHouse `ch.sorobanscan.rumblefish.dev` (mTLS, client
certificate issued on request).

## 4. Support Needed

—
