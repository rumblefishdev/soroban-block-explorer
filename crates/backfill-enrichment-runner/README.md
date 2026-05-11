# `backfill-enrichment-runner`

Local CLI that drains pre-existing un-enriched DB rows by calling the
same `enrichment_shared::enrich_and_persist::*` functions the live
SQS-driven [`enrichment-worker`](../enrichment-worker) Lambda invokes
per message.

The live worker is **forward-only** — it processes rows the indexer
emits onto SQS, never anything that pre-dates the queue's deployment.
For every enrichment kind there is a population gap covering the
historical region, and this binary is what drains it.

Mirror of `backfill-runner`'s single-bin shape: one `enrich` executable
with subcommands per kind plus a `status` aggregator.

## Subcommands

| Subcommand     | Kind             | Function called                                   | Columns written                            |
| -------------- | ---------------- | ------------------------------------------------- | ------------------------------------------ |
| `icon`         | `Icon`           | `enrich_and_persist::sep1_assets::enrich_asset_from_sep1` | `assets.icon_url`, `assets.name`           |
| `nft-metadata` | `NftTokenUri`    | `enrich_and_persist::nft_token_uri::enrich_nft_token_uri` | `nfts.name`, `nfts.media_url`, `nfts.collection_name` |
| `status`       | (read-only)      | inline `COUNT(*) FILTER` query                    | none — prints a Markdown table             |

### Gate: `nft-metadata` and 0195 Phase E

`NftTokenUriFetcher::resolve` is `unimplemented!()` until task 0195
Phase E lands the Soroban-RPC + IPFS fetcher. The subcommand will
compile and ship, but **will panic at runtime** the moment a real row
is processed before Phase E. That panic is intentional per spec —
visible failure beats silent NULL writes. The split-PR option in task
0196 anticipates this: `icon` may land first; `nft-metadata` follows
once 0195 Phase E ships.

## Standard filter vs `--force-retry`

The default SELECT mirrors the **live worker's producer SQL**, so the
drain only touches rows the worker would itself have re-emitted:

| Subcommand     | Standard filter                                                                       |
| -------------- | ------------------------------------------------------------------------------------- |
| `icon`         | `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`                   |
| `nft-metadata` | `WHERE name IS NULL AND media_url IS NULL AND collection_name IS NULL`                |

The empty-string sentinel `''` written on permanent fetch failure
breaks these `OR` clauses on subsequent passes — without it, the drain
(like the producer) would infinitely re-publish rows the upstream
permanently can't satisfy.

`--force-retry` drops the standard filter entirely and walks every row
in the target table (γ-semantics per task 0196):

- `enrich_*` functions are idempotent.
- `UPDATE` uses `COALESCE(NULLIF($n, ''), col, $n)` priority
  `real > sentinel > NULL`: a sentinel never clobbers a real value, a
  real value upgrades a sentinel, NULLs fill from either.
- Real → sentinel transitions only happen on legitimate
  re-classification (issuer removed the field upstream).

Use it after upstream fixes (issuer republishes a corrected TOML, an
RPC outage clears, an IPFS gateway recovers) to refresh rows the live
worker classified as permanent-fail before the fix.

## Usage

The runner reads `DATABASE_URL` from the environment (or `--database-url`
explicitly). Pool size is 4 — enough for the chunked SELECT plus the
per-task UPDATEs from the bounded-concurrency fan-out.

```bash
# Default drain — `icon` kind, standard filter
DATABASE_URL=postgres://postgres:postgres@localhost:5432/soroban_block_explorer \
  cargo run -p backfill-enrichment-runner -- icon

# Tuned concurrency / chunk size
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- \
  icon --concurrency 20 --chunk-size 500

# Test mode — cap at 10 rows
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- icon --limit 10

# Surgical mode — single row by id
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- icon --id 4242

# γ-overwrite — re-walk every assets row, ignore sentinels
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- icon --force-retry

# NFT drain (post-0195 Phase E)
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- nft-metadata

# Aggregate status across kinds — cheap point-in-time query
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- status
```

Operator-chainable: exit code is `0` on a clean run (every processed
row reached a terminal outcome — real value or sentinel), `1` on any
transient failure or DB error. So
`enrich icon && enrich nft-metadata` short-circuits cleanly
(shorthand for
`cargo run -p backfill-enrichment-runner -- icon && cargo run -p backfill-enrichment-runner -- nft-metadata`,
or define a shell alias `alias enrich='cargo run -p backfill-enrichment-runner --'`).

## Output

