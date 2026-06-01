# SCF Milestone 1 — Deliverable Verification (Soroban Block Explorer)

> Copy-paste-ready text for the Stellar Community Fund **Deliverable
> Verification** form, Milestone 1 (Tranche 1).
>
> Each `## Field N` block below maps to one form field. The text inside the
> blockquote is the exact text to paste; the italic editorial note underneath
> stays in this file (do not paste it).
>
> Companion video script: `milestone_one_video_scenario/m1-scenario.md` (kept
> locally outside the repo — the uploaded video is the form-visible artifact).

---

## Field 1 — Tranche Deliverables

> **Deliverable 1 — Indexing Pipeline & Core Infrastructure.**
>
> Milestone 1 delivers an end-to-end Stellar mainnet indexing pipeline writing
> into our own database, with no third-party chain API on the live read path,
> plus the supporting cloud infrastructure, monitoring, and Rust API scaffolding
> that Milestones 2 and 3 build on.
>
> What is live and verifiable today (each item is demoed in the video, in order):
>
> 1. **Galexie on AWS ECS Fargate** runs continuously on mainnet and writes one
>    `LedgerCloseMeta` XDR file to S3 every ~5–6 seconds, matching mainnet close
>    cadence. Bucket: `production-stellar-ledger-data`.
> 2. **Ledger Processor Lambda** (Rust, `production-soroban-explorer-indexer`)
>    is triggered on every S3 object, decodes the XDR, and writes ledgers,
>    transactions, operations, account changes, Soroban contract invocations,
>    and CAP-67 contract events into our database. End-to-end ledger-close to
>    DB-write is well under the 10-second target.
> 3. **Gap-free ledger history** is in the database from the Soroban-mainnet
>    activation ledger through the current network tip. The on-camera SQL
>    `(max(sequence) − min(sequence) + 1) − count(DISTINCT sequence)` returns
>    `0`. Historical backfill was produced via local indexing and transported
>    to the production database per ADR 0045 (FREEZE + rsync + ATTACH PART);
>    the backfill and live indexer share the same writer code.
> 4. **Full-content CAP-67 Soroban events** are stored as one row per event
>    (not raw XDR), with both topics and data decoded and queryable. Spot-checks
>    use known Soroswap / Aquarius / Phoenix contracts and a transaction-hash
>    lookup.
> 5. **Infrastructure as code:** the AWS side is defined in AWS CDK and deploys
>    through CI/CD on GitHub Actions. The Hetzner database host is provisioned
>    with an Ansible playbook (`infra-hetzner/ansible/site.yml`) executed against
>    a clean host, with no manual one-off steps. Both halves are reproducible
>    from the public repository.
> 6. **Monitoring:** a CloudWatch dashboard (`production-soroban-explorer`)
>    aggregates Galexie ingestion freshness and Ledger Processor duration / error
>    rate. Two alarms are wired and healthy: `production-galexie-ingestion-lag`
>    (fires if ledgers stop arriving) and `production-indexer-ch-write-failures`
>    (fires if database writes fail). The lag alarm was fire-tested in staging
>    by stopping ingestion and confirming the ALARM state.
> 7. **API foundation:** a Rust (axum + utoipa) API scaffold with eight feature
>    modules is in place and its OpenAPI specification is published. Endpoints
>    are implemented in Milestone 2, but the contract is now stable.
>
> **Scope refinement during the tranche.** The originally approved D1
> referenced PostgreSQL on AWS RDS as the primary datastore. During Milestone 1
> we migrated the primary datastore to ClickHouse running on a dedicated
> Hetzner host (`ch-prod-01`, mTLS behind Caddy), with the Lambda connecting
> over mutually-authenticated TLS. The deliverable scope — gap-free mainnet
> indexing with full Soroban event data — is unchanged. The driver was fit and
> cost: our full history takes ~700 GB in ClickHouse versus an estimated ~8 TB
> in PostgreSQL, and the run-rate is roughly $140/month (server + backup
> storage box) versus $800+/month for the equivalent RDS instance alone.
> Rationale, trade-offs, and the RDS retirement schedule are in
> [ADR 0047](https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/lore/2-adrs/0047_clickhouse-primary-api-datastore.md).

_Editorial note: opens with the approved deliverable name to satisfy the form's
"match initially approved" note; the AC mapping mirrors §7.4 of the technical
design doc 1:1 (S3 → CH ledgers → CH events → IaC → monitoring); the pivot
disclosure is one paragraph at the bottom so reviewers see it but it does not
displace the headline._

---

## Field 2 — Deliverable Verification - Video

> `<TODO: public URL — YouTube / Google Drive / Vimeo / Loom — sharing set to public>`

_Editorial note: recorded from `m1-scenario.md`, ~5–6 minutes. Single-line input
in the form. Replace placeholder after upload and re-check the link is publicly
viewable (the form requires that explicitly)._

---

## Field 3 — Additional Deliverable Verification

