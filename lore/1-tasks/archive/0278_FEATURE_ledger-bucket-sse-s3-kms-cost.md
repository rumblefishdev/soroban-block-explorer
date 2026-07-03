---
id: '0278'
title: 'Ledger bucket: drop SSE-KMS for SSE-S3 to kill KMS request cost'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: ['infra', 'cost', 'effort-small', 'priority-medium']
links: []
history:
  - date: 2026-06-02
    status: active
    who: fmazur
    note: 'Task created — senior flagged KMS request cost on production-stellar-ledger-data'
  - date: 2026-06-02
    status: completed
    who: fmazur
    note: >
      Switched ledger bucket to SSE-S3 (AES256), decoupled from retention.
      1 infra file + 4 doc spots changed. Verified by cdk synth (AES256 +
      Retain) and cdk diff against live prod (in-place encryption update,
      no replacement). Remaining operational step: `make
      deploy-production-ledger-bucket`.
---

# Ledger bucket: drop SSE-KMS for SSE-S3 to kill KMS request cost

## Summary

The `production-stellar-ledger-data` S3 bucket is encrypted with **SSE-KMS**
(`KMS_MANAGED`, AWS-managed `aws/s3` key) and **without S3 Bucket Keys**. Every
`PutObject`/`GetObject` therefore makes a per-object KMS call
(`GenerateDataKey`/`Decrypt`), and the high-volume ingest pipeline (Galexie
writes one XDR per ledger, the processor Lambda reads each one) drives a large,
growing KMS request bill. Senior flagged the cost. The data is **public on-chain
data** — no compliance need for KMS — so switch the bucket to **SSE-S3
(AES256)**, which is encrypted at rest but makes **zero** KMS calls.

## Status: Completed

**Current state:** Code + docs done and verified. `cdk diff` against live
production shows an in-place encryption update (no replacement). The only
remaining step is operational: run `make deploy-production-ledger-bucket`
(deliberately left to a human — touches production).

## Context

- Bucket defined in `infra/src/lib/stacks/ledger-bucket-stack.ts:29-40`.
- Encryption is gated by `config.kmsEncryption` (true in
  `infra/envs/production.json`).
- **Trap:** the same `kmsEncryption` flag also drives `removalPolicy`
  (KMS → `RETAIN`, else `DESTROY`) and `autoDeleteObjects`. Naively flipping the
  flag to `false` would set the **production** bucket to `DESTROY` +
  `autoDeleteObjects: true` — unacceptable. The encryption choice MUST be
  decoupled from the retention behaviour.
- Existing objects keep their original SSE-KMS encryption and stay readable;
  only newly written objects use SSE-S3. No re-encryption / backfill needed.

## Implementation Plan

### Step 1: Decouple encryption from retention

Make production keep `RemovalPolicy.RETAIN` + `autoDeleteObjects: false`
regardless of the encryption choice (production = retained data), so switching
encryption can never flip the bucket to destroy-on-delete.

### Step 2: Switch encryption to SSE-S3

Set the bucket to `s3.BucketEncryption.S3_MANAGED` (AES256) for production.
No KMS key, no Bucket Keys needed — SSE-S3 makes no KMS calls.

### Step 3: Verify diff is no-replacement

`cdk diff` must show only an encryption-config update (and removal-policy
decoupling), **no bucket replacement**. Confirm IAM grants still work (KMS
decrypt perms become unnecessary for new objects but stay harmless for old ones).

## Acceptance Criteria

- [x] Production ledger bucket uses `S3_MANAGED` (AES256); zero KMS calls on
      new Put/Get. (Synth shows `SSEAlgorithm: AES256`.)
- [x] `removalPolicy: RETAIN` + `autoDeleteObjects: false` preserved for
      production (no accidental destroy). (Synth: `DeletionPolicy: Retain`,
      `UpdateReplacePolicy: Retain`; unchanged in diff.)
