# Patch register — sweep at the end of the epic (task 0553)

Decided 2026-09-30: the epic ships with these patches; its last step rebuilds
each one in the shape we would build from scratch.

- **Ledger source as scattered ifs** (D1): `ownsLedgers` in `app.ts`,
  `if (ledgerBucket && ledgerBucketArn && ledgerBucketName)` in
  `compute-stack.ts`, `if (galexieCluster && galexieService)` in
  `cloudwatch-stack.ts`. From scratch: one "ledgers" module with a Galexie
  and a public-lake variant, each providing the bucket name, the doorbell and
  its own alarms; compute and CloudWatch without a branch.
- **Optional fields that only make sense together** (D1): Galexie sizing
  required even for a lake environment; `publicArchivePrefix?`,
  `clickhouseDatabase?` as conditional env. From scratch: `ledgerSource` as
  a discriminated union carrying its own fields; the database always explicit.
- **Two env vars that can contradict, plus a guard** (A, D2 fix 2):
  `BUCKET_NAME` + `PUBLIC_ARCHIVE_PREFIX`, and a start-up refusal of the bad
  pair. From scratch: one `LedgerSource { OwnBucket | PublicLake(prefix) }`
  parsed once.
- **Network check assumes pubnet for an own bucket** (A): a future testnet with
  its own Galexie would be refused at start. Falls out of the item above.

## Kept on purpose

- **API certificates requested by hand, ARN in `envs/*.json`** (D4,
  2026-10-01). From scratch: `acm.Certificate` in the ApiGateway stack for
  both environments. Kept: it saves one command per environment for its
  whole life, and moving production's certificate changes the public API
  edge. Production's validation record still resolves (checked with `dig`
  2026-10-01), so the move would not hang; the interruption question was
  never checked.
