# Pending NFTs never get metadata

Measured 2026-10-08 on the `testnet` database (read-only): `nft_enrichment`
held ~32k keys; only 232 were in `nfts`, 19,095 in `nfts_pending`.

- The indexer queues an `NftTokenUri` enrichment message only at mint, and
  only for a key without an enrichment row.
- `enrich nft-metadata` walks `FROM nfts FINAL` only.

So an NFT that sits in `nfts_pending` when its mint is seen, and is promoted
later, is never (re-)enriched by either path. Not yet measured on mainnet;
the same code runs there. Belongs to this task's completeness goal: either
the promotion step queues the enrichment, or the CLI also walks promoted keys
without a real row.
