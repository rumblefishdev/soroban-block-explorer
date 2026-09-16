---
id: '0520'
title: 'FEATURE: metadata enrichment follows the stored verdict, records why a fetch failed, and retries what can recover'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0455', '0311', '0392', '0220', '0231', '0542']
tags:
  [
    'enrichment',
    'nft',
    'sep1',
    'clickhouse',
    'effort-medium',
    'priority-medium',
  ]
links:
  - crates/indexer/src/handler/mod.rs
  - crates/indexer/src/handler/enrichment_publish.rs
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs
  - crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs
  - crates/xdr-parser/src/token_metadata.rs
  - crates/xdr-parser/src/ledger_entry_changes.rs
history:
  - date: 2026-08-27
    status: backlog
    who: karolkow
    note: >
      Spawned from 0455's carried-items list. The umbrella's defect 2 is
      "health measured by success"; this is that defect applied to an
      enrichment family - the worker reports success, a fifth of promoted NFTs
      carry nothing, and no signal separates the legitimate residual from a
      silent failure. Measured before filing rather than estimated.
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Research questions answered by measurement (production logs + a full
      re-fetch of every empty row); converted to an implementation task.
      Widened with the NFT-verdict inconsistency found in the indexer's
      enrichment producer. Decided: A1 + A2 + A3 below, in one task.
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      A4 folded in: the token-metadata warning matches a key that oracle-style
      pools use for their own bookkeeping, so it fires on 33 non-token
      contracts and hides the case it exists for. Same subject (metadata we can
      or cannot read), so it rides here rather than in 0473.
---

# FEATURE: metadata enrichment consistency and retry

## Summary

Four defects in the metadata path, one task:

1. **The producer ignores the NFT verdict.** Token contracts' `mint` events are
   queued for NFT metadata. 53 239 empty rows, none of them an NFT.
2. **A failed fetch is recorded as "nothing to fetch", forever.** An all-empty
   row means both, and is never retried. 20.9% of displayed NFTs and most asset
   rows are empty with no way to tell why.
3. **Two NFT URI shapes cannot be fetched at all:** metadata inlined as a
   `data:` URI, and IPFS URIs through gateways that now refuse the worker.
4. **The "token metadata we cannot read" warning fires 4 504 times a week**, every
   one of them a contract that is not a token.

## Measured 2026-09-15/16 (production, read-only)

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

## Implementation

**A1 — enrich what staging stored.** The producer takes its NFT candidates from
staging's routed output (rows written to `nfts` with a mint), not from the
parser. Pending NFTs are enqueued when `nft_reclassify` promotes them. One
routing decision, every consumer reads it (the 0542 principle applied to NFTs).

**A2 — explicit fetch outcome + narrow retry.**

- Replace the all-empty sentinel with a status: `ok` / `no_metadata` (the
  contract answers, nothing to fetch by design) / `failed` + reason, attempts,
  last attempt. New columns ship with DEFAULT (ingest froze twice in 0548 on a
  column without one).
- Retry `failed` with backoff 1 h / 1 d / 7 d, then stop — only for HTTP, DNS,
  TLS, timeout and "asset not listed in the toml yet". Never for contract
  reverts or malformed URIs. Issuers without a home domain are re-checked when
  the domain changes, not re-fetched.
- Accept `data:application/json` URIs; replace the IPFS gateway; add
  "preflight queue full" to the transient list; raise the SEP-1 timeout.
- API maps the status: "no metadata" and "could not fetch" render differently
  (never an empty value that looks real).

**A3 — history.** Existing empty rows become `failed` (retried by A2, so no
manual `--retry-sentinels`); the 53 239 Fungible-contract rows are deleted
(operator runs the DELETE).

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

## Acceptance Criteria

- [ ] No NFT enrichment request for a contract staging routes to Drop; the
      Fungible-contract empty rows stop growing (re-measure after a week)
- [ ] Every enrichment row carries a status; no all-empty sentinel is written
- [ ] Retry schedule exercised by a test; contract reverts are never retried
- [ ] `data:` JSON URIs decode; the IPFS gateway answers from the Lambda
- [ ] After backfill: re-run the re-fetch measurement — recovered counts
      reported against the table above
- [ ] The metadata warning fires only for contracts that really carry token
      metadata — re-measure the log line count after a week
- [ ] **Docs updated** — `docs/architecture/**` (enrichment pipeline, schema),
      `docs/backfills.md` (A3 steps)
- [ ] **API types regenerated** — if the status reaches the API surface
