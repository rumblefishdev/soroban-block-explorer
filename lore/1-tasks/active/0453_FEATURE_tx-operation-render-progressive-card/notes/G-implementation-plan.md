---
title: 'Operation card — governing spec: decisions, waves, acceptance criteria'
type: generation
status: developing
spawned_from: ../README.md
spawns: []
tags: [frontend, transaction-detail, ux, spec]
links:
  - 'https://github.com/stellar-expert/ui-framework'
  - 'https://github.com/stellar-expert/tx-meta-effects-parser'
history:
  - date: '2026-07-29'
    status: developing
    who: karolkow
    note: >
      Written after the from-zero design pass: full-context analysis (code map,
      per-op-type details inventory, satellite boundaries), live benchmark
      (stellar.expert, stellarchain.io, Solscan), three interactive prototypes,
      UX literature check, and a deep dive (SE open-source templates, Blockscout
      interpretation, Tenderly trace). Supersedes the render sketch in
      0359/notes/S-tx-render-audit.md as the governing spec — the audit remains
      the finding record.
---

# Operation card — governing spec

This note is the single source of truth for the 0453 redesign. The 0359 audit
(`S-tx-render-audit.md`) stays valid as the _problem_ record; where this spec
and the audit disagree on the _solution_, this spec wins. Reason: the audit's
sketch predates the data-reality corrections below (D9, D7).

## Layout decision (settled 2026-07-29, with Karol)

**One operation card in a master-detail frame. The left index (picker) renders
always** — an "if 1 op, hide the index" adaptive condition was considered and
deliberately deferred as an optional future slice, to avoid perceived variant
split. The card is identical regardless of the frame — the index is navigation,
not a second render.

