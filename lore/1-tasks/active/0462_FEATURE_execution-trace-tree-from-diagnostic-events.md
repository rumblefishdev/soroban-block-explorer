---
id: '0462'
title: 'FEATURE: execution trace — nested call tree from diagnostic events on the operation card'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0453', '0456', '0363', '0352']
tags: [frontend, transaction-detail, soroban, ux, priority-high, effort-medium]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from the post-ship review of 0453: the Events section shows
      the nested execution (fn X calls Y, Y calls Z, return, Z2, return, Y2…)
      as a flat raw table — investigate rendering it as a readable indented
      tree with per-node summaries, "0453-style wow". Investigation done
      same day: fully feasible client-side, no backend needed.
  - date: '2026-07-30'
    status: active
    who: karolkow
    note: 'Green-lit for implementation, FE-only scope.'
---

# FEATURE: execution trace — the real nested call tree, rendered readably

## Summary

Render the actual execution of a Soroban transaction as a collapsible,
indented call tree on the invoke operation card — every call with its
function name, called contract, args/return behind a per-node disclosure,
and the contract events (transfer/mint/burn/trade…) attached to the exact
call that raised them. Replaces reading a flat 100-row raw event table.

## Verified findings (2026-07-30, live prod data)

Fixture: `54aab000933f772dcf93529ccb6e30df92622cdcc5dc933956afdd4333b21f2d`
(DeFi `swap_collateral`, 117 diagnostic events).

1. **The data is already on the frontend.** `heavy.diagnostic_events`
   contains `fn_call` / `fn_return` pairs; a simple stack walk (push on
   `fn_call`, pop on `fn_return`) reconstructs the execution tree exactly —
   verified: 40 calls, depth 5, siblings and returns in the right places.
   No backend change, no new endpoint, no indexing.
2. **Events are position-attachable.** The diagnostic stream carries the
   contract events as in-order copies between their surrounding
   `fn_call`/`fn_return` (verified: `burn` at event_index 22 sits inside
   the `burn_scaled_and_transfer_to` call at 21). So each transfer/mint
   can render as an effect leaf under the call that raised it.
3. **Node shape:** `fn_call` topics = `[sym "fn_call", bytes <called
contract, 32B>, sym <fn name>]`, `data` = args; `fn_return` `data` =
   return value; the event's own `contract_id` = the CALLER (null at
   root). The called-contract bytes need a small strkey (C…) encoder in
   the web app — the only new utility required.
4. **Noise to filter:** `core_metrics` diagnostic events (19 of 117 here)
   are host counters — exclude from the tree (optionally sum as a
   footer, stellar.expert shows them as resource stats).
5. **Competitors don't do it readably.** stellar.expert has the same data
   but renders ~200 flat mono lines with barely-visible `↳` indents, no
   summaries, no collapsing, effects dumped separately below.
   stellarchain shows nothing nested. An Etherscan/Tenderly-style
   readable trace would be genuinely differentiating.
6. **Relation to the auth tree:** the card's current "Authorized calls"
   (`operation_tree`) is intent, not execution, and covers only
   auth-requiring calls (~53 % of Soroban txs have zero rows — see
   `invocation.rs` module docs). When diagnostic events are present the
   execution trace supersedes it visually; keep the auth tree as a
   fallback when they are absent.
7. **Failed transactions:** the pre-trap trace is present (see
   `diag_independent_count.rs` cases) — the tree can truthfully mark
   WHERE execution stopped, which 0444/0456 established the auth tree can
   never do. Pairs with 0352's reason banner.

## Implementation sketch

- Pure builder `buildExecutionTrace(diagnosticEvents)` → nodes
  `{fnName, calledContract, caller, args, returnValue, children, events}`;
  unit-test against the fixture (stack walk, event attachment,
  unbalanced-trace tolerance for failed txs).
