# 0197 audit Step 0 — fresh run summary

**Date:** 2026-05-13 (fresh DB reset run)
**Range:** ledgers 50944000..51007999 (full partition FCF6A7FF, 64000 ledgers)
**Captured at:** 2026-05-13T11:19:13Z

## Artifacts in this directory

| File                                                  | Content                                            |
| ----------------------------------------------------- | -------------------------------------------------- |
| `2026-05-13-fresh-backfill-full-partition-run.log`    | Full backfill stdout/stderr                        |
| `2026-05-13-fresh-post-indexing-diversity.txt`        | Row counts per audited table                       |
| `2026-05-13-fresh-pre-enrichment-indexing-status.txt` | `backfill-runner status` output                    |
| `2026-05-13-fresh-pre-enrichment-status.md`           | `backfill-enrichment-runner status` PRE asset seed |
| `2026-05-13-fresh-asset-seed-workaround.txt`          | Bug #1 workaround SQL + result counts              |
| `2026-05-13-fresh-pre-drain-post-seed-status.md`      | enrichment status AFTER seed, BEFORE drain         |
| `2026-05-13-fresh-sep1-drain.log`                     | sep1-assets drain report                           |
| `2026-05-13-fresh-nft-drain-limit1000.log`            | nft-metadata --limit 1000 drain report             |
| `2026-05-13-fresh-post-enrichment-status.md`          | Final enrichment status                            |
| `2026-05-13-fresh-sample-real-sep1-enrichments.txt`   | First 20 assets with real icon_url                 |
| `2026-05-13-fresh-top-nft-contracts.txt`              | Top 20 contracts by NFT row count                  |
| `2026-05-13-fresh-sample-real-nft-enrichments.txt`    | First 20 nfts with real name                       |

## Key counts

```
        table_name        |   rows   | min_seq  | max_seq
--------------------------+----------+----------+----------
 assets                   |        7 |          |
 liquidity_pools          |    11985 |          |
 soroban_contracts        |    11926 |          |
 ledgers                  |    23332 | 50944000 | 50967331
 accounts                 |   180612 |          |
 account_balances_current |   329367 |          |
 liquidity_pool_snapshots |   392256 |          |
 nfts                     |  1088584 |          |
 transactions             |  8683448 |          |
 operations_appearances   | 10276654 |          |
(10 rows)
```

## sep1-assets drain headline

```
**Processed:** 12900
**Succeeded (incl. sentinel writes):** 10741
**Unreachable (transient, retry candidate):** 2159
**DB failures:** 0
**Duration:** 40225 ms
```

## nft-metadata drain headline

```
**Processed:** 1000
**Succeeded (incl. sentinel writes):** 931
**Unreachable (transient, retry candidate):** 69
**DB failures:** 0
**Duration:** 2348 ms
```

## Cross-reference

See pre-audit findings in this directory:

- `2026-05-13-pre-audit-finding-classic-credit-asset-row-missing.md` (Bug #1)
- `2026-05-13-pre-audit-finding-home-domain-backfill-gap.md` (Bug #2)
- `2026-05-13-pre-audit-finding-nft-false-positives-looks-like-token-id.md` (Bug #3, includes Bug #6 fix verification)
- `2026-05-13-pre-audit-finding-sac-detection-misses-pre-existing-contracts.md` (Bug #4)
- `2026-05-13-pre-audit-finding-token-uri-signature-mismatch.md` (Bug #5, fix verified)
- `2026-05-13-audit-friction-notes.md` (DEVEX bundle)

## Post-fix verification (added 2026-05-13)

Two worker-side bugs (#5 and #6) were patched and verified end-to-end
on the same DB without re-ingesting:

### Bug #5 — `token_uri(token_id)` signature mismatch

Patch in `crates/enrichment-shared/src/nft_token_uri/client.rs`:
`build_simulate_envelope` now takes `Option<u32>` for the token_id;
`fetch_uncached` tries SEP-50 path first and falls back to SEP-39
zero-arg path on `MismatchingParameterLen`.

Manually seeded fixture: James Bachini's `SorobanNFT` contract
`CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`
inserted into `soroban_contracts` (id 1706144) + `nfts` (id 3146763,
token_id 1).

| Run                            | Outcome                                              | `nfts.name`      |
| ------------------------------ | ---------------------------------------------------- | ---------------- |
| PRE-fix `enrich --id 3146763`  | 0 succeeded, 1 transient (`MismatchingParameterLen`) | NULL             |
| POST-fix `enrich --id 3146763` | **1 succeeded, 0 transient**                         | **"SorobanNFT"** |

End-to-end RPC + IPFS + JSON parse + DB write verified live.

### Bug #6 — `is_transient` misclassifies permanent errors

Patch in `crates/enrichment-shared/src/nft_token_uri/errors.rs`:
`SorobanRpc(_)` now discriminates by message — patterns
`MismatchingParameterLen`, `"symbol not found in slice of strs"`,
`Error(WasmVm, UnexpectedSize)`, `Error(Storage, MissingValue)`
route to permanent (sentinel write); everything else stays transient
(default). Three unit tests added.

Combined with #5, the worker no longer burns SQS retry budget on
false-positive NFTs or signature mismatches — they go straight to
sentinel.

### Measurement (`--force-retry --limit 1000` on the same 1000-row sample)

| Metric                | PRE-fix | After #5 + #6 (initial) | After + `Storage, MissingValue` |
| --------------------- | ------- | ----------------------- | ------------------------------- |
| Succeeded             | 931     | 954                     | **1000** ✅                     |
| Transient (SQS retry) | 69      | 46                      | **0** ✅                        |
| DB failures           | 0       | 0                       | 0                               |
| Duration              | 2348 ms | 2256 ms                 | 2204 ms                         |

Every row reaches a terminal outcome in iteration 3. At full
~1M-fake-NFT scale this saves roughly 70 000 retry cycles per drain.

### Status command diff after fix

| Column                 | PRE NULL / sentinel | POST NULL / sentinel | populated                           |
| ---------------------- | ------------------- | -------------------- | ----------------------------------- |
| `nfts.name`            | 1 087 653 / 931     | 1 087 652 / 931      | **0 → 1** ✅                        |
| `nfts.media_url`       | 1 087 653 / 931     | 1 087 652 / 932      | 0 → 0 (Bachini JSON has no `image`) |
| `nfts.collection_name` | 1 087 653 / 931     | 1 087 652 / 932      | 0 → 0 (no `collection`)             |

Total unchanged: 1 088 584 rows.

### Bugs #1-#4 not test-able without re-ingest

Bug #1 (classic-credit assets producer), #3 (`looks_like_token_id`),
#2/#4 (initial-state RPC enrichment) are indexer-side fixes — they
mutate ingest behaviour and therefore need a fresh ingest run to
verify. Their fix paths are documented in their respective finding
docs but not implemented here. The SEP-1 audit already used a manual
SQL workaround for Bug #1 (`INSERT INTO assets FROM
account_balances_current`) which produced 603 real `icon_url` writes
out of 12 900 seeded classic credits (4.7%).