The normal/advanced **toggle survives until the card reaches parity** (nothing
advanced shows may be lost — inherited from 0071's "never hide values"). Its
fate is decided in the last wave, on evidence: remove, or demote to an
Etherscan-style _preference_ ("always show raw fields") that adds rows to the
same page rather than switching worlds.

## Decisions (numbered; each with the reason it holds)

- **D1 — Truth-first sentence per op type.** A summary must be true before it
  is useful (audit finding #1, Critical). No op renders "{label} processed".
- **D2 — Sentences are data, not code.** Table-driven `type → template`
  mapping (like stellar.expert's `typeMapping`), unknown type → warn + safe
  fallback, never a crash. Port templates from
  `stellar-expert/ui-framework` `tx/op-description-view.js` (MIT — keep an
  attribution comment). Do not invent 23 sentences from scratch.
- **D3 — Dual-tense templates.** Each template renders factual tense
  ("sent X to Y") and intent tense ("attempted to send X to Y") — SE's
  `isEphemeral` pattern repurposed for failed transactions. Kills the
  benchmark-wide bug (SE and stellarchain both narrate failed ops as if they
  happened).
- **D4 — The flow tree dies; the card replaces it.** Its honest content today
  is three boxes saying what one sentence says; its nested-call branch reads
  six `details` keys no backend ever emitted (0442). Resolution recorded here:
  _delete the dead branches_ (wave 0) and later feed a real call tree from
  `heavy.operation_tree` (wave 4) — the third option 0442 did not know about
  (the API already delivers the tree; the FE never reads it).
- **D5 — Verdict is transaction-level, in the summary banner.** Stellar is
  atomic; per-op verdicts do not exist in the API today (0352 Step 6 owns
  them). The Result node's lie (0444, issue #364) is closed by the banner +
  dimmed "attempted" cards, not by patching the node. Recoverable material in
  commit `d5444023`.
- **D6 — Per-op icons: adopted** (0257's orphan spec resolved _implemented_,
  wave 3). Both Stellar explorers and Solscan use them; in a card list they
  carry the scan.
- **D7 — Events stay transaction-level in v1.** The parser discards the CAP-67
  V4 per-op index, so per-card events are impossible without backend. Approved
  micro-backend (wave 4): emit `op_index` on `XdrEventDto` + types regen; then
  events move into the owning card. Until then the events section labels
  itself transaction-scoped.
- **D8 — Claim-CB asset: candidate micro-backend.** Asset is already parsed
  into `asset_appearances` from LedgerEntryChanges but not emitted into
  `details`. Until landed, headline says "claimed balance {id}" (same as SE).
- **D9 — Received amount: honest empty slot.** NOT derivable from
  `claimedAtoms` in general — the parser filters to LP atoms only
  (`operation.rs claim_lp_atoms`), order-book fills drop, failed txs get
  nothing, and the result-side `SimplePaymentResult.last` is never extracted.
  The card ships a "Received —" slot that lights up when the net_settled read
  path lands (0393/0411/0419 track — Karol: in indexer+DB already, frontend
  unfinished, historical backfill pending; explicitly NOT a blocker and NOT a
  dependency of any wave here).
- **D10 — Route strip is the WOW, inside the card.** Chips
  `account → pool → pool → account` with per-hop amounts where data exists
  (`details.path` + `claimedAtoms`), red/dashed edge for the known order-book
  gap. Full-page graph rejected as primary view (87% of txs have 1 op —
  measured; graph degenerates to two dots); may return later as a lens tab
  after D7.
- **D11 — Call tree with failure pinpoint.** `operation_tree` nodes carry
  `successful` per node — render the failing nested call highlighted
  (Tenderly-lite). No Stellar explorer has this.
- **D12 — Story chip (wave 3.5).** One heuristic classification for the whole
  tx ("Swap", "Arbitrage · 4 ops", "Mint") from op types — stellarchain/
  Blockscout pattern, no LLM.
- **D13 — Balance-changes section: parked behind net_settled.** Wallet-style
  per-asset deltas; natural consumer of `net_settled` once its read path +
  backfill are done. Separate slice, never a blocker.
- **D14 — Effects engine: separate future task.** Our Rust
  `ledger_entry_changes` parser already computes what SE derives client-side
  via `tx-meta-effects-parser`; exposing it would give a true Effects section
  (including order-book hops — the proper closure of the D9 gap). Out of 0453.
- **D15 — Degraded mode is cheap.** `parse_error` = 0 across 3.9B prod rows;
  `heavy == null` only on live archive-fetch failure. Card renders from light
  fields + one "details unavailable" note. No deep investment.

## Waves (each independently shippable; commit per wave at most)

| Wave    | Content                                                                                                                                                                                                                             | Size                      | Acceptance criteria                                                                        |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------ |
| **0**   | Dead code out + key fix (see list below)                                                                                                                                                                                            | XS                        | tests green; invoke ops show `Called fn() on C…`; no behavior change otherwise             |
| **1**   | Sentence coverage: port SE templates for all 27 op types into table-driven `opHeadline` (D1/D2/D3); change_trust names asset+issuer (issue #370 case)                                                                               | S×N (splittable per type) | no op renders "processed"; #370 wording present; tests per type                            |
| **2**   | Verdict honesty: tx summary banner states failure (+ `heavy.result_code` when present); failed ops dim + intent tense; Result node relabeled or removed with the tree                                                               | S                         | issue #364 face closed: failed tx can never read as successful                             |
| **3**   | `OpCard` in the master-detail frame (index always visible — layout decision above); card = icon + headline + facts grid + per-card "operation details" disclosure absorbing today's advanced per-op dump; per-op deep links `#op-N` | M                         | toggle still present; nothing advanced showed is lost; mobile: index collapses above cards |
| **3.5** | Story chip (D12)                                                                                                                                                                                                                    | XS                        | tx-level chip for recognized shapes; absent otherwise (never wrong)                        |
| **4**   | Route strip in swap/offer cards + call tree from `operation_tree` with failure pinpoint (D10/D11); micro-backend `op_index` on events (D7) → events into cards; API types regen                                                     | M                         | strips render only where data exists; failing nested call visibly marked                   |
| **5**   | Parity audit vs old advanced; toggle fate decided (remove vs preference, D5 in layout section); `/ux-expert` regression pass (umbrella AC); docs update per ADR 0032                                                                | S                         | 0453 umbrella acceptance criteria all resolved                                             |

Out of scope, tracked elsewhere: ScVal typed rendering (0363 — the card only
reserves the slot), fail-reason decoding backend (0352), pool-id links (0305 —
boundary: this task renders the route _assets_, 0305 makes crossed _pools_
clickable), net_settled read path (0411), effects engine (D14 → future task).

## Wave 0 — exact list (the "pewniaki")

1. `web/src/pages/transaction-detail/normal/humanizeOp.ts:66` — read
   `functionName` (camelCase, what `crates/xdr-parser/src/operation.rs:507`
   emits) instead of dead `function_name`; add test. Same fix pattern as
   `OperationJsonDetail.tsx:128-131` (found once already, this file was
   missed). Closes the 0380 key-mismatch half; u256/i256 decode stays in 0380.
2. `humanizeOp.ts:72-79` — delete `summaryFromHeavy` early-return
   (`details.summary` — no producer; latent override trap flagged by 0442).
3. `web/src/pages/transaction-detail/normal/toFlowNodes.tsx` — delete the
   unreachable nested-invocation machinery: `buildNestedChildren`,
   `contract_label` / `invocations` / `destination_summary` readers,
   `summary_line_1/2` in `buildResultSummary`. KEEP the reachable invoke root
   node (`light.contract_id` + `functionName` + `Invoke` connector) — after
   fix (1) it starts labeling properly. Closes 0442 by deletion (D4).
4. `web/src/pages/transaction-detail/index.tsx:109-116` +
   `advanced/RawDataSection.tsx` — delete the `results_meta_xdr` cast/row; the
   field does not exist in the API (openapi note: intentionally not surfaced).
5. `web/src/pages/transaction-detail/sections/OperationPicker.tsx` — delete
   the dead `subtype` filter (never assigned by `buildOperationEntries`;
   renders a filter with only "All types"). A live type filter may return in
   wave 3 with real data (p99 = 28 ops).
6. `advanced/OperationJsonDetail.tsx:135` — delete the `details.auth` reader
   (no producer).

Explicitly NOT in wave 0: `EventsSection` kind-from-array → `event_type`
switch. Checked and rejected as a pewniak: the diagnostic array carries
byte-identical _copies_ of contract events (task 0182), whose inner
`event_type` says "contract" — switching the chip source would mislabel
copies. Real fix belongs to the wave-4 events work (dedupe/copy handling).

## Data-reality constraints the card must respect (from the analysis session)

- One endpoint (`GET /v1/transactions/{hash}`); light ops are folded, heavy
  ops are 1:1 with the envelope; `operationEntries.ts` already reconciles.
- All amounts in `details` are raw i64 stroops as JSON numbers (>2^53 hazard
  documented in `humanizeOp.ts:24-34`); classic assets are always 7 decimals.
- Two pool-id formats in one response: light `pool_ids` = `L…` strkey,
  `details.poolIds`/`liquidityPoolId` = hex.
- `claimedAtoms`: LP-only, success-only (D9).
- Events: decoded typed-JSON ScVals, tx-scoped (D7); `soroban_events` on
  light is populated only when heavy is unavailable.
- `heavy.operation_tree`: array of per-auth-entry trees; safe to attach to
  the invoke card (protocol 21+: one invoke op per tx).
- Fee-bump: `heavy.fee_bump_source` + `light.inner_tx_hash` exist and are
  currently unread — summary should show them (benchmark: SE does).

## Open items (tracked, not blocking)

- Usage telemetry for the toggle decision (wave 5) — product call, none today.
- Mobile spec for wave 3 (index collapse, strip wrap).
- Heavy-fetch latency (skeleton quality; p95 unmeasured).
- Filter revival for 28+-op txs (wave 3 decision).
- A11y: extend `XdrRow`'s `role=button`/`aria-expanded` pattern to all
  disclosures (0363's instruction).

## Benchmark record (live, 2026-07-29)

stellar.expert = variant A shape (sentence list + effects expansion; no
Soroban events, no failure reasons, past tense on failed). stellarchain.io =
variant B shape (master-detail, per-op status chip, effects tab, tx
classification). Solscan = A + lenses (summary w/ USD, per-instruction raw,
List/Tree, balance-changes tab). Tenderly/TON viewer = trace-first (G).
Our differentiators on top of SE's foundation: decoded events, call tree with
failure pinpoint, failure reason, story chip.
