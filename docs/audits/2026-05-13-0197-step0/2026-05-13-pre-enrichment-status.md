    Finished `release` profile [optimized] target(s) in 0.36s
     Running `target/release/enrich status`

# backfill-enrichment-runner — status

**Timestamp:** 2026-05-13T07:33:29.932230+00:00

## `sep1-assets` kind (assets table)

| column                       | NULL (pending) | `''` (sentinel) |
| ---------------------------- | -------------: | --------------: |
| `assets.icon_url`            |          10369 |               0 |
| `assets.name` (type IN 1, 2) |          10368 |               0 |

**Total assets rows:** 10369

## `nft-metadata` kind (nfts table)

| column                 | NULL (pending) | `''` (sentinel) |
| ---------------------- | -------------: | --------------: |
| `nfts.name`            |         587067 |               0 |
| `nfts.media_url`       |         587067 |               0 |
| `nfts.collection_name` |         587067 |               0 |

**Total nfts rows:** 587067

## `lp-analytics` kind (liquidity_pool_snapshots) — DEFERRED to 0199

| column                                 | NULL (pending) |  total |
| -------------------------------------- | -------------: | -----: |
| `liquidity_pool_snapshots.tvl`         |         197065 | 197065 |
| `liquidity_pool_snapshots.volume`      |         197065 | 197065 |
| `liquidity_pool_snapshots.fee_revenue` |         197065 | 197065 |

**Total liquidity_pool_snapshots rows:** 197065

> `enrich` has no `lp-analytics` subcommand today — population is owned by task 0199 (LP analytics: TVL + volume + fee_revenue), blocked on the team-built price API. NULL counts above will stay at 100% until 0199 ships its Lambda 2 path.
