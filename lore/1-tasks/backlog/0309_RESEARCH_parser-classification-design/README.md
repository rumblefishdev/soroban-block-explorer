---
id: '0309'
title: 'RESEARCH: fundamental parser/classifier design — total-function, never-silently-miss'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0308', '0296', '0283']
tags:
  [
    parser,
    classifier,
    architecture,
    completeness,
    sep-48,
    phase-future,
    effort-large,
    priority-medium,
  ]
links: []
history:
  - date: 2026-06-19
    status: backlog
    who: karolkow
    note: 'Spawned from the NFT investigation (0308). Captures the fundamental "how should our parser/classifier be built" research; decision/refactor deferred until after the tactical NFT fix.'
---

# RESEARCH: fundamental parser/classifier design

## Question

How SHOULD our contract/event parser + classifier be designed so it **never silently misses** a
category (NFT, fungible, …)? Is our current approach (keyword/function-name classifier + fixed
event-shape allow-list) the right paradigm, or is there a fundamentally better one? This task is the
strategic counterpart to [0308](../0308_FEATURE_custom-abi-nft-parser-classifier-coverage/README.md)
(the tactical NFT fix). It exists so the design research is not lost; the refactor itself is
**deferred** by decision (close NFTs first, rebuild later).

## Answer (from deep-research — see `notes/R-classification-design-patterns.md`)

- Absolute "never miss" is **impossible** on a permissionless chain (arbitrary WASM; formal interface
  guarantees insufficient). The state of the art is **"never SILENTLY miss"** — a **total function**
  that yields exactly one known class OR an explicit, **monitored UNKNOWN** bucket.
- Our keyword + fixed-shape allow-list is the documented **anti-pattern**. Correct production systems
  are **schema/codec-first** ("parse, don't validate" — decode against a declared spec).
- Stellar's standards-track path exists: **SEP-48** (typed event spec, in-Wasm) + **SEP-46/47**
  (static capability declaration, designed for indexers). But: specs are unverified/optional/absent,
  interface ≠ events-fire, `prefixTopics` not unique → all need an UNKNOWN backstop + WASM-export
  validation. Adoption is ~0 today, so near-term value is low; value grows over time.

## Proposed target architecture (composite total function)

Every contract/event flows down to **exactly one class or a monitored UNKNOWN**:

- **L0** spec-driven decode (SEP-48 event spec if present) — parse-don't-validate.
- **L1** capability detect (SEP-47/46 meta, **validated vs actual Wasm exports**).
- **L2** typed-shape discriminator cascade (by interface shape, not names) + tiebreakers.
- **L3** behavioral corroboration (emitted event shapes).
- **L4** everything unmatched → **explicit MONITORED UNKNOWN** (generalize `nfts_pending`); `%UNKNOWN`
  metric + alarm + promotion loop; **never** a silent drop.
- **Cross-cutting:** version/state-aware re-classification on Wasm upgrade (`update_current_contract_wasm`)
  — an open-research frontier (no published prior art; we'd pioneer).
- wasm-hash registry = CACHE only, never source of truth.

## Status / next steps (when picked up)

- Deferred — do not start until the 0308 tactical NFT fix + 0306 ops are done.
- Then: decide scope (whole-pipeline vs per-domain rollout), write an ADR for the total-function +
  monitored-UNKNOWN design, and a phased migration plan. The `minted` family (bytes token_id) and
  the heterogeneous custom tail are the concrete stress-tests this design must handle.
- **The implementation task this spawns must carry one extra criterion: delete
  `contract-type-rebuild`** — the subcommand, its row in `docs/backfills.md`, and its entry in
  `crates/backfill-runner/README.md`. That pass exists only because live leaves contracts the
  classifier cannot name at `Other`; a total classifier removes its reason to exist, and a
  maintenance pass left standing after its hole closes is how the 0425 audit found seven spent
  ones. Per lore 0425 clause 4. If the rollout is phased and `Other` survives in some domain,
  say which domain and why in the same PR, rather than keeping the pass by default.

## Notes

- Full research report + citations: `notes/R-classification-design-patterns.md`.
- The NFT-specific shape catalog (sibling research) lives in 0308's `notes/R-nft-event-shape-catalog.md`.
- Durable summary in auto-memory `reference-soroban-classification-seps`.
