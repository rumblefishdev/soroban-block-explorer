# Testnet reset — rebuild the testnet database from a new genesis

SDF resets Stellar Testnet two to four times a year, at 17:00 UTC, announced
at least two weeks ahead on the Stellar networks page. After a reset the
network starts again from ledger 1, under the same passphrase. Nothing in the
`testnet` database survives it: every table is keyed on the ledger sequence,
so the new chain's ledger N would overwrite the old one's.

The same steps build the database the first time (steps 4–6).

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

**2. Pause the testnet indexer:** `indexerLambdaConcurrency: 0` in
`infra/envs/testnet.json`, then `make -C infra deploy-testnet`. The doorbells
keep queueing and are harmless.

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

**6. Backfill from genesis** with the operator write cert
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
The indexer continues from `max(sequence) + 1`.

## Done when

- `testnet-ingestion-stall` is back to OK;
- `SELECT max(sequence) FROM testnet.ledgers` follows the RPC tip within
  seconds;
- one recent transaction resolves on the testnet site under the hash testnet
  RPC reports for it.
