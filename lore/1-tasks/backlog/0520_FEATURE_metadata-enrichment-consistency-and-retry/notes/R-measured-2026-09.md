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