> The video is sufficient on its own, but for deeper review the following
> resources back every claim above:
>
> **Source code & task ledger**
>
> - Public repository: https://github.com/rumblefishdev/soroban-block-explorer
> - Project task ledger (all tasks completed in M1, with status and ADR links):
>   https://rumblefishdev.github.io/soroban-block-explorer/
>
> **Architecture & design documents (in-repo)**
>
> - Technical design overview, including the verbatim Milestone 1 acceptance
>   criteria (§7.4 "Three-Milestone Delivery Plan → Deliverable 1"):
>   https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/docs/architecture/technical-design-general-overview.md
> - Database schema overview (DDL and field mappings for `ledgers`,
>   `transactions`, `operations`, `soroban_events`, etc.):
>   https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/docs/architecture/database-schema/database-schema-overview.md
> - ADR 0047 — ClickHouse on Hetzner as primary API datastore (rationale for
>   the in-tranche pivot from RDS PostgreSQL):
>   https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/lore/2-adrs/0047_clickhouse-primary-api-datastore.md
> - ADR 0045 — Local backfill + FREEZE/rsync/ATTACH PART transport to Hetzner
>   (how the historical ledger range was populated):
>   https://github.com/rumblefishdev/soroban-block-explorer/blob/develop/lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
>
> **Live endpoints (anonymous)**
>
> - Staging API health probe: https://api.staging.sorobanscan.rumblefish.dev/health
> - Staging OpenAPI / Swagger UI (M2 API surface, contract already stable):
>   https://api.staging.sorobanscan.rumblefish.dev/api-docs
>
> **Operational endpoints (private, available on request to SCF reviewers)**
>
> - Production CloudWatch dashboard: `production-soroban-explorer`
>   (eu-central-1). Read-only IAM access can be provisioned for a reviewer
>   AWS account on request.
> - Production ClickHouse: `ch.sorobanscan.rumblefish.dev` (mTLS — client
>   certificate issued on request).
>
> Tranche 1 deliverables are infrastructure / data and do not require
> application credentials for testing; credentials for the user-facing app
> will be supplied with the Milestone 2 submission.

_Editorial note: all in-repo links use `develop` branch (current default).
Anonymous staging probes let a reviewer self-verify in <30 seconds; the
private endpoints are listed so the reviewer knows what exists and how to
ask. Credentials block is intentionally deferred to M2 per the form's
helper text ("For tranches 2 and 3, include application credentials")._

---

## Field 4 — Support Needed

> —
>
> _(Optional: replace with one short paragraph if you want SCF to connect you
> with a specific resource — e.g. introductions to Stellar dev-advocates for
> the M2 launch, or a review pass from a Stellar core engineer on our CAP-67
> event coverage. Leave as `—` if nothing blocking.)_

_Editorial note: field is optional in the form. Default is the em-dash so the
field is not left blank (some forms reject empty required fields even when
the helper says optional). Decide whether to keep `—` or fill in something
specific right before submission._

---

## Appendix A — Acceptance-criteria → evidence map

For internal QA before submission. Each row maps one acceptance criterion from
§7.4 of the technical design doc to the place it is proven.

| AC  | Criterion (verbatim)                                                                                                                                                                                                                                              | Proven in                                                                              |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| 1   | S3 bucket contains consecutive `LedgerCloseMeta` files with timestamps matching mainnet ledger close times                                                                                                                                                        | Video Scenes 3 + 4 (ECS task running; S3 sorted by Last modified, refresh)             |
| 2   | **ClickHouse on Hetzner** `ledgers` table contains all ledgers from backfill start through current tip with no gaps                                                                                                                                               | Video Scene 6, query (2): `expected_span − distinct_ledgers = 0`                       |
| 3   | **ClickHouse on Hetzner** `soroban_events` table contains full-content rows for CAP-67 events in known Soroswap/Aquarius/Phoenix transactions (spot-checked by transaction hashes); decoded events are confirmed by re-expanding via `xdr_parser::extract_events` | Video Scene 6, queries (4) and (4b) — by contract and by tx hash                       |
| 4   | `cdk deploy` (AWS side) + `ansible-playbook` (Hetzner side) from clean environments produces the full working stack with no manual steps                                                                                                                          | Video Scenes 2 + 8 (architecture + `infra/` tour); repo CI workflow                    |
| 5   | CloudWatch dashboard accessible; Galexie lag alarm fires correctly in staging                                                                                                                                                                                     | Video Scene 7 (live dashboard + alarms in OK state; optional staging-ALARM screenshot) |

---

## Appendix B — Pre-submission checklist

- [ ] Video recorded from `m1-scenario.md`, ~5 minutes, uploaded to
      YouTube / Drive / Vimeo / Loom with **sharing set to public**.
- [ ] Video URL pasted into Field 2 (this file's only `<TODO: …>` marker
      cleared).
- [ ] All Field 3 GitHub links resolve on `develop` branch.
- [ ] Staging health probe returns `200 OK`:
      `curl -sS https://api.staging.sorobanscan.rumblefish.dev/health`
- [ ] Staging Swagger UI loads in an incognito window:
      https://api.staging.sorobanscan.rumblefish.dev/api-docs
- [ ] If a staging-ALARM screenshot was captured during the alarm fire-test,
      attach it (or mention it inline in Field 1, point 6).
- [ ] Field 4 — decide between `—` and a specific support request before
      submitting.
- [ ] No Polish / no internal slang anywhere in the four pasted blocks
      (English-only artifact policy).
