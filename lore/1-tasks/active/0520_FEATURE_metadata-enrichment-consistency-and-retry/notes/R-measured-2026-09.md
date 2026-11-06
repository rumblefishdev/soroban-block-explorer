# Measured 2026-09-15/16 (production, read-only)

**Defect 1 — one decision, two rules.** The parser treats any `mint(address,
number)` as an NFT candidate: it cannot tell a token id from an amount without
the contract's class. Staging decides with the class (`stage.rs` `route_for`:
Token/Fungible → drop, Nft → `nfts`, other → `nfts_pending`, since task 0220).
The enrichment producer (task 0231, a month later) queues the parser's raw
candidates (`handler/mod.rs`, `batch_minted_nfts`) instead of what staging
stored. Result: 53 239 `nft_enrichment` keys from Fungible contracts, none in
`nfts`; ~5 200 `sentinel written` warnings/week, ~4 400 of them
`token_uri` calls on contracts without that function (estimate from log lines).

**Defect 2 — what a retry recovers.** Every empty row of a real NFT (2 916 of
13 980 displayed: 2 055 all-empty, 861 with only a collection name) was
re-fetched on a stratified sample of 558; every empty asset row (343 271,
15 361 domains) in full, reproducing the worker's rules.

|        | Empty today | Recovered by a retry now | Never fixable by retry                                                   |
| ------ | ----------- | ------------------------ | ------------------------------------------------------------------------ |
| NFTs   | 2 916       | ~22 (0.8%, extrapolated) | 94% — contract reverts (40%), empty `token_uri` (34%), `data:` URI (19%) |
| Assets | 343 271     | 3 299 (1%)               | 78% no home domain, 13% dead DNS                                         |

The recoverable failures were attempted 12–36 s after the NFT was minted or
the asset first appeared; siblings minted minutes later were enriched fine —
metadata published late. Going forward ~70 assets and ~2 NFTs per week
(estimate). A 2026-07-02 manual `--retry-sentinels` pass recovered 809 of
276 584 asset rows (0.29%); that flag skips the 861 half-empty NFT rows.

**Defect 3.** All 90 sampled `data:` URIs decode to JSON with a name (~556
NFTs). Both configured IPFS gateways answered 429 to every request (observed
from a workstation, not from the Lambda — unverified there); stored
`media_url` values on the same gateway are at risk.

Also found: RPC "preflight queue full" is not on the transient list (an
overloaded provider writes permanent empties); the SEP-1 fetch gives up at 2 s
(339 assets answer only within 15 s).

## A4 in full (moved from the task file)

**A4 — stop warning about a key that is not token metadata.** `is_metadata_key`
(`token_metadata.rs`) accepts `Symbol("METADATA")` and the OZ NFT
`Vec([Symbol("Metadata")])`; when neither yields a name, symbol or decimals,
`ledger_entry_changes.rs` warns "non-standard shape, skipped". Measured over 7
days of production logs: 4 504 such lines, all 33 contracts concentrated-liquidity
pools whose `Metadata` slot holds oracle bookkeeping (`checkpoint_count`,
`last_checkpoint_index`, `last_observation_ledger`) — read from chain, none has
`Symbol("METADATA")`, none is a token, none has a `soroban_contract_metadata`
row, so no name is lost. They log on every instance write, twice (the discarded
"before" image is decoded too), and one pool produces 78% of the lines. Accept
the NFT-enum key as metadata only when its map carries `name` or `symbol`;
leave `Symbol("METADATA")` untouched, and skip decoding the pre-image. The
warning then means what it was written for: a real token whose metadata we
cannot read.

## Gateway replacement, measured 2026-11-04/05

The gateway half of defect 3 reached the enrichment DLQ. Logs, 2026-10-21 to
2026-11-04: messages with three failed tries equal the queue depth on both
networks. Mainnet 30 of 30 are IPFS fetches; testnet 46 of 103 (the other 57 are
5xx from two issuer domains, `stellar.toml`).

- `ipfs.io`: 162 of 162 logged errors in 30 days are 429. Source:
  gatewaychanges.ipfs.io (retired 2026-09-21), blog.ipfs.tech/2026-08-beyond-sponsored-gateways.
- `gateway.pinata.cloud`, from a workstation: 4-7 s per answer against a 5 s
  budget, then 429 on 5 of 5. 88 worker tries end in `error sending request`;
  the log drops the underlying cause.
- `ipfs.filebase.io`, from a workstation, one request a second: 16 of 17 metadata
  files from the failing keys (p50 0.23 s); the 17th, `QmU1sB…/0`, times out on
  Pinata too. 44 of 44 distinct stored images (p50 1.3 s, max 3.7 s, all
  `image/png`). Not measured from a Lambda.

Filebase's page: "intended for testing and light usage", 200 requests a minute,
"For production traffic, use a dedicated gateway".

Stored `media_url` on `ipfs.io`: 217 rows on mainnet on 2026-11-05 and growing until
the worker deploys, 165 on testnet. The image address is baked into the row, so
after the worker deploys these need a rewrite, testnet first: `INSERT ... SELECT`
with `replaceOne` on the prefix and `version = now64(3)`. A `token_uri` that names
`https://ipfs.io/...` literally has one candidate and still fails (one known
contract, `CDA5FGE4...`, one empty row).

A gateway's refusal can still store an empty row for good. Traced in code, not
seen live: a 403 or a connect error from a gateway is permanent, no other gateway
is tried, the `''` sentinel is written and no alarm fires. 0 such sentinels for
Pinata in 30 days of mainnet worker logs; the same search for `ipfs` did not run.
Fix with A2: for `ipfs://`, treat any non-2xx or transport error as a gateway
fault, and transient when every gateway fails that way.

## After the release that carries #705 and #709

These PRs are merged and wait for the next release. Testnet first, then mainnet:

1. Deploy all stacks. #710 renames the failure log field, so Compute and CloudWatch
   must both go: `deploy-testnet`, later `deploy-production`.
2. Check `list-metrics` on `SorobanBlockExplorer/Indexer/<env>` shows `IndexerFailures`.
3. Redrive `<env>-enrichment-dlq` and read the worker log: fetches must succeed
   through `ipfs.filebase.io` from the Lambda. This settles the gateway criterion.
4. Rewrite stored image addresses once the worker runs the new code:

   ```sql
   INSERT INTO nft_enrichment (contract_id, token_id, name, media_url, collection_name, version)
   SELECT contract_id, token_id, name,
          replaceOne(media_url, 'https://ipfs.io/ipfs/', 'https://ipfs.filebase.io/ipfs/'),
          collection_name, now64(3)
   FROM nft_enrichment FINAL
   WHERE media_url LIKE 'https://ipfs.io/ipfs/%'
   ```

5. Drain keys still with no row: `backfill-enrichment-runner nft-metadata`, at low
   concurrency, because Filebase allows 200 requests a minute.
