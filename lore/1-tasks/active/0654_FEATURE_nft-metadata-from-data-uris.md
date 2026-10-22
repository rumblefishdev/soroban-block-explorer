---
id: '0654'
title: 'FEATURE: NFT metadata and images inlined as data: URIs'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0520', '0644', '0311']
tags: ['enrichment', 'nft', 'frontend', 'effort-small', 'priority-high']
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/684'
history:
  - date: 2026-10-22
    status: active
    who: karolkow
    note: >
      Split out of 0520 (its defect 3, the data: URI half) after issue #684.
      The owner chose the small fix now; the rest of 0520 stays in backlog.
---

# FEATURE: NFT metadata and images inlined as data: URIs

## Summary

Some NFT contracts return their metadata inline from `token_uri()` — a
`data:application/json,…` string instead of a link — and some inline the image
too (`data:image/svg+xml,…`). The enrichment worker rejects every `data:` value
as an unsafe scheme and stores an empty row, so the site shows "Token N" and a
placeholder. Read `data:` values locally, accept inline images, show them on
the detail page, and make the existing empty rows re-runnable.

## Stan teraz

- Done: code and tests for every decision row (PR on
  `feat/0654-nft-metadata-data-uris`); verified end to end against mainnet
  RPC into a local ClickHouse.
- Next: review, merge, deploy; then the operator re-run below.
- In force: no network request for a `data:` value; the 8 KB `media_url` cap
  and the 256 KB metadata cap stay as they are.

## Context

Measured 2026-10-21/22 on mainnet (details: 0520
`notes/R-issue-684-inline-data-uri.md`):

- `CDC5ZX…JFGB` (Stroopies, 7 tokens): plain `data:application/json,{…}`; its
  `image` is `data:image/svg+xml,<svg …>`, 5,080–5,293 bytes.
- `CARTUL…UQZ4` (SushiSwap V3 positions, 1,284 NFTs without metadata):
  `data:application/json;base64,…` with `name` and `description`, no image.
  Closed positions revert (`Error(Storage, MissingValue)`) and stay empty.
- `--retry-sentinels` walks only rows where `name`, `media_url` AND
  `collection_name` are empty; Stroopies rows carry `collection_name =
'Stroopies'`, so it skips them. 3,275 rows of displayed NFTs have no name
  and no media; the current predicate walks 13,478 rows, mostly keys of
  fungible contracts that are not in `nfts`.
- SEP-50 (Draft 0.1.0) sets no rule on URI schemes; the WHATWG data-URL
  processor is the parsing reference (0520 note).

## Decisions so far

| Situation                                                                    | What happens                                                                                      | Date       |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------- |
| `token_uri` is `data:application/json,<json>` (plain, percent-decoded)       | parsed locally as the metadata JSON, no request                                                   | 2026-10-22 |
| `token_uri` is `data:application/json;base64,<b64>`                          | decoded and parsed the same way                                                                   | 2026-10-22 |
| `token_uri` is `data:image/…`                                                | treated as the direct-image convention: `{"image": <the data: value>}`                            | 2026-10-22 |
| `token_uri` is `data:` with any other media type (`text/html`, `text/plain`) | permanent error, empty row, as an unsupported content type today                                  | 2026-10-22 |
| `data:` value that does not parse (no comma, bad base64, bad JSON)           | permanent error, empty row                                                                        | 2026-10-22 |
| decoded `data:` body over the 256 KB metadata cap                            | permanent error (`BodyTooLarge`)                                                                  | 2026-10-22 |
| `image` is `data:image/{png,jpeg,gif,webp,svg+xml}` within 8 KB              | stored as `media_url` as received                                                                 | 2026-10-22 |
| `image` is any other `data:` type, `http:`, `javascript:`                    | empty `media_url`, as today                                                                       | 2026-10-22 |
| `image` is `https://…` or `ipfs://…`                                         | unchanged                                                                                         | 2026-10-22 |
| detail page gets a `data:image/…` `media_url`                                | renders it as an image                                                                            | 2026-10-22 |
| `--retry-sentinels`                                                          | walks displayed NFTs (`nfts`) with empty `name` and `media_url`, whatever `collection_name` holds | 2026-10-22 |

## Not yet decided

- When the production re-run happens (operator step, after the deploy):
  `nft-metadata --retry-sentinels` against production, ~3,275 candidates.

## Out of scope

- Status column, retry schedule, gateway replacement, fungible-contract rows
  (0520).
- Refreshing images that a contract computes at call time (needs 0633).
- Serving images through our own endpoint (rejected for now: ~99% of linked
  images load, sample of 6 per host, 2026-10-21).

## Acceptance Criteria

- [x] Every row above has a test written before the code
- [x] Stroopies and a live SushiSwap position resolve to their names locally
      (worker against mainnet RPC, docker ClickHouse)
- [x] A real Stroopies `data:image/svg+xml` value renders in a browser `<img>`
      (Chromium; Firefox and WebKit not checked)
- [ ] Production re-run done; Stroopies shows names and images (operator)
- [x] **Docs updated** — `docs/architecture/indexing-pipeline/enrichment.md`
      (accepted `token_uri` forms), `crates/backfill-enrichment-runner/README.md`
      (`--retry-sentinels` predicate)
- [x] **API types regenerated** — `Cargo.lock` changes (`data-url` crate);
      the regenerated types are unchanged
