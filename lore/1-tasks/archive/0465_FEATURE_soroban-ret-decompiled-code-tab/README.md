---
id: '0465'
title: 'soroban-ret decompilation: experimental Code tab on contract detail'
type: FEATURE
status: completed
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
  - date: 2026-08-10
    status: active
    who: claude
    note: >
      Steps 1-3 built and in review: API PR #384 (endpoint + RPC fetcher +
      infra env), UI PR #385 stacked on it (Code tab, Prism highlighting,
      line numbers, downloads, report-issue, diagnostics). Verified against
      production data through a local API bound to prod ClickHouse. Two
      review rounds folded in; `validation` diagnostics added ahead of the
      0.0.5 release.
  - date: 2026-08-11
    status: completed
    who: stkrolikiewicz
    note: >
      DEPLOYED and verified on production. API merged via #384; the UI
      ultimately landed via #388 (a #385 merge mishap sent the UI commits
      into the API branch instead of develop — rebased onto develop and
      re-opened). Release develop→master as PR #389 (Refs #374 #368),
      deployed 2026-08-11 ~08:00 UTC: Compute (44.8 s, three Lambdas +
      SOROBAN_RPC_URLS env) + SPA sync, bundle grep-verified armed.
      Prod verification: Code tab renders real decompiled Rust on
      CDU5…HD7G ("12 functions · 13 unresolved holes · 1 unknown value",
      SDK 22.0.7 chip), SAC contracts correctly get no tab; /health 200,
      indexer at head, DLQ 0. Issue #374 closed with a live link;
      Inferara notified. Future work stays recorded in README (recovery
      API on 0.0.5, SEP-41 badge, .wasm download, view-code links,
      behavioral equivalence) — backlog tasks to be spawned when picked up.
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

## Spike Results (2026-08-07) — full mainnet sweep

All **3,326 distinct wasm hashes** on mainnet (every contract), bytes
fetched live from RPC (`getLedgerEntries`, batches of 200 — **0 expired**),
decompiled with `soroban-ret-cli 0.0.4`:

- **Success: 3,310/3,326 hashes (99.5%); instance-weighted 99.98%** of all
  133,841 contracts. Rust-first validated decisively.
- **Timing: median 28 ms, p90 90 ms, p99 1.1 s**; long tail exists —
  slowest success 104 s (102 KB wasm). Endpoint timeout ~10 s covers p99
  with margin; the tail falls to the WAT fallback by design.
- **16 failures**, all niche (≤7 instances each): 8 timeouts >120 s
  (large wasms, 48–129 KB) and 8 `formatting error` cases where emitted
  Rust doesn't parse — two clusters (3× ~52 KB, 5× ~11.5 KB, likely two
  contract families). Hash list in spike `results.csv` — this is the
  failure corpus to share with Inferara.
- **Holes**: hole-free 350/3,310 (11%); `todo!()` per contract median 15,
  mean 58, max 3,075. Confirms counts-not-percentages UI guidance.
- Cross-check: `ae0da5a8…` (54,833 B) is Aqua Rewards from the soroban-ret
  benchmark corpus — our pipeline reproduces their input.

Methodology caveats: timings are CLI subprocess wall-time on an M-series
laptop (upper bound — the API will call the library in-process, and prod
CPU differs); first-run numbers include cold-start; hole counts are marker
strings (`todo!(`/`todo !(`/`var_N`) per the interim guidance, measuring
completeness, not correctness.

Spike artifacts persisted in `benchmark/` (fetch + sweep scripts, per-hash
`results.csv`); scripts are self-contained and re-runnable. Failure corpus
(16 hashes, two error classes) shared with Inferara 2026-08-07 together
with the full CSV.

## Open Points

- Issue template contents/mechanism — pending from Dominik. The button
  ships with a URL-prefilled body in the meantime; swapping it for an
  issue form is a one-function change (`reportIssueUrl`).
- ~~On-demand latency budget~~ — resolved by the sweep: decompile p99 is
  1.1 s, so a ~10 s endpoint timeout covers it 10×; the slow tail falls to
  WAT by design. Remaining knob: traffic threshold to revisit caching.
- Recovery-metrics source: release cut with `soroban_ret::recovery`
  requested (2026-08-07, along with GitHub Pages hosting for the benchmark
  report); if it lands before implementation, use the module — otherwise
  count markers on 0.0.4.

## Risks

- 0.0.x API churn — mitigated by pin + adapter.
- Complex contracts mostly don't compile clean (3/24 corpus) — expect
  readable Rust with explicit `todo!()` holes; UI copy must not overpromise.
  Hole counts measure completeness, not correctness. (Sweep note:
  display-level success is 99.5%, so this shapes hole density, not
  availability.)
- Every Code-tab open burns API CPU (repeat views repeat work). Guards
  shipped: per-request timeout + a semaphore bounding concurrent Rust
  decompilations. NO in-process rate limiting — that lives at the infra
  edge (Cloudflare / API Gateway); the earlier plan wording claiming a
  route-level limiter was never implemented and is corrected here.
  Caching only if traffic hurts.
- Expired/archived contract code → RPC can't serve bytes → Code tab
  unavailable (Interface unaffected).

## Implementation Notes (2026-08-10)

Two stacked PRs, 15 commits, verified against production data throughout:
a `local` API binary (borrowed from task 0199) bound to prod ClickHouse
over mTLS, the SPA dev proxy pointed at it, so every case below was
clicked on real mainnet contracts rather than fixtures.

