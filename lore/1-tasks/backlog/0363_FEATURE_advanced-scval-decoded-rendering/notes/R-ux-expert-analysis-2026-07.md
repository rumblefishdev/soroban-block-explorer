---
title: 'UX expert analysis of the raw-JSON tx detail (2026-07)'
type: research
status: mature
spawns: []
tags: [frontend, transaction-detail, ux]
links: []
history:
  - date: '2026-11-17'
    status: mature
    who: karolkow
    note: 'Moved out of the task README unchanged when the task became a directory.'
---

# UX Expert Analysis (`/ux-expert`)

Audit of the current state + per-option verdict, grounded in the actual
components (`EventsSection`, `OperationJsonDetail`, `HighlightedJson`, `XdrRow`)
and the decoder (`crates/xdr-parser/src/scval.rs`). Premise confirmed:
`scval_to_typed_json` emits a clean, recursive tagged union `{type, value}` — a
`switch(type)` renderer is real, not speculative. `scval.rs:19` (`e.name()`)
does drop the error code, so the `Type/Code` chip needs the Rust fix.

## Current-state findings (2 Critical, 5 Major)

| #   | Finding                                                                                                                                        | Dimension                                     | Sev         |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- | ----------- |
| F1  | Meaning buried under `{type,value}` scaffolding — type tags dominate, values buried (signal-to-noise inverted)                                 | Cognitive Load / Data Presentation            | 🔴 Critical |
| F2  | No overview / progressive disclosure — all events fully expanded = ~13k px wall; `XdrRow` collapse exists but only on base64 blobs             | Information Architecture / Screen Real Estate | 🔴 Critical |
| F3  | Amounts unreadable — `i128` as raw digit string, no grouping / decimal-adjust / asset context; `formatCompactAmount` unused here               | Data Presentation                             | 🟠 Major    |
| F4  | Addresses raw + unclickable inside topics/data/args — same datum rendered two ways (Contract column uses `IdentifierDisplay`, inline does not) | Interaction Cost / Data Presentation          | 🟠 Major    |
| F5  | Table container fights tree content — 2 free-width JSON columns, `overflowX:auto`, `break-all` snaps strkeys mid-char                          | Screen Real Estate / Responsiveness           | 🟠 Major    |
| F6  | No semantic/human layer — even decoded, reader must assemble "Transfer X: A→B" themselves                                                      | Information Architecture                      | 🟠 Major    |
| F7  | Error code dropped at `scval.rs:19` (`e.name()` only) — `Type/Code` chip impossible without Rust fix (shared w/ 0352)                          | Data Presentation / Correctness               | 🟠 Major    |

F1+F2 together = user cannot do the primary task (understand the tx) without
hand-parsing JSON. **Quick wins (parts already exist):** collapse via `XdrRow`
kills the wall before any decoder work; route addresses → `IdentifierDisplay`,
amounts → `formatCompactAmount`. `XdrRow` already has `role=button` /
`aria-expanded` — carry that a11y into `ScValView` collapse.

## Per-option verdict

- **E (raw JSON, current):** ✅ keep ONLY as per-node "raw" fallback; ❌ never default. Escape hatch, not a view.
- **A (collapse):** ✅ highest ROI, uses existing `XdrRow`. **But collapse without a decoded summary line is a blind toggle** — A has a hard dependency on ≥ B-lite (decode `topics[0]` sym + contract) for the row label.
- **B (`ScValView`):** ✅ the load-bearing fix (F1/F3/F4/F5). Input already a clean union → one component, table-driven `switch`. Ponytail: no plugin architecture; first cut = common types (sym/address/int/bool/bytes/vec/map), tail (void/duration/timepoint/contract_instance) handled but not gold-plated. Must default-collapse nested structures or it becomes a new wall.
- **C / C-lite (semantic):** ✅ biggest comprehension gain (F6), biggest domain risk. Known-shape match is a heuristic → unknown shapes MUST degrade to plain B, never guess; "assets moved" amount needs decimals metadata. C-lite (known events + `fn(args)→result` signature/call-tree) is the right ceiling. Named-params-from-ABI = genuine bonus, defer.
- **D (JSON-viewer lib):** ❌ reject. Renders the same `{type,value}` scaffolding prettier — no decode, no linked addresses, no formatted amounts. Solves F2 weakly, nothing else, adds a dependency + foreign visual language. Wrong rung when B (one component + existing primitives) solves 100%.

## Sequencing (affirms the Implementation Plan, two nuances)

1. **B-core** first — A and C both depend on a decoded summary. Wire into events topics/data + op args.
2. **A** (collapse via `XdrRow`) — each collapsed row now has a real summary line. Kills the wall.
3. **C-lite** — known-event line + invocation signature/call-tree, with graceful fallback to B.
4. Fix **`scval.rs:19`** error code (with 0352) — unblocks the `Type/Code` chip.
5. Bonus: named params from ABI — defer.

**Two corrections to make explicit in the plan:** (1) A has a hard dependency on
B-lite — collapse needs a summary to show; (2) reject D outright. AC mapping is
clean: "13k→scannable" ← A, "typed chips" ← B, "semantic summary" ← C-lite.
Task is well-diagnosed and well-scoped.
