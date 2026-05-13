# backfill-enrichment-runner — status

**Timestamp:** 2026-05-13T11:18:29.090896+00:00

## `sep1-assets` kind (assets table)

| column                       | NULL (pending) | `''` (sentinel) |
| ---------------------------- | -------------: | --------------: |
| `assets.icon_url`            |          12900 |               0 |
| `assets.name` (type IN 1, 2) |          12899 |               0 |

**Total assets rows:** 12900

## `nft-metadata` kind (nfts table)

| column                 | NULL (pending) | `''` (sentinel) |
| ---------------------- | -------------: | --------------: |
| `nfts.name`            |        1088584 |               0 |
| `nfts.media_url`       |        1088584 |               0 |
| `nfts.collection_name` |        1088584 |               0 |

**Total nfts rows:** 1088584

## `lp-analytics` kind (liquidity_pool_snapshots) — DEFERRED to 0199

| column                                 | NULL (pending) |  total |
| -------------------------------------- | -------------: | -----: |
| `liquidity_pool_snapshots.tvl`         |         392256 | 392256 |
| `liquidity_pool_snapshots.volume`      |         392256 | 392256 |
| `liquidity_pool_snapshots.fee_revenue` |         392256 | 392256 |

**Total liquidity_pool_snapshots rows:** 392256

> `enrich` has no `lp-analytics` subcommand today — population is owned by task 0199 (LP analytics: TVL + volume + fee_revenue), blocked on the team-built price API. NULL counts above will stay at 100% until 0199 ships its Lambda 2 path.
