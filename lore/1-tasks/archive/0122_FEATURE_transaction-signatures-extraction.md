---
id: '0122'
title: 'Indexer: extract transaction signatures'
type: FEATURE
status: superseded
related_adr: ['0004', '0029']
related_tasks: ['0024', '0046', '0050', '0150']
tags: [priority-low, effort-small, layer-indexer, audit-gap]
milestone: 1
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — tech design specifies signatures display on tx detail but XDR parser does not extract them.'
  - date: '2026-05-12'
    status: superseded
    who: stkrolikiewicz
    by: ['0150']
    note: >
      ADR 0029 (2026-04-22) reversed the storage approach this task assumed:
      signatures are not persisted as JSONB on `transactions`. Instead, task
      0150 (completed 2026-04-22) added a read-time public-archive XDR fetch
      that extracts signatures from the envelope; tasks 0046 and 0050 wired
      the extracted `heavy.signatures` into the E3 (`/v1/transactions/:hash`)
      and E14 endpoints. Verified live: `SignatureDto` at
      `crates/api/src/runtime_enrichment/stellar_archive/dto.rs:73`,
      `envelope_signatures()` at `extractors.rs:209`, response wiring at
      `crates/api/src/transactions/{dto.rs:62, handlers.rs:139}`. AC1
      ("extracted AND stored as JSONB") is itself obsolete under ADR 0029;
      AC2 + AC3 are met via the read path. User-facing gap from the
      2026-04-10 pipeline audit is closed.
---

# Indexer: extract transaction signatures

## Summary

The technical design specifies showing signature data on the transaction detail page.
`DecoratedSignature` in Stellar XDR contains only a 4-byte public key hint and the
signature blob — signer weight is NOT available from the envelope (it lives in the
account's signers list on the ledger). The XDR parser does not extract signatures and
the transactions table has no signatures column.

## Implementation

1. Extract `signatures` from `TransactionEnvelope` during XDR parsing (they are in the
   envelope's `signatures` field — `Vec<DecoratedSignature>`).
2. Store as JSONB column on `transactions` table or decode from `envelope_xdr` at API time.
3. Recommendation: store at ingestion time (consistent with ADR 0004 — no server-side XDR).

## Acceptance Criteria

- [ ] Transaction signatures extracted and stored (JSONB array)
- [ ] Each signature includes: public key hint, signature hex
- [ ] API returns signatures in transaction detail response