Each drain prints a Markdown report at the end (mirrors
`audit-harness`'s "summary block + sample failures" shape):

```
# backfill-enrichment-runner — icon drain report

**Timestamp:** 2026-05-11T08:42:00+00:00
**Processed:** 42193
**Succeeded (incl. sentinel writes):** 42183
**Unreachable (transient, retry candidate):** 10
**DB failures:** 0
**Duration:** 1684300 ms

## Sample transient errors (first 10)
| id | error |
| --- | --- |
| 12 | error sending request for url (https://example.com/.well-known/stellar.toml): connect timeout |
...
```

`succeeded` lumps real-value writes together with permanent-fail
sentinel writes — both are healthy terminal outcomes from the drain's
perspective. The only way to distinguish post-hoc is `enrich status`
against the column.

## Ops runbook

**When to run:**

1. **One-shot after deploying 0191** (icon kind): once the live worker
   is up, the standing population of un-enriched assets is the
   drain's entire payload. Expect ~30 min wall clock for 50K rows on
   a local laptop at `--concurrency 10`.
2. **One-shot after 0195 Phase E ships** (NFT kind): same shape, but
   typically smaller payload (NFT mints are a minority of writes).
3. **After targeted upstream fixes**: an issuer fixes their TOML, an
   IPFS gateway comes back — `--force-retry` re-walks affected rows.
4. **Never on a cron** — backfill is operator-on-demand, not a
   periodic job. If a kind ever needs periodic refresh, the right
   answer is a dedicated cron Lambda, separate from this CLI (see
   task 0196 Future Work).

**Pre-flight checklist:**

```bash
# 1. Sanity — DB reachable, schema present
DATABASE_URL=... cargo run -p backfill-enrichment-runner -- status

# 2. Smoke — drain 10 rows, watch the log
DATABASE_URL=... RUST_LOG=info cargo run -p backfill-enrichment-runner -- icon --limit 10

# 3. Full drain — leave running; cargo run inherits a non-fd-locking stdin
DATABASE_URL=... cargo run --release -p backfill-enrichment-runner -- icon
```

**Watch:** `tracing` events at `info` level for per-chunk progress
(`info!(asset_id, ...)` inside `enrich_asset_from_sep1`). `error`
events fire on DB failures — these are real and should be
investigated before another drain.

**Halt condition:** the runner self-terminates with non-zero exit if
`db_failed > 0`. A sustained transient-failure rate (e.g. > 50% of
chunk reported `unreachable`) is the operator's signal to abort
(`Ctrl-C`) and check the upstream service before continuing.

## Benchmark target

50K `assets` icon backfill, local laptop (M-series Mac), Postgres
running in Docker on the same host:

- `--concurrency 10 --chunk-size 200`
- Wall clock target: **< 30 min**
- Bound by SEP-1 issuer-host latency (typical 100-500 ms per fetch),
  amortised by `Sep1Fetcher`'s in-process LRU (an issuer with 100
  assets pays the SEP-1 cost once).

If your run substantially exceeds this, the typical culprits are (a)
the `Sep1Fetcher` cache not surviving (multi-process invocation
defeats the LRU) or (b) chunk size too small for the row width.

## Architecture notes

- **Why a new crate, not a `backfill-runner` subcommand:**
  `backfill-runner` re-ingests Stellar ledgers from XDR archives via
  Galexie. Enrichment backfill drains pre-existing DB rows by calling
  external enrichment APIs. Different concerns, different data
  sources, different operational profiles. Task 0191 design decision
  #8 was emphatic that the ledger-backfill code path must not be
  modified; a separate crate guarantees that.
- **Why no SQS path:** a 50K-row queue publish would hit SQS rate
  limits, and per-message visibility-timeout / delete-after-ack
  overhead wastes time when we already hold a DB connection. Direct
  call into `enrichment-shared` also proves a clean library boundary.
- **Why `Semaphore` + `tokio::spawn` (mirror of `audit-harness`)
  instead of `buffer_unordered`:** each task gets its own retry / log
  scope, and the Semaphore permit is dropped exactly when the task
  finishes — `buffer_unordered` couples the permit lifetime to the
  stream poll order, which is subtly different under cancellation.

## Related

- Task spec: [`lore/1-tasks/active/0196_FEATURE_enrichment-backfill-crate.md`](../../lore/1-tasks/active/0196_FEATURE_enrichment-backfill-crate.md)
- Field allocation rule: [`lore/2-adrs/0043_field-allocation-rule.md`](../../lore/2-adrs/0043_field-allocation-rule.md)
- Live worker: [`crates/enrichment-worker`](../enrichment-worker)
- Shared HTTP / fetch primitives: [`crates/enrichment-shared`](../enrichment-shared)
- Sibling backfill (ledgers, not enrichment): [`crates/backfill-runner`](../backfill-runner)
