---
id: '0343'
title: 'Contract Interface: render function doc as an italic comment, preserve rustdoc line breaks'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: ['area-frontend', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-07-02
    status: active
    who: stkrolikiewicz
    note: 'Task created'
---

# Contract Interface: render function doc as an italic comment, preserve rustdoc line breaks

## Summary

The per-function `doc` shown in the contract Interface tab is the verbatim
rustdoc string embedded in the contract spec (`ScSpecFunctionV0.doc`). It
currently renders as an upright, whitespace-collapsed paragraph, so the
author's `-` bullets and `# Panics` heading run together into one hard-to-read
blob. Make it read like a code comment: dimmed italic, with the original line
breaks preserved.

## Context

- Source of the text: contract author's `///` doc comments, captured into the
  WASM `contractspecv0` section at compile time, read verbatim by
  [`spec_function_to_contract_function`](../../../crates/xdr-parser/src/contract.rs)
  (`func.doc` → `ContractFunction.doc`), surfaced as `fn.doc`.
- Render site: [`ContractInterface.tsx`](../../../web/src/pages/contracts/ContractInterface.tsx)
  wraps `{fn.doc}` in a plain `<Typography>`. Default HTML whitespace handling
  collapses the rustdoc `\n` separators, and the font is upright — so the doc
  looks like a run-on error rather than an annotation.
- The data is correct; this is purely a display polish. No markdown rendering
  (rustdoc `#` / backtick intra-doc links stay literal) — deferred, add only if
  a request for full formatting comes in.

## Implementation Plan

### Step 1: Style the doc `<Typography>` as a comment

In the `fn.doc` block of `FunctionRow`, add to the `sx`:

- `fontStyle: 'italic'` — reads as an annotation, not code.
- `whiteSpace: 'pre-wrap'` — preserve the rustdoc line breaks so bullets and
  the `# Panics` heading land on their own lines. Harmless when the doc has no
  newlines.

Keep `color: text.tertiary` (already dimmed) and the proportional
`bodyXsRegular` variant (mono would read as code, not prose).

## Acceptance Criteria

- [ ] Function doc renders dimmed + italic in the Interface tab.
- [ ] Multi-line rustdoc (e.g. `transfer_team`: `-` bullets + `# Panics`) keeps
      its line breaks instead of collapsing to one line.
- [ ] Empty-doc functions still render nothing extra (existing `fn.doc !== ''`
      guard unchanged).
- [ ] **Docs updated** — N/A. Pure frontend rendering polish; no change to
      schema, endpoints, ingestion, infra, XDR responsibilities, or the
      frontend data contract (the `doc` field already flows end-to-end).
- [ ] **API types regenerated** — N/A. No change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

Two-line `sx` change in one file. Verify visually against a contract whose
functions carry multi-line rustdoc (e.g. the `Collateral` contract's
`transfer_team`).