- Tree component on the invoke op card ("Execution trace"), reusing the
  0453 vocabulary: indented rows, OpAvatar-style glyphs, per-node
  DisclosureRow with `HighlightedJson` args/return (raw always one click
  away), depth >2 collapsed by default with "N calls inside" badges.
- Event leaves humanized via the 0363 event templates when those land;
  raw chip until then.
- Contract display names (0460 #1) slot straight into node headers.

## Acceptance criteria

- [x] Invoke card shows the execution tree for any tx with diagnostic
      events; auth-tree fallback otherwise (fallback covered by unit test —
      no live pre-P23 fixture hunted down)
- [x] Verified live on the fixture: 40 calls, depth 5, events attached to
      the right nodes (burn/trade/swap_exec at their call sites),
      `core_metrics` excluded, deep branches folded with "5 calls" badges
- [x] Args/return reachable per node (collapsed `<>` toggle), full raw
      Events table unchanged below
- [x] Failed-tx trace renders with a truthful stop marker — verified live
      on 9bdcaba0… (0352's Contract-error fixture): `work(3)` carries the
      `error`/`log`/`host_fn_failed` diagnostic events plus a red
      "stopped here" chip — the failure reason lands at the stop point for
      free
- [x] Docs: frontend-overview §6.4 updated (execution trace primary,
      authorized-calls as fallback)

## Implementation notes (2026-07-30)

- One file, `web/src/pages/transaction-detail/op-card/ExecutionTrace.tsx`:
  builder (`buildExecutionTrace`, `contractStrkeyFromBase64`,
  `traceCallCount`, `traceEventLabel`) + tree component, mirroring the
  RouteStrip/CallTree pattern. Tests: `ExecutionTrace.test.ts` (builder,
  strkey against the real byte pair, unbalanced traces, noise tolerance)
  plus an OperationCard render test. 70 web tests green; typecheck, lint
  clean.
- `OperationCard` renders "Execution trace · N calls" for invoke ops and
  parses the auth tree only when the trace is empty; `OperationsSection`
  passes `heavy.diagnostic_events` through.

### Emerged decisions

1. **Builder merged into the component file, not a sibling module.** The
   initial split (`executionTrace.ts` + `ExecutionTrace.tsx`) differed
   only by case — on macOS's case-insensitive filesystem Vite resolved
   `./ExecutionTrace.js` to the builder file and the component rendered as
   `undefined`. Same-name-different-case siblings are a trap; the repo
   idiom (RouteStrip, CallTree) already co-locates builder + component.
2. **Hand-rolled 60-line strkey encoder** instead of adding
   `@stellar/stellar-sdk` as a web dependency — one function, verified
   byte-for-byte against a known pair; the SDK would be a heavyweight dep
   for exactly this.
3. **Error diagnostics attach like any other event** — no special-casing:
   on failed traces the host's `error`/`log` events naturally sit on the
   unfinished call, which is precisely where the reader looks.

## Review round (2026-07-30, same day)

- **Two `error` chips explained + grouped**: a failing call carries the
  contract's own error event AND the host's "escalating error to VM trap"
  copy — both topic `error`. Truthful but reads as a bug; same-label
  neighbours now group into one `error ×2` chip (full data stays in the
  Events table).
- **Mid-trace failure fixture found** (`ec87214f…`, live prod):
  `harvest` → nested `harvest` → deeper call; "stopped here" marks the
  whole unfinished stack path down to the trap point.
- **Route strip on a FAILED path payment** (`872c6b15…`): no amounts and
  no note — the old `hasFills` guard assumed no-fills meant "don't blame
  the order book", which also silenced the all-order-book successful case.
  Replaced with `applied`-based notes: applied+partial → order-book note
  (now also covers all-OB routes); failed → "Route as signed — the
  transaction failed, so no exchange was executed." `hasFills` removed.
- **Skeleton parity**: Operations ghost now uses the live grid ratio
  (md 5fr/7fr, lg 4fr/8fr) and Events/Raw-data ghost cards were added.
- **Claimable-balance claimants**: parser emits only a count — recorded as
  0460 item 14 (backend), not fixable here.
- **Args format**: fn(N) counts vs literal inline args — options analysis
  delivered; decision pending (0460 #11 territory if inline).

## Round 5 — unified rows (reviewer's design, 2026-07-30)

The proposal: events stop being chips and become FIRST-CLASS ROWS,
nested exactly like calls. Research before building:

- **Phalcon (BlockSec) invocation flow**: "each node represents a function
  call or event trigger" — the industry precedent for exactly this model.
  Tenderly likewise interleaves log opcodes into the decoded call trace.
  stellar.expert renders invocations and effects as two separate flat
  lists — the anti-pattern we're leaving.
- **Library check**: `@mui/x-tree-view` rejected deliberately — the
  WAI-ARIA tree pattern expects one focusable per item, and our rows carry
  a link + two buttons each; forcing them into `role="tree"` would degrade
  a11y, and the repo has no `@mui/x-tree-view` dep today. Hand-rolled
  disclosure rows keep correct semantics. No trace-specific React library
  fits a MUI design system; the reusable "gotowiec" here is the EVENT
  FORMATTER, which 0363/0457 will source from stellar-expert's MIT
  tx-meta-effects-parser — until then a local ~40-line formatter covers
  token events, error diagnostics (quoted message + code inline!) and a
  generic elided fallback.
- **Implementation**: `TraceNode.children` is now a single stream-ordered
  list of `{kind:'call'}|{kind:'event'}` — chronology preserved (a
  transfer firing between two sub-calls sits between them; chips used to
  flatten that). Event rows: dot glyph, category-coloured name (blue
  token / red diagnostics / grey protocol), inline payload
  (`transfer(GC4Q…K7XQ, CCTU…V6J7, 13171)`), `by EMITTER` only when the
  emitter differs from the surrounding call's contract, per-row raw
  topics/data disclosure. Folded-branch badge counts calls AND events
  (an events-only branch used to say "0 calls"). Legend updated.
- Also this round: adaptive index (1 op → no picker, 0460 #5), `→ void`
  explicit (secondary tone — full truth, not a UI abbreviation), literal
  values in secondary tone, flexbox indent bug fixed (spacer squeezed to
  zero on overflow).

## Round 6 — pre-P23 research + emitter-match (2026-07-30)

Question from review: "old txs show no events in the trace — can nothing
be done? how does stellar.expert cope?"

- **Emitter-match fallback SHIPPED**: old-format (tx-level) events attach
  to the trace when exactly ONE call targets the event's emitter —
  provable, never guessed; ambiguous emitters and unmatched ones (e.g.
  tx-level `fee` events raised by a non-callee, seen on the `f82f4e90…`
  fixture) stay in the Events table. Runs only when the stream carried no
  event copies, so new-format traces cannot double-attach. 3 tests.
- **Empirical limit found**: a 12-tx scan around ledger 51M — most pre-P23
  archive metas have NO diagnostic events at all → no trace exists to
  attach anything to. Fixture `a404570a…` (7 transfer events, diag 0).
- **stellar.expert verified live on that fixture**: ZERO invocation lines
  (they cannot build the tree either) but a full effects list ("54.9 AQUA
  debited from GD6X…") computed from ledger-entry changes — era-proof.
  That mechanism is exactly task 0457, whose priority was raised with
  this evidence (see its Summary).
- Resulting truth tiers: (1) new tx → full chronological trace; (2) old
  tx with a trace → trace + emitter-match; (3) old tx without a trace →
  auth fallback + (0460 #4 note) until 0457 renders meta effects.
- Also this round: story chip moved to the page title (0460 #9 shipped),
  copy buttons on every identifier in the trace, canonical CODE:ISSUER
  asset strings link from the JSON viewer, Lab deep links verified on the
  10,920-char envelope.
