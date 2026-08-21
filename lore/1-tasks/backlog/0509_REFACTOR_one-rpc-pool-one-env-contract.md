---
id: '0509'
title: 'REFACTOR: two RPC pools with two different env contracts, and four third-party endpoints in code'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0455']
tags:
  [
    'architecture',
    'rust',
    'configuration',
    'egress',
    'effort-small',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0455 review sweep (findings 8, 9). The second finding was
      reported against the API and is actually in the shared enrichment crate —
      corrected here; the API still honours the singular variable, the other
      pool does not.
---

# REFACTOR: one RPC pool, one environment contract

## Summary

Two independent constructors build a Soroban RPC endpoint pool, and they do not
agree on how it is configured. One honours a singular fallback variable, the
other ignores it. Both fall back to third-party endpoints compiled into the
binary, so a misconfigured or unconfigured deployment still reaches out to the
public internet rather than failing.

## Context — verified 2026-08-19

| Site                                                       | Reads              | Singular fallback | Default                  |
| ---------------------------------------------------------- | ------------------ | ----------------- | ------------------------ |
| `crates/api/src/runtime_enrichment/wasm_code.rs:64`        | `SOROBAN_RPC_URLS` | **yes** (`:73`)   | one SDF endpoint (`:36`) |
| `crates/enrichment-shared/src/nft_token_uri/client.rs:123` | `SOROBAN_RPC_URLS` | **no**            | four endpoints (`:44`)   |

`infra/src/lib/stacks/compute-stack.ts:462` records the move: the four-endpoint
list was taken out of the Lambda environment and put into the code, leaving the
environment variable as an ad-hoc override.

Two consequences:

- **The contract is not one contract.** An operator setting `SOROBAN_RPC_URL`
  changes one pool and silently not the other.
- **Egress is a default, not a declaration.** `crates/api` depends on
  `enrichment-shared`, so the read-side API links a client whose zero-config
  behaviour is to call four third-party hosts. Nothing states, in one place,
  which external hosts this system is allowed to reach.

Neither is breaking production. Both make the blast radius of a
misconfiguration larger than it reads.

## Implementation

- One pool constructor, used by both call sites, with one documented
  environment contract (plural, singular fallback, or neither — but the same
  answer in both places).
- Decide whether an unconfigured pool should **fail** rather than silently use
  compiled-in third-party hosts. Failing is the honest default for a service;
  the compiled list may still be right for the CLI.
- Write the allowed external hosts down in one place, so "what does this system
  call out to" has an answer that is not a grep.

## Acceptance Criteria

- [ ] One constructor; both call sites use it
- [ ] The environment contract is identical at both sites and documented
- [ ] Decision recorded on unconfigured behaviour (fail vs compiled default),
      with the reason
- [ ] The external hosts this system may reach are listed in one place
- [ ] **Docs updated** — the infrastructure overview names the egress surface
- [ ] **API types regenerated** — N/A, no API surface change expected
