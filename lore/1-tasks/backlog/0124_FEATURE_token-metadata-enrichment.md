---
id: '0124'
title: 'Indexer: token metadata enrichment pipeline'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0049', '0074']
tags: [priority-low, effort-medium, layer-indexer, audit-gap]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — tokens.metadata column exists but is hardcoded to None. No plan to populate it.'
---

# Indexer: token metadata enrichment pipeline

## Summary

The `tokens.metadata` JSONB column exists in the schema but `convert.rs:167` hardcodes it
to `None`. The `ExtractedToken` struct has no `metadata` field. No task in backlog, active,
or archive addresses populating this column.

## Context

Token metadata (description, icon URL, home domain) comes from:

- SEP-1 stellar.toml files published at issuer's home_domain
- On-chain contract metadata (for Soroban tokens)
- External enrichment services

## Implementation

1. For classic assets: fetch SEP-1 TOML from issuer's `home_domain` (if set in accounts
   table) to extract currency metadata.
2. For Soroban tokens: extract metadata from contract interface (name, symbol, decimals
   already available from WASM spec).
3. Store in `tokens.metadata` JSONB: `{"description": "...", "icon": "...", "domain": "..."}`.
4. Decide: at ingestion time or as a separate enrichment job?

## Acceptance Criteria

- [ ] `tokens.metadata` populated for classic assets with available SEP-1 TOML data
- [ ] `tokens.metadata` populated for Soroban tokens with contract-level metadata
- [ ] Graceful handling of missing/unavailable metadata (remains NULL)
- [ ] API returns metadata in token detail response