- [x] `cdk diff` shows in-place update, **no bucket replacement**. (Real
      change-set diff: only `aws:kms → AES256` on `BucketEncryption`, same
      LogicalId, no `[-]/[+]` on the bucket.)
- [x] Existing KMS-encrypted objects still readable by Galexie + processor.
      (Per-object encryption is set at write time; old objects keep SSE-KMS and
      stay readable via the AWS-managed `aws/s3` key. No customer KMS key →
      `grantRead`/`grantReadWrite` never added explicit `kms:*` IAM, so IAM is
      unchanged.)
- [x] **Docs updated** — `infrastructure/infrastructure-overview.md` (2 spots)
      and `technical-design-general-overview.md` (2 spots) updated: ledger
      bucket now documented as SSE-S3 (AES256); RDS/ECR KMS statements left
      intact. (`indexing-pipeline-overview.md` only names the bucket, no
      encryption claim → no change needed.)
- [ ] **API types regenerated** — **N/A** (no `crates/api`, `Cargo.*`, or
      `libs/api-types` change; infra-only).

## Notes

- Alternative considered: enable S3 Bucket Keys (`bucketKeyEnabled: true`),
  keeping SSE-KMS but cutting ~99% of KMS calls. Rejected — data is public, so
  KMS buys nothing here; SSE-S3 removes the cost entirely rather than reducing
  it.
- S3 cannot be fully unencrypted (AWS enforces at-rest encryption since
  Jan 2023); SSE-S3 is the free, KMS-free baseline.

## Implementation Notes

- **`infra/src/lib/stacks/ledger-bucket-stack.ts`** — `encryption` is now a
  constant `s3.BucketEncryption.S3_MANAGED` (was a `config.kmsEncryption`
  ternary). `removalPolicy` / `autoDeleteObjects` still read
  `config.kmsEncryption` (now purely a "retained prod vs ephemeral env"
  signal), so production stays `RETAIN` + no auto-delete.
- **Docs (ADR 0032):** updated `infrastructure/infrastructure-overview.md`
  (hardening baseline + launch-readiness bullet) and
  `technical-design-general-overview.md` (security baseline + go-live
  checklist). RDS/ECR KMS statements left intact — only the ledger bucket
  changed. `indexing-pipeline-overview.md` only names the bucket (no
  encryption claim), so no change.
- **Verification:** `make synth-production` → `SSEAlgorithm: AES256`,
  `DeletionPolicy/UpdateReplacePolicy: Retain`. `cdk diff
Explorer-production-LedgerBucket` (real change-set) → single change
  `aws:kms → AES256` on `BucketEncryption`, same LogicalId
  `LedgerData3D556F09`, no replacement.

## Design Decisions

### From Plan

1. **SSE-S3 over Bucket Keys:** data is public on-chain XDR, so KMS buys no
   confidentiality. SSE-S3 removes the request cost entirely instead of merely
   reducing it (~99%) as Bucket Keys would.
2. **Decouple encryption from retention:** encryption is now a constant;
   `kmsEncryption` only governs `removalPolicy`/`autoDeleteObjects`, so flipping
   encryption can never destroy the production bucket.

### Emerged

3. **Left the flag named `kmsEncryption`:** it no longer drives encryption on
   this bucket, only retention. Renaming it would ripple into `ingestion-stack`
   (ECR) and `types.ts` — out of scope for a cost hotfix. Documented via inline
   comment instead. Follow-up candidate if the flag's meaning is revisited.
4. **Did not touch the Galexie ECR repo** (`ingestion-stack.ts:102`), which
   also uses `kmsEncryption` for KMS encryption. ECR is low-traffic (image pull
   at task start), so KMS request cost is negligible; senior flagged only the
   S3 bucket. Out of scope.
5. **Marked completed before deploy:** all code/docs are done and diff-verified;
   the actual `cdk deploy` is a human-run production step (left per policy).