**API — [PR #384](https://github.com/rumblefishdev/soroban-block-explorer/pull/384)**

- `runtime_enrichment::wasm_code`: `WasmCodeFetcher` (RPC pool, failover,
  sha256 verification of fetched code against the requested hash) and
  `decompile_on_blocking_pool` (soroban-ret behind a semaphore).
- Handler: StrKey validation → wasm_hash lookup → RPC fetch → decompile
  under a 10 s timeout; `LONG` cache header (output immutable per
  (hash, version)); new codes `wasm_fetch_failed` / `decompile_failed`.
- `?format=rust|wat`; Rust failures degrade to WAT _in the same response_
  (`representation: "wat"` + `rust_error`).
- `SOROBAN_RPC_URLS` added to the API Lambda env (same keyless pool the
  enrichment worker uses).

**UI — [PR #385](https://github.com/rumblefishdev/soroban-block-explorer/pull/385)**

- Separate **Code** tab (amber dot = experimental) beside an untouched
  Interface tab; hidden entirely for SAC / pre-upload contracts, and a
  stale `?tab=code` falls back to Interface.
- Prism syntax highlighting in a lazy chunk, sticky line-number gutter
  with click-to-highlight, copy on the block, `Download .rs/.wat`,
  prefilled report-issue link, Inferara attribution line.
- Completeness counters (counts, never percentages) and — added ahead of
  the 0.0.5 release — soroban-ret's own compliance diagnostics, grouped
  by (category, message).

## Issues Encountered

- **NUL byte in `ContractCode.tsx`** — a stray `\x00` inside a template
  literal made git classify the file as binary; PR #385 showed
  `Bin 14933 -> 18549 bytes` instead of a diff for several commits.
  Replaced with a space. Watch for `Bin` in `git show --stat` on text files.
- **Paused queries never resolve** — with the API host unreachable
  TanStack sets `fetchStatus: 'paused'` while `status` stays `pending`,
  so an `isPending`-first branch spins a skeleton forever (>60 s observed,
  no error, no retry). A paused fetch now takes the error path. Only
  reproducible by clicking; invisible in code review.
- **Timeout does not cancel `spawn_blocking`** — the decompilation kept
  burning a thread after the client got its 500 (the slow tail reaches
  100 s+). Fixed by moving a semaphore permit into the blocking closure.
- **npm 11 vs CI npm 10 lockfile** — adding prismjs with the local npm 11
  pruned nested entries CI requires; every job died at `npm ci`. Repaired
  with `npx npm@10.9.4 install --package-lock-only`.
- **`git add -A` scope leak** — swept an unrelated WIP file and the
  untracked dev binary into a commit; rewritten before review.

## Design Decisions

### From Plan

1. **On demand, no cache** — output is deterministic per (wasm_hash,
   version), so a cache is a drop-in later; the sweep (p99 1.1 s) said it
   was not needed to launch.
2. **Counts, not percentages** — per Georgii; the sweep's own spread
   (median 15 holes, mean 58, max 3075) is the evidence a single global
   number would misrepresent.
3. **Separate Code tab, Interface untouched** — reverted the earlier
   master-detail merge idea at the user's call.

### Emerged

4. **Auto-fallback to WAT on `decompile_failed`** — the API already
   advises "retry with format=wat"; the UI performs it instead of showing
   an error wall. The user sees code plus a "WAT only" chip.
5. **Rust toggle disabled when the contract has no Rust** — showing WAT
   under a selected "Rust" toggle was two contradictory signals.
6. **Failure attribution split** — soroban-ret's diagnostics are quoted
   verbatim; our timeout is labelled as ours in both the UI and the
   prefilled issue, so upstream reports never blame the decompiler for a
   SorobanScan limit.
7. **Single report CTA with one payload builder** — two buttons rendered
   at once with different bodies, and the one users actually click was
   missing the decompiler version.
8. **Limits on rendering** — highlighting skipped above 400 KB, gutter
   above 10k lines (a real mainnet WAT is 2.1 MB / 57k lines).
9. **Empty RPC result is not proof** — every endpoint in the pool must
   agree before returning 404 "not live"; the sweep found 0 archived
   binaries, so one empty answer is more likely a lagging node.
10. **SDK chip trimmed at `#`** — `contractmetav0` stores
    `<version>#<40-char sha>`; the sha moved to a tooltip.

## Future Work

- `soroban_ret::recovery` (per-function statuses) once 0.0.5 ships —
  replaces the marker-string counting and enables per-function badges.
- SEP-41 badge: compute from our own `wasm_interface_metadata`, not from
  the decompiler's `standard_interfaces` (available always, no RPC, works
  on lists) — belongs on the contract header, not this tab.
- `.wasm` download (needs a small endpoint serving raw bytes).
- "View code" links from Interface rows into the Code tab.
- Behavioral-equivalence badge — hosted/batch only, deliberately deferred.

## Acceptance Criteria

- [x] Spike results documented in task notes (success rate, hole density,
      timing over top prod hashes) and failure corpus shared with Inferara.
      (Full-mainnet sweep 2026-08-07; see Spike Results + `benchmark/`.)
- [x] `GET /v1/contracts/{id}/decompiled` with timeout, pinned `=0.0.4`
      behind an adapter module; unit tests + an `--ignored` live-RPC smoke
      test. (PR #384. Rate limiting deliberately NOT in-process — see Risks.)
- [x] Code tab shipped with full fallback ladder, Experimental marking,
      banner, counts, copy/download, report-issue button, attribution line.
      (PR #385, plus syntax highlighting, line numbers and diagnostics that
      were not in the original plan.)
- [x] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      lists the endpoint (PR #384), per ADR 0032.
- [x] **API types regenerated** — `DecompiledResponse` +
      `DecompileDiagnostic` in `libs/api-types` (both PRs).
- [x] Merged and deployed; issue #374 closed with a link to a live
      contract page (issues close at deploy, never at merge).
      (Release PR #389 → master, deployed + prod-verified 2026-08-11;
      #374 closed the same day.)

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
