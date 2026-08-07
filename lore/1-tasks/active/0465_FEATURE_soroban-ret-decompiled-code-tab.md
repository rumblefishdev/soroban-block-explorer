---
id: '0465'
title: 'soroban-ret decompilation: experimental Code tab on contract detail'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: ['effort-medium', 'frontend', 'api', 'cooperation-inferara']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/374']
history:
  - date: 2026-08-07
    status: backlog
    who: claude
    note: 'Task created after the 2026-08-04 call with Inferara and two rounds of written answers (Dominik, Georgii). Design settled; refs #374.'
  - date: 2026-08-07
    status: active
    who: claude
    note: 'Activated; starting with the spike (Step 1).'
---

# soroban-ret decompilation: experimental Code tab on contract detail

## Summary

Embed the [Inferara soroban-ret](https://github.com/Inferara/soroban-ret)
decompiler (SCF #41, Apache-2.0) into the contract detail page: a separate
**Code** tab, labeled Experimental, showing decompiled Rust with a WAT
fallback — fully on demand, no persistence. The existing Interface tab stays
untouched. Refs #374.

## Context

On-chain Soroban contracts are WASM bytecode only; users cannot read what a
contract does. We already parse and render the declared interface
(`contractspecv0` → `wasm_interface_metadata`); soroban-ret adds the missing
half — function bodies. Design was agreed with the Inferara team (call
2026-08-04 + written answers). Prod scale: 132,544 contracts on 3,295
distinct wasm hashes.

## Decisions (agreed with Inferara)

- Build on **v0.0.4** (all SCF tranches complete; their recommended version).
  Pin `=0.0.4`, wrap the crate in a thin adapter module.
- **On-demand only**: `GET /v1/contracts/{id}/decompiled` — WASM bytes from
  Stellar RPC, decompile per request with a timeout; no cache table.
- Per-function boundaries via **`ir::ContractModule`** (their supported
  surface), not by slicing emitted source.
- Accuracy presentation (per Georgii): Experimental banner stays; quantified
  signals are **per-contract only** — stage-split copy (spec-derived
  signatures authoritative, bodies inferred), **counts not percentages**,
  per-function badges (recovered / partial / logic-lost / missing),
  validation diagnostics (`call_indirect` warnings rendered as benign),
  soroban-ret version chip. Never quote corpus-global figures in the UI.
- **Attribution** (required): "WASM decompilation provided by Inferara
  soroban-ret" under the code viewer, linking inferara.com and the repo.
- **"Report an issue" button**: prefilled GitHub issue against
  `Inferara/soroban-ret` (contract id, wasm hash, tool version, active
  Rust/WAT mode). Template details pending from Dominik.
- Behavioral-equivalence verification is **hosted/batch only** (their
  guidance: never in the request path; tri-state badge, never a percentage)
  — future work, not this task.

## Implementation Plan

### Step 1: Spike

Run soroban-ret 0.0.4 over the most-used distinct wasm hashes from prod
(order by instance count; top 20 hashes cover 94% of contracts). Measure
Rust success rate, hole density, timing. Rust-first is the assumption; the
spike validates it and produces a failure corpus to share back with
Inferara. Requires `rustup update` (MSRV 1.95).

### Step 2: API

`GET /v1/contracts/{id}/decompiled`: resolve `wasm_hash` → fetch bytes from
Stellar RPC → adapter around `soroban-ret` (`decompile_with_options`,
`wasm_to_wat`) with per-request timeout → response with rust/wat source,
`sdk_version`, recovery counts (on 0.0.4: count `todo!(` / `todo !(` /
`var_N` markers), diagnostics. Rate limiting on the route.

### Step 3: UI

Separate Code tab (Experimental marker) next to Interface: Rust/WAT toggle
(auto-WAT when Rust emission fails), permanent banner with stage-split copy,
recovery counts, copy button on the code block, download menu
(`.rs`/`.wat`/`.wasm`), report-issue button, version chips, attribution
line. Fallback ladder: Rust → WAT only → unavailable; SAC / no-wasm
contracts don't get the tab. Wireframes in the team note (see Links).

### Future work (separate tasks when reached)

"View code" links from Interface rows; per-function recovery badges via
`soroban_ret::recovery` (unreleased — on their `main`; release cut offered);
batch behavioral-equivalence job cached on `(wasm_hash, ret_version)` with
tri-state badge; decompilation caching if real traffic justifies it.

## Open Points

- Issue template contents/mechanism — pending from Dominik.
- On-demand latency budget: acceptable p95 for first Code-tab render
  (~0.1–3 s fetch + decompile); traffic threshold to revisit caching.
- Recovery-metrics source: release cut with `soroban_ret::recovery`
  requested (2026-08-07, along with GitHub Pages hosting for the benchmark
  report); if it lands before implementation, use the module — otherwise
  count markers on 0.0.4.

## Risks

- 0.0.x API churn — mitigated by pin + adapter.
- Complex contracts mostly don't compile clean (3/24 corpus) — expect
  readable Rust with explicit `todo!()` holes; UI copy must not overpromise.
  Hole counts measure completeness, not correctness.
- Every Code-tab open burns API CPU (repeat views repeat work) — timeout +
  rate limit; caching only if traffic hurts.
- Expired/archived contract code → RPC can't serve bytes → Code tab
  unavailable (Interface unaffected).

## Acceptance Criteria

- [ ] Spike results documented in task notes (success rate, hole density,
      timing over top prod hashes) and failure corpus shared with Inferara.
- [ ] `GET /v1/contracts/{id}/decompiled` with timeout, rate limit, pinned
      `=0.0.4` behind an adapter module; unit + integration tests
      (mocked RPC / sample wasm).
- [ ] Code tab shipped with full fallback ladder, Experimental marking,
      banner, counts, copy/download, report-issue button, attribution line.
- [ ] **Docs updated** — `docs/architecture/**`: new endpoint + frontend
      data contract, per ADR 0032.
- [ ] **API types regenerated** — new endpoint DTOs surface in
      `libs/api-types` (`npx nx run @rumblefish/api-types:generate`).

## Notes

- Team note with wireframes, both rounds of Inferara answers and the full
  decision log: https://claude.ai/code/artifact/c32ad835-826e-4732-af8a-aed725448a43
- soroban-ret facts verified 2026-08-03/05: panic-safe API
  (`InternalPanic`), `wasm_to_wat()` independent of Rust emission,
  `#[non_exhaustive]` discipline, corpus restoration 92.4% (post
  grading-bug audit), behavioral equivalence 99.2% (all divergences honest
  `todo!()` traps).
- SAC contracts have no WASM by design (native host implementation) — a
  possible separate cheap task: serve the canonical static StellarAsset
  interface for `is_sac` contracts + link to rs-soroban-env instead of the
  empty state.
