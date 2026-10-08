# Testnet reset — rebuild the testnet database from a new genesis

SDF resets Stellar Testnet two to four times a year, at 17:00 UTC, announced
at least two weeks ahead on the Stellar networks page. After a reset the
network starts again from ledger 1, under the same passphrase. Nothing in the
`testnet` database survives it: every table is keyed on the ledger sequence,
so the new chain's ledger N would overwrite the old one's.

The same steps build the database the first time (steps 2 and 4–8): pause
first (`indexerLambdaConcurrency: 0`), so the deploy starts nothing until the
database is ready.

> **Pause the indexer (step 2) before dropping the database (step 3).** An
> empty `testnet` database makes a running indexer start over from ledger 2
> of the folder it is configured with — until step 5, the OLD genesis — and
> the new chain then lands on top of it, two chains in one database with
> nothing to detect it.

An empty `testnet` database needs no seed: the indexer reads the lake from
the network's first closed ledger (2) on its own. It stores a ledger in
~0.9 s (mainnet's median indexer run against our own bucket, measured
2026-09-30 — an estimate here: the lake sits in another region, which slows
each read, and early testnet ledgers are light, which speeds it) while the
network closes one every ~5 s, so it gains ~0.9 ledgers a second, and
catching up takes about 22% of the time since the reset: ~5 h for a day,
~1.5 days for a week. Skip step 6 when that wait is acceptable. The first build — a chain
months old — takes the backfill.

## How it shows up

The public data lake starts a **new genesis folder** under
`s3://aws-public-blockchain/v1.1/stellar/ledgers/testnet/`, and the folder in
`publicArchivePrefix` (`infra/envs/testnet.json`) stops growing. The indexer
finds no next file, the newest indexed ledger ages, and
`testnet-ingestion-stall` fires (over 60 s for 3 minutes). The sequence never
goes backwards, so there is no other signal.

The folder names are undocumented, and the live chain has sat in a folder
dated a day after its reset (`2025-12-18/` for the 2025-12-17 reset) next to
an abandoned stub. The right folder is the one that **keeps growing**.

## Steps

**1. Confirm it is a reset**, not a lake outage (read-only):

```bash
aws s3 ls --no-sign-request --region us-east-2 s3://aws-public-blockchain/v1.1/stellar/ledgers/testnet/
curl -s -X POST https://soroban-testnet.stellar.org -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}'
```

A new dated folder, and an RPC tip far below the last ledger in `testnet`,
mean a reset. List the new folder twice a minute apart: its newest partition
must grow.

Not a reset — the RPC tip is close to the last indexed ledger:

- the indexer fails (`testnet-ledger-processor-error-rate`, its logs): testnet
  took a protocol upgrade the parser cannot decode yet — bump `stellar-xdr`
  (testnet upgrades weeks before mainnet, so mainnet needs the same bump);
- the indexer runs clean but finds no next file: the lake is late — wait, the
  alarm clears by itself once files land.

**2. Pause the testnet indexer:** `indexerLambdaConcurrency: 0` in
`infra/envs/testnet.json`, then `make -C infra deploy-testnet`. The same
setting disables the once-a-minute keepalive, so nothing queues while paused;
`testnet-ingestion-stall` stays in alarm until step 7.

**3. Drop the database** (production ClickHouse box, as `default`; irreversible,
testnet data only):

```sql
DROP DATABASE testnet SYNC
```

**4. Recreate the schema:** rerun the sidecar, which creates `testnet` and
applies `init.sql` to it (`EXPLORER_DATABASES`, `docker-compose.yml`):

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up db-clickhouse-init
```

Its log must end with `init.sql applied to testnet`.

**5. Point testnet at the new folder:** set `publicArchivePrefix` in
`infra/envs/testnet.json` to `v1.1/stellar/ledgers/testnet/<new folder>`,
merge, `make -C infra deploy-testnet` (still paused).

**6. Backfill from genesis** — for the first build, or when the catch-up at
the top is too long — with the operator write cert
([`docs/backfills.md`](../backfills.md)). The environment names the database,
the network and the folder; forgetting `CLICKHOUSE_DATABASE` writes into
mainnet's `default`, where the rows sit below its first ledger (50,457,424)
and are removable by that bound.

```bash
CLICKHOUSE_DATABASE=testnet \
STELLAR_NETWORK_PASSPHRASE='Test SDF Network ; September 2015' \
PUBLIC_ARCHIVE_PREFIX=v1.1/stellar/ledgers/testnet/<new folder> \
backfill-runner --clickhouse-url https://<ch-host> \
  --ch-cert <cert.pem> --ch-key <key.pem> --ch-ca infra-hetzner/ca/ca.crt \
  run --start 2 --end <RPC tip>
```

Ranges run in parallel as separate processes; after a parallel run,
`repair-tier1` is mandatory, with the indexer still paused.

**7. Resume:** `indexerLambdaConcurrency: 1`, `make -C infra deploy-testnet`.
This also enables the keepalive; its first wake continues from
`max(sequence) + 1`, or from ledger 2 when step 6 was skipped.

**8. Enrich the backfilled rows** — only after step 6. The live indexer
queues every new asset and NFT for the enrichment worker; `backfill-runner`
writes straight to ClickHouse and queues nothing, so the backfilled range has
no icons, asset names or NFT metadata until the enrichment backfill
(`crates/backfill-enrichment-runner`, binary `enrich`) drains it. It runs
from a laptop with the same operator write cert as step 6, after
`repair-tier1`; the indexer may already be running. `SOROBAN_RPC_URLS` must
name the testnet RPC — the NFT calls go to whatever pool it holds.

```bash
CLICKHOUSE_DATABASE=testnet \
SOROBAN_RPC_URLS=https://soroban-testnet.stellar.org \
enrich --clickhouse-url https://<ch-host> \
  --ch-cert <cert.pem> --ch-key <key.pem> --ch-ca infra-hetzner/ca/ca.crt \
  sep1-assets
```

Then the same with `nft-metadata`, and `status` to read the coverage. Rows
an earlier run filled with empty sentinels (an RPC of the wrong network, an
upstream outage) are retried with `nft-metadata --retry-sentinels`. A
transient failure leaves the key for the next run; rerun until `status` stops
moving.

## Done when

- `testnet-ingestion-stall` is back to OK;
- `SELECT max(sequence) FROM testnet.ledgers` follows the RPC tip within
  seconds;
- one recent transaction resolves on the testnet site under the hash testnet
  RPC reports for it;
- after step 8, `enrich status` (with `CLICKHOUSE_DATABASE=testnet`) shows
  the backfilled assets and NFTs enriched.
