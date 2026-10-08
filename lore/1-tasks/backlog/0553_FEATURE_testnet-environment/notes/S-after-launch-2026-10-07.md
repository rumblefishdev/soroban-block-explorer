# After the launch — 2026-10-06 / 08

What broke or surprised once testnet ran live, and what was done. Times UTC.

## Indexer caught up, alarms verified

- The indexer reached the tip on 2026-10-06 ~19:39 (~10.5 h of catch-up at
  ~1.7 ledgers/s from 4,992,000); `testnet-ingestion-stall` went OK then.
- **Stall alarm, end to end:** it fired on a real stall (below) and on a
  forced `set-alarm-state` test; SNS delivered to the chat integration and the
  test message reached the testnet Slack channel. The chain works; a missed
  page is a channel-notification setting, not delivery.

## A deploy without its schema step stopped ingestion (2026-10-07 07:59)

Testnet was deployed from `develop` carrying the rename of
`wasm_interface_metadata` to `wasm_programs` (#618) while the `testnet`
database still had the old name. The first new WASM program failed every
insert (`Code: 60`, unknown table) from 08:01:46 until the rename and the
`code` column were applied by hand (~08:10). Lesson: a testnet deploy runs
the same pre-deploy schema steps as production (task 0603, `/release`
checklist). The 2026-10-08 deploy created `contract_instances` (#641) first
and kept ingesting.

## The self-paced chain was cut every ~80 s — fixed by #634

Lambda's recursive-loop detection counted the indexer's own delayed SQS
messages as a loop and dropped the 17th hop: `RecursiveInvocationsDropped`
= 300/h, SQS `Received − Invocations` = 300/h, ~30 messages/h into the DLQ,
ingestion pausing ~40 s every 2 min. #634 allows the loop on the lake-reading
indexer only and adds `<env>-indexer-runaway-wakeups` (wake-up messages sent,
not invocations — a long catch-up drains piled-up keepalives in a burst).
Production's synth stayed byte-identical. The from-scratch shape (scheduler
invokes the indexer directly, no chain) is left to 0603.

## Testnet NFT metadata came back empty — fixed by #632

The enrichment worker had no `SOROBAN_RPC_URLS` and fell back to a mainnet
list in code; every testnet NFT got an empty sentinel (mainnet answered
"non-existing contract instance"). #632 removed the in-code default (missing
list = startup error) and gave the worker its network's list. After the
2026-10-08 deploy the same contract answers from testnet (`WasmVm,
MissingValue` — the token has no metadata), so sentinels are now true.
#633 lets the `enrich` CLI reach the production ClickHouse over mTLS like
`backfill-runner`; run it with the testnet enrichment certificate, whose user
`testnet_writer` cannot write outside `testnet.*`. Not yet run: the old
sentinels and the ~4% SEP-1 asset coverage stay until it is.

## Edge

The Cloudflare Free Managed Ruleset now covers both API hosts (zone repo PR
#4, 2026-10-07): two days on testnet blocked only a React RCE probe and our
own Log4j-shaped tests. A WAF block never reaches AWS, so no alarm of ours
sees a false positive (task 0624). The testnet SPA links the Prices API on
mainnet (#631); the portal ships with mainnet only.
