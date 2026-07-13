---
id: '0378'
title: 'Typed OpFacts IR — kill the 2nd God-Payload (OpTyped::from_details) + retire legacy columns'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-low, effort-large, layer-indexer, refactor]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles R2, R1, R3, God-Payload #2, fetch_operations migration, adoption #1/#3.'
---

# Typed OpFacts IR — kill God-Payload #2

## Summary

Finish the typed-extraction migration 0359 started for accounts. The op-COLUMN
extractor (`OpTyped::from_details`) still string-matches out of the `details`
JSON (the 2nd God-Payload). Make one typed op-facts IR the single source; the
`details` JSON becomes a derived view.

## Context

Spawned from 0359. 0359 killed the account-participant God-Payload
(`op_participant_str_keys`) via a typed emitter. The column extractor remains a
string round-trip (XDR → details JSON → re-parsed columns, silent `_ => {}`). It
produces **correct columns today** → this is a pure refactor, no data win, higher
risk (column + API parity), hence deferred.

## Implementation

- **R2** — typed OpFacts IR: one typed emitter produces dest/contract/assets/pools
  once from XDR; `details` JSON derived from it.
- **God-Payload #2** — delete `OpTyped::from_details` string extraction.
- **fetch_operations migration** — move `fetch_operations` off the legacy
  `operations_appearances.asset_code` / `asset_issuer_id` columns (per-op asset),
  THEN drop those columns from `init.sql` (0359 §14 — they are NOT dead; live
  reader).
- **R3** — ops cleanup: dead `account_balances_current`, `idx_oa_asset_issuer_id`
  bloom, the retired legacy asset columns.
- **R1** — shared tx-feed engine · **Adoption #1** central `meta.rs` (silent-V5,
  6 sites) · **Adoption #3** shared commit-fence builder.
- Quick-win: deterministic HashMap output ordering (state.rs / nft.rs).

## Acceptance Criteria

- [ ] typed OpFacts IR; `details` is a derived view — R2
- [ ] `OpTyped::from_details` string extractor deleted
- [ ] `fetch_operations` migrated off legacy asset columns; columns dropped
- [ ] R3 dead-code / index cleanup done
