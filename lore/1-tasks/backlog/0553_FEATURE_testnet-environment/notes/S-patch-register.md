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
- **Two env vars that can contradict, plus a guard** (A, D2 fix 2): kept on
  purpose (2026-10-02). A `LedgerSource` type (#595) was built and dropped:
  two plain fields and two start-up checks read more simply than a new type
  with five methods. The empty-lake start (#597) adds one more
  `bucket == PUBLIC_BUCKET` check, in `first_ledger_to_read`.
- **Network check assumes pubnet for an own bucket** (A): decided 2026-10-02
  that this is the design, not debt — testnet reads the lake and will not run
  its own Galexie, so our own bucket is mainnet's only. Kept as the start-up
  refusal of any other passphrase there (`LedgerSource`, #595).

## Kept on purpose

- **API certificates requested by hand, ARN in `envs/*.json`** (D4,
  2026-10-01). From scratch: `acm.Certificate` in the ApiGateway stack for
  both environments. Kept: it saves one command per environment for its
  whole life, and moving production's certificate changes the public API
  edge. Production's validation record still resolves (checked with `dig`
  2026-10-01), so the move would not hang; the interruption question was
  never checked.
