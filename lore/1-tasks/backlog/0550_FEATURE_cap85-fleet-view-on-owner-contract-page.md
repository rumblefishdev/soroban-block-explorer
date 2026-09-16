---
id: '0550'
title: 'FEATURE: CAP-85 fleet view — show what an owner contract manages and who runs it'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0548', '0327']
tags: [priority-low, effort-medium, layer-api, layer-frontend, protocol-28]
links:
  - 'https://github.com/stellar/stellar-protocol/blob/master/core/cap-0085.md'
history:
  - date: 2026-09-14
    status: backlog
    who: karolkow
    note: >
      Spawned from 0548 (option C of the frontend decision). The member-side
      "runs code of <owner> · tag <tag>" display is separate, smaller work;
      this task is the owner side, which needs a new API read.
---

# CAP-85 fleet view on the owner contract page

## Summary

Protocol 28 (CAP-85, "Externally managed contract executables") lets a contract
run code referenced from another contract's storage: the owner keeps
`tag → wasm hash` entries, and every contract pointing at `(owner, tag)` — a
"fleet" — changes code at once when the owner re-points the tag. It is
Stellar's equivalent of Ethereum's beacon-proxy pattern, which the CAP itself
names. The member page can say whose code it runs; the owner page today says
nothing about the fleets it controls.

## Context

Task 0548 stores the reference on the member (`soroban_contracts.executable_owner_id`,
`executable_tag`) and the owner's targets in `contract_executable_refs
(owner_id, tag) → wasm_hash`, resolved at read time. The contract-detail API
exposes `executable_owner` / `executable_tag` for a member. Nothing reads the
relation in the other direction.

Worth doing only once fleets exist: production had zero members at 0548 time
(references are impossible before the 2026-09-16 vote). Re-measure before
starting:
`SELECT count() FROM contract_executable_refs` and
`SELECT countIf(executable_owner_id IS NOT NULL) FROM soroban_contracts FINAL`.

## Implementation

- API: an owner-side read — for a contract, its tags with the current target
  hash (`argMax(wasm_hash, ledger)` per `(owner_id, tag)`) and the member count
  per tag; a paginated member list per tag. Point lookups on
  `ORDER BY (owner_id, tag)`; no unfiltered aggregate of the refs table (the
  0548 review measured ~80 MiB per request for that shape).
- Consider consolidating the read-time resolution into one helper first (0548
  review: the refs subquery is pasted into two queries and resolved two
  different ways) so the fleet view does not add a third copy.
- Frontend: a "Managed executables" section on the owner's contract page —
  tag, current WASM hash (linked to its interface/decompiler), member count,
  link to the member list; the member list on the shared detail-table
  component.
- Docs: `docs/architecture/**` endpoint + frontend data contract (ADR 0032).

## Acceptance Criteria

- [ ] Owner contract page lists every tag it manages with the current target
      hash and member count, verified against RPC `getLedgerEntries` for at
      least one real mainnet fleet
- [ ] Member list per tag is paginated and matches the count
- [ ] A tag re-pointed twice shows the newer hash (versioned by ledger)
- [ ] Contracts that manage nothing render no section (not an empty one that
      reads as "manages zero")
- [ ] API types regenerated; docs updated
