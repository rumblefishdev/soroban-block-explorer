# Ledger source and doorbell cadence (task 0553)

### Ledger source for testnet — decided 2026-09-29

**Mainnet stays on its own Galexie. Testnet reads the public data lake**
(`aws-public-blockchain/v1.1/stellar/ledgers/testnet/<genesis>/`) instead of
running a second Galexie — but only once a week-long measurement confirms the
lake is fresh and gap-free. After testnet goes live, a standing measurement
compares the lake with our mainnet Galexie for ~2 months.

Why it is feasible with no indexer logic change: the indexer treats the S3
event as a doorbell only — it walks `max(sequence)+1` in `BUCKET_NAME` by HEAD
(`crates/indexer/src/handler/mod.rs:236-266`). It needs a key prefix and the
lake's region (`us-east-2`); the doorbell becomes a schedule (the lake publishes
no notifications).

Evidence so far (2026-09-29, read-only):

- **Freshness, one night.** Every 15 s, newest file on each side. Mainnet, 620
  samples: lake equal to our Galexie 92.9%, one ledger ahead 6.6%, one behind
  0.5%, never two behind. Testnet, 995 samples over 6 h: lake 0–1 ledgers
  behind RPC (99.8%), 2 behind twice. Zero missing files in 25 partition counts.
- **S3 timestamps cannot measure the lake.** SDF rewrites the previous week's
  objects every Sunday (73k / 96k / 155k objects on 09-13 / 09-20 / 09-27), so
  `LastModified` is the rewrite time. Only live sampling measures freshness.
- **Content.** Ledger 64,670,659 from both sources, decompressed: same size,
  same transactions, results and events. Differences: the order of ledger-entry
  changes within an operation, and `core_metrics` timings in diagnostic events.
  The parser groups changes by key, never by position, and stores no
  `core_metrics`, so neither reaches a table. One ledger — a sample, not proof.
- **Our Galexie is ~86% of the explorer's tagged AWS spend** (Cost Explorer,
  2026-08-29 → 09-27; ECS is the single `production-galexie-live` service).
  It runs at 94–97% of its 13,312 MiB memory limit (Container Insights daily
  max, 09-18 → 09-28).

### Doorbell cadence — decided 2026-09-30

The data lake sends no notifications, so a schedule rings the indexer: an
EventBridge schedule sends batches of delayed SQS messages, **one every 2 s**.
Measured (one hour, 720 testnet ledgers, none missing): a file lands in the
lake p50 2.37 s, p90 3.22 s, p99 6.59 s after ledger close. Simulated lag to
the database (lake + wait for the bell + 0.9 s processing, mainnet's median):

| Bell every    | p50       | p90       | p99       | indexer calls / day |
| ------------- | --------- | --------- | --------- | ------------------- |
| mainnet today | 2.1 s     | 3.1 s     | 4.1 s     | 17,280              |
| 1 s           | 3.8 s     | 4.7 s     | 7.9 s     | 86,400              |
| **2 s**       | **4.3 s** | **5.5 s** | **8.4 s** | **43,200**          |
| 6 s           | 6.4 s     | 8.9 s     | 11.2 s    | 14,400              |
| 60 s          | 33.9 s    | 57.9 s    | 62.9 s    | 1,440               |

Mainnet's latency is out of reach: the lake alone publishes later than our
Galexie delivers to the database. Each step below 2 s buys ~0.5 s and
doubles the calls. Re-check the duration of an empty call after deploy.

**Stall alarm (proposed 2026-09-30, replaces the reset alarm).** On the lake a
reset never moves our sequence backwards — the old genesis folder just stops
growing — so one alarm covers a lake outage, a reset and a protocol upgrade we
cannot decode: `IngestionLagSeconds` (already published per environment)
above 60 s for three consecutive minutes, missing data treated as breaching.
The worst lag in the hour measured was ~10 s.
