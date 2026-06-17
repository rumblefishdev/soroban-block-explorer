# Runbook — local enrichment backfill smoke (lore 0231)

Drive `backfill-enrichment-runner` end-to-end against a **local** ClickHouse with
a curated seed, to validate the loop (chunks / cursor / pagination / fan-out)
and every reproducible enrichment outcome (real / sentinel) before a prod drain
(Step 7). Manual / operator-run — **not CI** (needs live CH + network).

> Prod drain is a different thing: same binary, mTLS to Hetzner CH, real data at
> scale. This runbook is the **local correctness** check.

## Prerequisites

- Local ClickHouse up with the schema applied (the docker-compose `clickhouse`
  service; `init.sql` creates `assets` / `accounts` / `nfts` /
  `soroban_contracts` / `asset_enrichment` / `nft_enrichment`).
- Network egress (the real path fetches issuer `stellar.toml` + mainnet
  Soroban-RPC).
- Connection env:

  ```bash
  export CLICKHOUSE_URL=http://localhost:8125 \
         CLICKHOUSE_USER=default \
         CLICKHOUSE_PASSWORD=clickhouse \
         CLICKHOUSE_DATABASE=default
  ```

## 1. Seed

```bash
docker exec -i soroban-block-explorer-clickhouse-1 \
  clickhouse-client --password clickhouse --multiquery \
  < scripts/enrichment-backfill-seed.sql
```

Seeds **50 sep1 candidates** (`asset_type IN (1,2)`) + 2 excluded (native /
soroban) + **50 nft candidates**. Re-runnable (clears its own `900000..999999`
id range first). Outcome mix (see the SQL header): 9 real across 5 issuers/domains (USDC
centre.io · AQUA + ICE/dICE/governICE/gdICE aqua.network · EURC mykobo.co ·
PEN + ARS anclap.com), 1 each of no-home-domain / domain-404 / no-CURRENCIES /
unsafe-hostname, the rest missing-issuer fillers (fetch-free) to fill the page
count.

Verify the seed:

```bash
# expect: 50 candidates, 0 tried, 50 untried (both tables)
cargo run -q -p backfill-enrichment-runner -- status
```

## 2. Drain sep1

```bash
cargo run -q -p backfill-enrichment-runner -- sep1-assets --chunk-size 5 --concurrency 10
echo "exit=$?"   # expect 0
```

`--chunk-size 5` over 50 candidates → **10 keyset pages** (cursor advance);
`--concurrency 10` → fan-out (logs land in waves, one per page). Observed report:

```
# enrich sep1-assets — drain report
**Processed:** 50
**Real values:** 9
**Sentinels (`''` tried-nothing):** 41
**Unreachable (transient, retry candidate):** 0
**DB failures:** 0
**Duration:** ~3 s
✓ All processed keys reached a terminal outcome.
```

`Real` / `Sentinel` are split (not folded into one "succeeded") — matching the
`status` real/sentinel breakdown.

## 3. Drain nft

```bash
cargo run -q -p backfill-enrichment-runner -- nft-metadata --chunk-size 5 --concurrency 10
```

Observed: `Processed 50 · Real 1 · Sentinel 49 · Transient 0 · exit 0`. The seed
covers every reproducible NFT outcome:

- **1 real (partial)** — `CDA5FGE4…` (the JamesBachini tutorial contract, 0-arg
  `token_uri`) → `name = "SorobanNFT"` **+ `media_url`** (the JSON has no `image`
  field but carries the image CID under `url`; the resolver's `image`→`url`
  fallback recovers it → a real `https://…/ipfs/Qme…` 1200×1200 PNG, no extra
  RPC). `collection_name` stays `''`, so this row is a **partial** (name+media
  real, collection empty) — `status` shows it as `1 partial`. The **only**
  verified mainnet Soroban NFT — Soroban NFTs are sparse, so one real, not nine.
- **3 RPC-fail → sentinel** — contracts present in `soroban_contracts` (lookup
  hits) whose `token_uri` RPC errors: native XLM SAC `CAS3J7GY…` (×2 tokens,
  `Value/InvalidInput` "symbol not found") + game contract `CDL74RF5…`
  (`WasmVm/MissingValue` "non-existent contract function"). All classified
  **permanent** → sentinel. (The `CDL74RF5` case was a transient mis-classify
  until the `is_transient` fix this branch — see the note below.)
- **46 lookup-miss → sentinel** — contract absent from `soroban_contracts` →
  sentinel **before** any RPC (fetch-free).

## 4. Status (after) + spot-check

```bash
cargo run -q -p backfill-enrichment-runner -- status
```

