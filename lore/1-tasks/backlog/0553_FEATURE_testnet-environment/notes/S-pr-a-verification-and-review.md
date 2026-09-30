# PR A: local verification and post-merge review (task 0553)

### PR A built and verified locally — 2026-09-29

PR #543, branch `feat/0553-ledger-source-config`. Env knobs, each defaulting
to today's production value: `CLICKHOUSE_DATABASE` (all three Lambdas — they
had `default` hardcoded as `PROD_DATABASE`, missed by the list above; the env
var only reached the CLIs) and `PUBLIC_ARCHIVE_PREFIX` (API, backfill-runner,
indexer). The indexer reads the public dataset unsigned in `us-east-2` when
`BUCKET_NAME` names it — a request signed by the Lambda role, which holds no
grant there, is refused (review finding; the first cut had this wrong). API,
indexer and `backfill-runner run` refuse to start when the ledger folder's
network (`pubnet` / `testnet` segment) disagrees with
`STELLAR_NETWORK_PASSPHRASE` — ledger meta carries no network, so a mismatch
would otherwise hash silently wrong. `snapshot-seed`, which reads the mainnet
history archive and writes, refuses any other network. The history-archive URL
and the snapshot ledger floor stay mainnet-only: testnet starts from genesis
and never seeds.

Local run (repo `timeouts.xml` mounted; a container without it fails with
`channel closed`, the documented 30 s `http_receive_timeout` trap):
2,000 testnet ledgers (4,862,000–4,863,999, 33,746 transactions) into a
`testnet` database. Ledger hash of 4,863,000 and 5/5 transaction hashes,
application orders and statuses equal testnet RPC. The native-XLM SAC our code
derives from the passphrase is the contract emitting 66,978 events in the
window — the mainnet derivation would name a contract that emits none there.
The three refreshable MVs bind to `testnet.*`. The guard refuses a mainnet
passphrase with the testnet folder.

### PR A, post-merge devil's-advocate review — 2026-09-29

Verdict: no production risk (every Lambda gets the exact mainnet passphrase;
nothing sets `PUBLIC_ARCHIVE_PREFIX` or `CLICKHOUSE_DATABASE`). To settle in
PR D before testnet goes live:

- **The genesis partition would be skipped silently.** The lake's testnet
  folder starts at ledger 2 (0 and 1 answer 404), so `FFFFFFFF--0-63999`
  holds at most 63,998 files, and `sync_partition` wants exactly 64,000
  (`backfill-runner/src/sync.rs:198,213`): `S3Incomplete`, a `warn`, and
  ledgers 2–63,999 never ingested. The count check must start at the
  network's first ledger.
- **The network guard does not detect a reset.** It accepts any
  `testnet/<date>` folder; an old folder just stops growing and the indexer
  logs at `info`. Only the stall alarm catches it, and the prefix must never
  move without dropping the database first.
- **A prefix set with a non-lake bucket is ignored silently** (the indexer
  applies it only when `BUCKET_NAME` is the lake). Refuse that combination
  at start-up.

Small follow-ups: the API guard compares the trimmed passphrase but hashes
the raw one — trim once; nothing stops a testnet Lambda from calling the
mainnet RPC default — extend the guard to the RPC list.
