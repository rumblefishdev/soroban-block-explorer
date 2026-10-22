---
id: '0655'
title: 'FEATURE: read off-chain metadata once — store what the detail page shows, drop the live re-read'
type: FEATURE
status: active
related_adr: ['0043']
related_tasks: ['0654', '0520', '0633', '0188', '0195']
tags:
  [
    'enrichment',
    'nft',
    'sep1',
    'clickhouse',
    'api',
    'effort-medium',
    'priority-medium',
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/684'
history:
  - date: 2026-10-22
    status: active
    who: karolkow
    note: >
      Spawned from 0654 review: the same off-chain source is read twice, once
      by the enrichment worker (stored list fields) and again live on every
      detail-page open (detail-only fields). The owner chose one read, for
      NFTs and assets together, and a rule change in ADR 0043.
---

# FEATURE: read off-chain metadata once

## Summary

Two off-chain sources are read twice. An NFT's `token_uri` JSON is read by the
enrichment worker at mint (name, image, collection → `nft_enrichment`) and
again on every detail-page open by the API (`metadata` → attributes). An
issuer's SEP-1 `stellar.toml` is read by the worker (icon, name →
`asset_enrichment`) and again on every asset detail open (description, home
page). The detail page mixes the two reads. Read each source once, in the
worker; store what the detail page shows; the API serves it from the database.

## Stan teraz

- Done: ADR 0043 amended (NFT PR); asset part built on
  `feat/0655-asset-metadata-once` from `develop`.
- Next: review of both PRs; production `ALTER`s, deploy, refills.
- In force: list endpoints unchanged; `#687` (0654) lands first.

## Context

- ADR 0043 rule today: a detail-only field is "never persisted", read live
  (`lore/2-adrs/0043_field-allocation-rule.md:92`); its matrix splits one
  NFT JSON into stored `name`/`media_url` and live `metadata` (`:195-196`),
  and one SEP-1 TOML into stored `icon_url`/`name` and live
  `description`/`home_page` (`:188`, `:192`).
- Live reads: `crates/api/src/nfts/handlers.rs:208` (RPC + HTTP, 3 s cap,
  24 h in-process cache), `crates/api/src/assets/handlers.rs:277` (SEP-1).
- Who does the same (checked): Blockscout stores the parsed metadata in
  `token_instances.metadata` with error and refetch fields, fetched in the
  background (`apps/explorer/lib/explorer/chain/token/instance.ex:45`);
  Alchemy stores `tokenUri` and `rawMetadata`. None reads on view as the main
  path. SEP-50 has no metadata-update event.
- Measured 2026-10-22: NFT metadata JSON median 610 B, p90 1.9 KB, max
  5.5 KB (40 sampled); 14,395 NFTs → ~15 MB (estimate). `asset_enrichment`
  371,081 rows; 38,564 issuers with a home domain on 18,743 domains.
- Live and stored already disagree: 2 of 21 NFT comparisons (0654 census).

## Decisions so far

| #   | What is done                                                                                                                                                                                              | From                            | vs the task | Who does the same                         | Code                                                                                                           | Test                                                                                             |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- | ----------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 1   | One read per off-chain source, in the enrichment worker                                                                                                                                                   | owner (A)                       | —           | Blockscout, Alchemy                       |                                                                                                                |                                                                                                  |
| 2   | Scope: NFTs and assets in this task                                                                                                                                                                       | owner                           | —           | —                                         |                                                                                                                |                                                                                                  |
| 3   | ADR 0043 rule changes: a source is read once; what the detail page shows from it is stored with the list fields. Live reads stay only for sources nothing is stored from (full transaction XDR, ADR 0029) | owner                           | —           | —                                         |                                                                                                                |                                                                                                  |
| 4   | `nft_enrichment` gains `metadata String DEFAULT ''` (the JSON as read) and `token_uri String DEFAULT ''`                                                                                                  | agent                           | —           | Blockscout `metadata`, Alchemy `tokenUri` |                                                                                                                |                                                                                                  |
| 5   | `asset_enrichment` gains `description` and `home_page`, `Nullable(String) DEFAULT NULL`, filled from the same SEP-1 read as `icon_url`/`name`                                                             | agent                           | —           | —                                         | `init.sql` asset_enrichment; `AssetEnrichmentRow`; `persist::insert_asset`; `sep1_assets::resolve_detail_text` | `resolve_detail_text_takes_desc_and_org_url_from_the_same_read`; `column_order_asset_enrichment` |
| 6   | `GET /v1/nfts/:id` `metadata` comes from the stored JSON; `''` → `null`, as a failed live read today                                                                                                      | agent                           | —           | —                                         |                                                                                                                |                                                                                                  |
| 7   | `GET /v1/assets/:id` `description`/`home_page` come from the stored columns                                                                                                                               | agent                           | —           | —                                         | `assets/sep1_detail_text.rs`; `handlers::get_asset`                                                            | `assets/sep1_detail_text/tests.rs`                                                               |
| 8   | The API makes no RPC or SEP-1 call for these endpoints; the live NFT fetcher and SEP-1 runtime fetch leave the API                                                                                        | agent                           | —           | —                                         | SEP-1 part: `runtime_enrichment::sep1` removed from the API                                                    | compile: `RuntimeEnrichment` has no SEP-1 fetcher                                                |
| 9   | Columns added before the code deploy (DEFAULT makes old builds' inserts work); history refilled by the runner after                                                                                       | agent, `docs/deployment.md:358` | —           | —                                         |                                                                                                                |                                                                                                  |
| 10  | NFT PR stacked on `#687`; asset PR on `develop`; each PR carries its own `ALTER`                                                                                                                          | agent                           | —           | —                                         |                                                                                                                |                                                                                                  |

## Not yet decided

- Production order and timing of the two `ALTER`s and refills (operator).

## Out of scope

- Retry schedule and fetch status (0520).
- Refresh of metadata a contract computes per call (needs 0633); refresh on
  demand stays the runner's `--force-retry`.
- Serving images through our own endpoint.

## Acceptance Criteria

- [ ] ADR 0043 amended (rows 3), reviewed by the owner before code
- [ ] Every row above has a test written before the code
- [ ] NFT and asset detail endpoints make no outbound call (test with the
      fetchers absent)
- [ ] Production: both `ALTER`s, deploy, both refills; a Stroopies token and a
      classic asset with a TOML description show them from the database
- [ ] **Docs updated** — ADR 0043, `docs/architecture/indexing-pipeline/enrichment.md`,
      backend overview (runtime enrichment section)
- [ ] **API types regenerated** — handler and DTO doc changes
