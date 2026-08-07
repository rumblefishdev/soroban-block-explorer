---
id: '0459'
title: 'REFACTOR: split xdr-parser state.rs by domain'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0453']
tags: [backend, xdr-parser, architecture, priority-low, effort-medium]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453's architecture review: state.rs is 3,290 lines
      spanning five domains (contract deployments, account states, LPs,
      assets, NFTs) — the crate's god module and the graph's densest hotspot
      (pre-existing, not introduced by 0453).
---

# REFACTOR: split xdr-parser state.rs by domain

## Scope

Split along the file's own documented "Step" sections into per-domain
modules (deployments / accounts / liquidity-pools / assets / nft), keeping
the public API of the crate unchanged — callers in api/indexer must not
notice. No behavior change; clippy + full test suite as the gate.

## Acceptance criteria

- [ ] state.rs dissolved into cohesive modules, each under ~800 lines
- [ ] Zero public-API changes; `cargo test -p xdr-parser` and dependents
      green with no test edits
