# SCF Milestone 1 — Form Field Text (copy-paste)

> Short text to paste into each field of the Stellar Community Fund
> **Deliverable Verification** form for Milestone 1 (Tranche 1).
>
> Each block is **the exact text** to paste. Anything in `<ANGLE_BRACKETS>` is a
> placeholder you must replace before submission.
>
> Full evidence and rationale live in the companion document
> [`milestone-1-evidence.md`](./milestone-1-evidence.md), exported to PDF and
> attached in Google Drive next to the video.

---

## Field 1 — Tranche Deliverables

> **Deliverable 1 — Indexing Pipeline & Core Infrastructure** (as originally
> approved).
>
> What is live and verifiable today:
>
> - **Galexie on AWS ECS Fargate** is running on mainnet and writing
>   `LedgerCloseMeta` XDR files to S3. At submission time it is closing the
>   remaining gap between the imported ClickHouse backfill and the live network
>   head, so S3 can receive many files in quick succession; once caught up
>   (expected within ~2 days), the same task settles into the normal mainnet
>   cadence of one file every ~5–6 seconds.
> - **Rust Ledger Processor Lambda** parses each file and writes ledgers,
>   transactions, operations, account changes, Soroban invocations, and CAP-67
>   events into our database.
> - **Gap-free ledger history** from Soroban-mainnet activation to current tip
>   (`max(sequence) − min(sequence) + 1 − count(DISTINCT sequence) = 0`).
> - **Full-content CAP-67 Soroban events** stored as one decoded row per event
>   (not raw XDR), queryable by contract or by transaction hash.
> - **Infrastructure as code:** AWS CDK (`cdk deploy` from an operator's
>   machine) plus an Ansible playbook for the Hetzner database host
>   (clean-host execution, no manual one-off steps).
> - **Monitoring:** CloudWatch dashboard plus production Galexie-lag,
>   ClickHouse write-failure, API, and enrichment alarms; the production alarm
>   set is currently healthy.
> - **API foundation:** Rust (axum + utoipa) scaffold with eight feature
>   modules and published OpenAPI specification.
>
> **In-tranche scope refinement:** the production datastore was migrated
> mid-tranche from PostgreSQL on AWS RDS to ClickHouse on Hetzner. The
> deliverable scope is unchanged. The drivers were fit (columnar OLAP for
> our read-heavy explorer workload, ~10× compression) and cost (~$126/mo
> Hetzner vs $800+/mo RDS for the equivalent ~8 TB working set).
>
> **Full evidence — acceptance criteria mapping, queries with current output,
> AWS screenshots, architecture diagram, ADR references, and the complete pivot
> rationale:** > `<DRIVE_LINK_TO_milestone-1-evidence.pdf>`

---

## Field 2 — Deliverable Verification - Video

> `<VIDEO_URL>`

---

## Field 3 — Additional Deliverable Verification

> **Evidence package (Google Drive):** `<DRIVE_FOLDER_LINK>` — contains
> `milestone-1-evidence.pdf` (full acceptance-criteria walkthrough,
> architecture diagram, AWS screenshots, current ClickHouse query outputs,
> pivot rationale) and the demo video.
>
> **Live & anonymous (verify directly in a browser):**
>
> - Production API health: `https://api.sorobanscan.rumblefish.dev/health`
> - Production OpenAPI / Swagger UI: `https://api.sorobanscan.rumblefish.dev/api-docs`
>
> **Source code (public):**
>
> - Repository: `https://github.com/rumblefishdev/soroban-block-explorer`
> - Project task ledger (every M1 task with status + ADR links):
>   `https://rumblefishdev.github.io/soroban-block-explorer/`
>
> **Operational endpoints (private, available on request):** production
> CloudWatch dashboard `production-soroban-explorer` (eu-central-1) and
> production ClickHouse `ch.sorobanscan.rumblefish.dev` (mTLS, client
> certificate issued on request).

---

## Field 4 — Support Needed

> —

---

## Pre-submission checklist

- [ ] Confirm `milestone-1-evidence.md` includes only the architecture diagram
      and AWS Console screenshots as images; ClickHouse query outputs should be
      embedded as text from the latest captured outputs, not screenshots.
- [ ] Confirm all figures, tables, and query output blocks in
      `milestone-1-evidence.md` have captions.
- [ ] `milestone-1-evidence.md` finalised and exported to PDF.
- [ ] PDF uploaded to a Google Drive folder with link-sharing set to
      "anyone with the link can view".
- [ ] Drive folder link copied into Field 1 closer **and** Field 3 opener
      (replace both `<DRIVE_*>` placeholders).
- [ ] Video uploaded with public sharing; URL pasted into Field 2.
- [ ] All `<ANGLE_BRACKET>` placeholders in this file replaced.
- [ ] `curl -sS https://api.sorobanscan.rumblefish.dev/health` returns
      `{"status":"ok"}` (status 200).
- [ ] Production Swagger UI loads in an incognito window.
- [ ] Field 4 — decide between `—` and a specific support request.
- [ ] English-only across all four blocks (no Polish/internal slang).