```
## `asset_enrichment`   candidates: 50 | rows (tried): 50 | untried: 0
| column     | real | '' sentinel |
| icon_url   |    9 |          41 |
| name       |    9 |          41 |
rows: 9 all-real · 0 partial · 41 all-sentinel
## `nft_enrichment`     candidates: 50 | rows (tried): 50 | untried: 0
| name            | 1 | 49 |
| media_url       | 1 | 49 |
| collection_name | 0 | 50 |
rows: 0 all-real · 1 partial · 49 all-sentinel
```

Spot-check the real values (read-path neutralises `''` with `NULLIF`):

```bash
curl -s "http://default:clickhouse@localhost:8125/" --data-binary \
 "SELECT asset_code, nullIf(icon_url,'') icon, nullIf(name,'') name \
  FROM asset_enrichment FINAL WHERE issuer_id BETWEEN 900001 AND 900013 \
  ORDER BY asset_code FORMAT TSVWithNames"
# USDC      https://www.centre.io/images/usdc/usdc-icon-…png  USD Coin
# AQUA/ICE/dICE/gdICE/governICE  https://aqua.network/...      AQUA / ICE / …
# EURC      https://mykobo.co/assets/img/eurc_icon_128.png     EURo Coin
# PEN       https://static.anclap.com/coin/pen.png             Sol Digital
# ARS       https://static.anclap.com/coin/ars.png             Peso Digital
# D404 / NOHOME / NOTREAL / TMOUT  → \N \N  (sentinel)
```

## 5. `--force-retry` + `--limit`

```bash
# re-enrich (drops the NOT-IN candidate filter); --limit stops early
cargo run -q -p backfill-enrichment-runner -- sep1-assets --force-retry --limit 8 --chunk-size 5
```

Observed: `Processed 8` (limit honoured) `· Real 1 · Sentinel 7` — re-processes
already-tried keys (idempotent, newer-`version` INSERT, latest-wins).

## 6. `status` vs `report` — they answer different questions

- **report** = _this run's dynamics_: processed / real / sentinel / transient /
  db-failed / duration (+ a bounded sample of transient errors).
- **status** = _standing coverage_: candidates / tried / untried + the
  per-column real-vs-sentinel split. Independent of any run.

Run a drain → read the `report`; run `status` before+after → read the coverage
delta. (As of lore 0231 the report no longer folds real+sentinel into one count,
so its vocabulary matches `status`.)

## 7. Cleanup

```bash
docker exec -i soroban-block-explorer-clickhouse-1 clickhouse-client --password clickhouse --multiquery <<'SQL'
ALTER TABLE accounts          DELETE WHERE id         BETWEEN 900000 AND 999999;
ALTER TABLE assets            DELETE WHERE issuer_id  BETWEEN 900000 AND 999999;
ALTER TABLE asset_enrichment  DELETE WHERE issuer_id  BETWEEN 900000 AND 999999;
ALTER TABLE soroban_contracts DELETE WHERE id         BETWEEN 900000 AND 999999;
ALTER TABLE nfts              DELETE WHERE contract_id BETWEEN 900000 AND 999999;
ALTER TABLE nft_enrichment    DELETE WHERE contract_id BETWEEN 900000 AND 999999;
SQL
```

## Known gaps (covered by unit tests, not this local e2e)

These outcomes can't be forced deterministically against a **local CH + real
fetcher**, so they stay unit-tested (resolver / `is_transient` mock-input tests),
not exercised here:

| outcome                                                                | why not reproducible locally                                                                                                                                                                                                                                     |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **transient** (`EnrichError::Transient` → no row → retry)              | the fetcher's SSRF guard rejects the easy `localhost:1` trick as a **permanent** unsafe-hostname sentinel; a real timing-out public host isn't deterministic. NFT transient is worse — the RPC URL is global to the fetcher, so one key can't be made transient. |
| **B** real-partial (image present, name absent → icon real, name `''`) | no real issuer found whose TOML carries an image but no name (all 9 seeded reals incl the ICE family carry images → full real). Would need a controlled TOML host.                                                                                               |
| **C** unsafe `http://` image → icon `''`                               | couldn't find a stable real issuer serving an `http://` image. The `https://`-only guard (also blocks `javascript:` / `data:` XSS) is **not** relaxed to accommodate one — a real `http://` issuer is correctly sentinel'd.                                      |
| **D** oversize value → `''`                                            | no real TOML carries a 4 KB+ name / 8 KB+ URL.                                                                                                                                                                                                                   |

Forcing B/C/D deterministically would need a local TOML host + a fetcher base-URL
override (the fetcher hard-codes `https://{home_domain}`) — judged
over-engineering for guards already unit-tested.
