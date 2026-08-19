---
id: '0363'
title: 'FEATURE: tx-detail — decode ScVals to typed chips (kill the raw-JSON wall)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0352', '0071', '0453', '0462']
tags:
  [frontend, transaction-detail, soroban, ux, priority-medium, effort-medium]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/378']
history:
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Advanced-mode tx detail dumps raw ScVal JSON ({type,value}) for event
      topics/data and operation args — a ~13,000px wall. Audited our components
      + researched how mature explorers do it (stellar.expert = same-chain
      reference, Etherscan/Solscan). Everyone decodes to typed/human; raw is
      secondary. We're the outlier. This task moves us to decoded rendering.
      Shares the ScVal decoder with 0352 (error chip) — do together.
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Issue #378 triaged into this task as scope item F (event taxonomy).
      Same section, same "unreadable wall" complaint, different axis from
      the decoder: no ScValView needed, ships independently, client-side
      only. Two corrections during review the same day, both recorded in
      F: (1) the reporter's grouped sub-categories were rejected — this
      section is the chronological record and grouping destroys order, so
      the shape is filter-plus-counts over one in-order list; (2) the
      system-events-mislabelled-as-Contract fix must NOT re-split the
      arrays on event_type, which would resurrect the 0182 double-count —
      only the chip label changes.
  - date: '2026-08-05'
    status: backlog
    who: karolkow
    note: >
      F merged (PR #379, 39111c44) — NOT deployed; backend and frontend must
      ship together or the Where column reads "—". Acceptance criteria
      reconciled against what actually shipped: the filter-toggle design was
      abandoned mid-implementation once the 27 rows proved to be two records
      rather than one noisy list, so three criteria were rewritten rather
      than ticked, and both "N/A" gates (docs, api-types) were wrong and are
      now done. Two late commits recorded: c3679237 (resource meter made
      total + widened to all Soroban ops) and fe70d7a0 (the disclosure
      claimed the debug channel copies every event — real meta says 1 of 3).
      Design decisions logged: raw diagnostics table kept by ruling after a
      9-transaction measurement showed 0 unplaceable entries; Copy chip tried
      and reverted; stage is informational only. New debt in Future Work: the
      trace renders the diagnostic copy, not the consensus event.
      Task stays backlog — A, B and C-lite's semantic line are open.
  - date: '2026-08-19'
    status: backlog
    who: karolkow
    note: >
      Re-checked against develop. NOT superseded — 0453 and 0462 deliberately
      wrote around this task and defer to it: 0453 README "this task decides
      WHERE the decoded events live; 0363 decides HOW they render", and 0462
      "event leaves humanized via the 0363 event templates when those land;
      raw chip until then". Since 2026-08-05: F deployed (issue #378 closed,
      production tags 2026.08.17/18); A landed under 0453 wave 5 (Events
      section collapsed by default); the fn(args) to result sub-call tree
      landed as 0462's execution trace. What is left is B and C-lite's
      semantic line — still zero ScValView in the tree, HighlightedJson still
      the renderer at five call sites including inside 0462's trace nodes.
      New implementation route recorded from 0462: source the event formatter
      from stellar-expert's MIT tx-meta-effects-parser rather than hand-rolling
      it; 0462 ships a local ~40-line stopgap until then. Stale references
      corrected in the body (the advanced/ path, the "advanced mode" framing —
      that split no longer exists).
---

# FEATURE: decoded ScVal rendering in tx detail

## Summary

The transaction-detail view renders Soroban event **topics/data** and operation
**arguments** as raw, pretty-printed ScVal JSON (`{type,value}` trees) via
`HighlightedJson`. A tx with ~30 events was a ~13,000px scroll wall where the
meaning is buried under JSON scaffolding.

> **Two framing corrections, 2026-08-19.** There is no "advanced" view any more —
> 0453 replaced the Normal/Advanced split with one progressive page, so this is
> now the only view. And the wall is collapsed by default (0453 wave 5), so the
> pain is on expand rather than on load: less urgent, equally unreadable. The input is already a **typed
> discriminated union** (`{type, value}`) — so a type-aware renderer is
> straightforward. Adopt the pattern every mature explorer uses: **decoded typed
> chips as the primary view, raw behind a toggle/collapse.**

## Current state (what we have vs the options)

Options considered (A ⊂ B ⊂ C; D=lib, E=raw):

| Option                                    | Have it?                   | Evidence                                                                                                                                                                                                                                          |
| ----------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **E — raw JSON** _(baseline)_             | ✅ this is us now          | Events `topics`+`data`, ops `args/return/auth` → `HighlightedJson` = blind `{type,value}` dump (`web/src/pages/transaction-detail/op-card/HighlightedJson.tsx`).                                                                                  |
| **A — collapse / progressive disclosure** | ✅ done, under 0453 wave 5 | Was: raw-XDR section only (`XdrRow.tsx`), Events/Operations always fully expanded. Now the Events section is collapsed by default — `EventsSection.tsx` ("a fully expanded raw-JSON table is a wall of pixels"). Delivered by 0453, not here.     |
| **B — semantic ScValView**                | 🟡 fragments only (~10%)   | `inlineScalar` (bool/number/short-string → Chip) in `OperationJsonDetail.tsx`; `IdentifierDisplay` only on the Contract column; `categoryChip` Soroban/Classic. No `switch(.type)` renderer — addresses/amounts INSIDE topics/data/args stay raw. |
| **C — "assets moved" / event decode**     | 🟡 trace only              | 0462's execution trace inlines a per-event summary (`transfer(GC4Q…K7XQ, …)`, `error("failing with contract error", 7)`) from a local ~40-line stopgap formatter. No semantic line in the Events section, no tx-level "assets moved" summary.     |
| **D — JSON-viewer lib**                   | ❌ no                      | `HighlightedJson` hand-rolled.                                                                                                                                                                                                                    |

**Was (2026-07-07):** E + a slice of A (raw-XDR only) + scattered B.
**Now (2026-08-19):** A is done (0453), the call tree is done (0462), C exists only
inside the trace. **The open target is B + C-lite's semantic line.**

## Reference — how mature explorers do it (researched)

- **stellar.expert (same chain, open-source — north star):** ScVals rendered as
  **typed chips with a small dimmed type subscript** (`0i128`, `164266u32`,
  `"mint"sym`, `…bytes`); maps/vecs as `{key: val}` / `[…]`. Invocation shown as
  a **function signature `fn(arg,arg) → result`** with a **nested sub-call tree**
  (each sub-call's return after `→`); an ⓘ tooltip fetches the WASM interface to
  show **named parameters**. Events decoded **twice**: a semantic line
  (`0.55 KALE minted`) + the decoded `["mint"sym, …] with data …i128` (topics vs
  data separated). Zero raw XDR embedded — links out to Stellar Lab/Horizon for
  raw. Addresses truncated + identicon + clickable + known-name label (`[Kale]`).
- **Etherscan / Solscan / Blockscout:** decoded is the **default**, raw/hex is a
  toggle or a tab labeled "debugging"; a plain-language **"assets moved" summary
  line** at the top (Tokens Transferred / Balance Change), decimal-adjusted +
  logo + truncated links; typed **named param tables** (`#/Name/Type/Value`) with
  the resolved signature as header; **prominent decoded failure reason** in the
  status row (red "Fail with error 'X'"); verbose logs collapsed by default.
- **Architectural note:** StellarChain's invocation trace uses **ephemeral
  Soroban RPC → disappears for old txs.** stellar.expert (and us, via `heavy`
  archive-XDR fetch, ADR 0029) reconstruct from **persisted** meta → always
  available. Our foundation is on the robust side; keep it.

## Target design (A + B + C-lite)

### B (core) — `<ScValView value>` component

A recursive, type-aware renderer replacing `HighlightedJson` wherever a ScVal
enters. Our decoder emits these `type`s: `sym, string, address, bool, bytes,
i128/u128/i64/u64/i32/u32, timepoint, duration, vec, map, error, void,
contract_instance, ledger_key_nonce, ledger_key_contract_instance`.

| ScVal type                  | Render                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------- |
| `sym`, `string`             | text (event `topics[0]` sym → name badge)                                             |
| `address`                   | `IdentifierDisplay` (truncate 4…4, clickable; account vs contract by G/C)             |
| `i128/u128/i64/u64/i32/u32` | mono number, grouped / `formatCompactAmount`, full in tooltip, small `type` subscript |
| `bool`                      | `Chip`                                                                                |
| `bytes`/hashes              | mono truncated 4…4 + copy                                                             |
| `timepoint`/`duration`      | human time / duration                                                                 |
| `error`                     | error chip `Type/Code` — **shared with 0352**                                         |
| `void`                      | `—`                                                                                   |
| `vec`                       | scalars → inline `a, b, c`; structured → nested rows (collapse)                       |
| `map`                       | `key → value` pairs                                                                   |

Keep a per-node/section **"raw"** affordance that falls back to today's
`HighlightedJson` (devs keep the raw).

### A — collapse Events/Operations — ✅ DELIVERED (0453 wave 5)

> Done outside this task. The Events section is collapsed by default —
> `EventsSection.tsx`: "Collapsed by default since this section is on the
> one-and-only view now (0453 wave 5) — a fully expanded raw-JSON table is a
> wall of pixels." What remains of the original wording below is the _decoded
> summary line_ on the collapsed row, which needs B and belongs to C-lite.

Original scope, for reference: wrap each event / operation in the `XdrRow`-style
collapsed row: decoded summary line visible; expand for the full decoded detail;
raw JSON one more click in. Default collapsed for large sets (mirror
stellar.expert's "Show operation details" / N-more spoiler).

### C-lite — semantic layer

- **Events:** for known shapes (`transfer/mint/burn/approve`, SAC) render a
  one-line summary `Transfer 326,131,711 · GX…4P → GY…8K` above the decoded
  topics/data.
- **Invocation:** ✅ **delivered by 0462** as the execution trace — `fn(args) →
result` with the nested sub-call tree and return values. Do not rebuild.
- **Named params (bonus):** we ingest ABIs (`wasm_interface_metadata`) → map
  positional args to `#[contractfn]` param names (stellar.expert's ⓘ pattern).

**Implementation route for the event formatter (recorded from 0462, 2026-08-19).**
Do not hand-roll it. 0462 investigated the same problem and concluded the
reusable piece is the EVENT FORMATTER, to be sourced from stellar-expert's MIT
`tx-meta-effects-parser`; it shipped a local ~40-line stopgap covering token
events, error diagnostics and a generic elided fallback, explicitly "until
0363/0457 land". So the C-lite semantic line starts from that library, and the
stopgap in `ExecutionTrace.tsx` is what it replaces. 0457
(effects-from-ledger-entry-changes) is the sibling consumer of the same parser —
check it before starting so the two do not adopt it twice.

### F — event taxonomy (issue #378, added 2026-08-04)

A second, independent axis on the same wall: even fully decoded, the Events
section is **one flat undifferentiated list**. The reported transaction
(`0a120260bb0fbe48903291b8606b3058fbfb95defd45e02e12cf0361ec6dc38e`) carries
27 events, of which most are diagnostics, and most of those are host resource
counters. A reader looking for what the contract actually did has to sift.

**Ask:** separate contract events from diagnostic events as expandable
sub-groups, and within diagnostics separate the protocol/core noise from the
rest.

**Everything needed is already on the client — no backend, no indexing:**

| signal                         | where it already is                                                                                                                           |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| contract / system / diagnostic | `XdrEventDto.event_type` (`crates/api/.../stellar_archive/dto.rs:87`) — for the chip label only, NOT for splitting; see the 0182 caveat below |
| `core_metrics` vs the rest     | `topics[0]` — `ExecutionTrace.tsx:125` already branches on exactly this                                                                       |
| calls vs effects               | `fn_call` / `fn_return` kinds, same stream (task 0462's stack walk)                                                                           |
| position in the record         | `XdrEventDto.event_index` — already on the wire, unused by the UI today                                                                       |

**Target — filter the one chronological list, do NOT regroup it.** The first
draft of this item said "group by kind, each group collapsible". Rejected on
review the same day: **this section is the raw chronological record, and the
order IS content.** Sequence tells you the balance check preceded the transfer
which preceded the mint; three sorted boxes preserve order inside each box and
destroy every relation between them. The decoded story lives in 0462's
execution trace — the value of THIS section is being the faithful in-order
dump, so a redesign that breaks order trades away the only thing it is for.

So: one list, original order, untouched. What changes around it:

- **Per-row index.** `event_index` is already on the wire and already unused by
  the UI. Show it. `#3 … #7 … #12` makes a gap self-evident: the reader sees
  both that something is hidden and how much.
- **Counts in the header.** `27 events — 3 contract, 5 calls, 19 host counters`.
  This one line does half the work on its own: today "27 events" conveys
  nothing about whether there are 2 real effects in there or 25.
- **Per-kind toggles**, `core_metrics` off by default (host counters, not part
  of the transaction's story — 0462 excludes them from the trace for the same
  reason).
- **Hiding is stated, never silent** — "19 host counters hidden", with the
  toggle next to it. A list that looks complete and is not is worse than the
  noise it removed.

Cheaper than the grouping build, too: no restructuring of the table, just a
filter predicate plus a header line.

**Bug found alongside, fix here — but mind 0182.** `heavy.contract_events`
carries **contract _and_ system** events (`dto.rs:40`), while `EventsSection`
chips every row from that array as `Contract` (`EventsSection.tsx:88-98`). A
real, in-our-data example of the mislabel: `executable_update` — the host's own
event for a contract code upgrade (CAP-0046-05, mainnet `CCABO2IQ…` at ledger 55363489) — reads on the page as an ordinary contract event, which it is not.

**The naive fix is wrong.** "Read `event_type` per event instead of inferring
the kind from the array" would undo the deliberate container-based split in
`split_events` (`extractors.rs:256-262`): the diagnostic container holds
byte-identical **Contract-typed copies** of the consensus events, so a
type-based split resurrects task 0182's double-count — one transfer rendering
as two.

Correct shape: **the array split stays exactly as it is** (it encodes
provenance — which XDR container the event came from). Only the chip label
changes, and only for rows from the consensus array, where `event_type`
distinguishes `Contract` from `System`. Rows from the diagnostic array stay
labelled `Diagnostic` regardless of their inner type — that label is telling
the truth about where they came from.

**Independently shippable** — F needs no `ScValView`, no decoder, no summary
line. It can go out before B/A/C-lite and is the cheapest single improvement to
that section. Do not let it queue behind the decoder.

**Not a duplicate of 0462:** that task's execution trace lives on the operation
card and already drops `core_metrics` and nests calls. F is the page-level raw
Events section further down, which nothing has touched.

## Wireframe (Events)

Now:

```
Type      Topics                        Data
Contract  [ {"type":"sym",              { "type":"i128",
             "value":"fee"}, … ]          "value":"947930" }
```

Target (collapsed):

```
● transfer   CCW67T…MI75   GX…4P → GY…8K        326,131,711   [raw ⌄]
● fee        CAS3J7…OWMA   GB6KVOP2…KNFCP            947,930   [raw ⌄]
```

## Component inventory (reuse, don't rebuild)

`IdentifierDisplay` (addresses), `Chip` (bool/badge/event-name), `formatCompactAmount`
(amounts), collapse pattern from `XdrRow`, `useCopyToClipboard`, and fold in the
existing `inlineScalar` logic. **New: only `ScValView`.**

## Scope / cost

- **Pure frontend.** Data already typed via `heavy` (archive XDR) — **zero
  backend, zero indexing.**
- Core = 1 component (`ScValView`) + wiring in `EventsSection` + `OperationJsonDetail`.
- **F (event taxonomy)** is cheaper still and separable: a filter predicate, a
  header count line and a chip-label fix inside `EventsSection` only — no new
  component, no decoder, no re-layout. Ship it first.
- **Do with 0352:** the `error` ScVal → typed chip is the same decoder as the
  fail-reason banner. `scval.rs:19` currently drops the code — fix there feeds both.

## Implementation Plan

0. **F — event taxonomy** (issue #378): keep `EventsSection` one chronological
   list; add per-row `event_index`, per-kind counts in the header, per-kind
   toggles with `core_metrics` off by default and the hidden count stated.
   Chip label from `event_type` for consensus-array rows only (the array split
   itself stays — 0182). Ships alone, first: depends on nothing below it.
1. `ScValView` — recursive `switch(value.type)` renderer (table above), with a raw fallback.
2. Wire into `EventsSection` (topics/data) and `OperationJsonDetail` (args/return/auth); retire `inlineScalar` into `ScValView`.
3. Collapse each event/operation (A) — `XdrRow`-style, decoded summary visible, raw one click deeper.
4. C-lite: known-event semantic line + invocation signature/call-tree.
5. (bonus) named params from `wasm_interface_metadata`.
6. Keep `RawDataSection` (base64 XDR) collapsed; optionally add an "open in Stellar Lab" deep-link.

## Acceptance Criteria

**F — reconciled 2026-08-05 against what actually shipped in #379.** Three
criteria below described the _filter-toggles_ design and were rewritten, not
ticked: that design was rejected mid-implementation once the 27 rows turned out
to be two different records rather than one noisy list. Filtering assumes the
rows belong together and the reader picks a subset; they do not belong
together, so the split is by record and there is nothing to toggle. The
original wording is kept struck through so the change is visible, not silent.

- [x] ~~Events section stays a single chronological list — never split into per-kind boxes~~ → Events lists the **consensus stream only**, in container order, never reordered. The debug channel is a separate disclosure — split by RECORD, not by kind, which is what the rejected wording was guarding against (F)
- [x] ~~Header shows a count per kind; `core_metrics` hidden by default with a toggle stating the hidden count~~ → no toggles: the header counts the consensus stream alone (`3 events`), the debug channel states its own count on its disclosure, and `core_metrics` left the list entirely for the operation card's `Resources` panel (F)
- [x] Every row shows its `event_index` (F)
- [x] A consensus-container `system` event (e.g. `executable_update`) is chipped `System`, not `Contract` — `EventsSection.test.tsx` (F)
- [x] The `contract_events` / `diagnostic_events` split still routes on container, not `event_type` — a transaction whose diagnostic container copies its contract events still renders each effect once (0182 regression guard, F) — `EventsSection.test.tsx`
- [x] Tx-level events state WHEN they fired: `TransactionEvent.stage` carried XDR → wire → a `Where` column, beside the operation index for per-op events. Pinned against real mainnet meta in `crates/xdr-parser/tests/tx_event_stage_real_meta.rs` (F, added during implementation)
- [x] The `Resources` panel cannot silently drop a counter, whatever shape its value arrives in — `resources.test.ts` (F, added 2026-08-05; load-bearing, see below)
- [ ] Event topics/data + operation args render as typed chips (addresses linked+truncated, amounts formatted), not raw `{type,value}` JSON
- [ ] Events/Operations collapsed for large sets; raw JSON still reachable per node/section
- [ ] Known events (transfer/mint/burn) show a one-line semantic summary
- [ ] Invocation shown as `fn(args) → result` with sub-call tree — **done in 0462** (execution trace); do not rebuild
- [ ] `error` ScVal renders as `Type/Code` chip (shared with 0352)
- [ ] Page height for a ~30-event tx drops from ~13k px to a scannable list
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`: the Events-section contract (consensus stream vs debug channel), the `Where` column, and the `Resources` disclosure. Was marked `N/A — pure FE presentation`; wrong, the frontend data contract is described in those docs (ADR 0032)
- [x] **API types regenerated** — `98a0eee8`. Was marked `N/A unless scval.rs changes`; wrong. `XdrEventDto.stage` is a new wire field, and separately a doc comment on a `ToSchema` struct IS the OpenAPI `description`, so even the comment-only correction moved the spec and reddened `API types freshness`

## UX Expert Analysis (`/ux-expert`)

Audit of the current state + per-option verdict, grounded in the actual
components (`EventsSection`, `OperationJsonDetail`, `HighlightedJson`, `XdrRow`)
and the decoder (`crates/xdr-parser/src/scval.rs`). Premise confirmed:
`scval_to_typed_json` emits a clean, recursive tagged union `{type, value}` — a
`switch(type)` renderer is real, not speculative. `scval.rs:19` (`e.name()`)
does drop the error code, so the `Type/Code` chip needs the Rust fix.

### Current-state findings (2 Critical, 5 Major)

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

### Per-option verdict

- **E (raw JSON, current):** ✅ keep ONLY as per-node "raw" fallback; ❌ never default. Escape hatch, not a view.
- **A (collapse):** ✅ highest ROI, uses existing `XdrRow`. **But collapse without a decoded summary line is a blind toggle** — A has a hard dependency on ≥ B-lite (decode `topics[0]` sym + contract) for the row label.
- **B (`ScValView`):** ✅ the load-bearing fix (F1/F3/F4/F5). Input already a clean union → one component, table-driven `switch`. Ponytail: no plugin architecture; first cut = common types (sym/address/int/bool/bytes/vec/map), tail (void/duration/timepoint/contract_instance) handled but not gold-plated. Must default-collapse nested structures or it becomes a new wall.
- **C / C-lite (semantic):** ✅ biggest comprehension gain (F6), biggest domain risk. Known-shape match is a heuristic → unknown shapes MUST degrade to plain B, never guess; "assets moved" amount needs decimals metadata. C-lite (known events + `fn(args)→result` signature/call-tree) is the right ceiling. Named-params-from-ABI = genuine bonus, defer.
- **D (JSON-viewer lib):** ❌ reject. Renders the same `{type,value}` scaffolding prettier — no decode, no linked addresses, no formatted amounts. Solves F2 weakly, nothing else, adds a dependency + foreign visual language. Wrong rung when B (one component + existing primitives) solves 100%.

### Sequencing (affirms the Implementation Plan, two nuances)

1. **B-core** first — A and C both depend on a decoded summary. Wire into events topics/data + op args.
2. **A** (collapse via `XdrRow`) — each collapsed row now has a real summary line. Kills the wall.
3. **C-lite** — known-event line + invocation signature/call-tree, with graceful fallback to B.
4. Fix **`scval.rs:19`** error code (with 0352) — unblocks the `Type/Code` chip.
5. Bonus: named params from ABI — defer.

**Two corrections to make explicit in the plan:** (1) A has a hard dependency on
B-lite — collapse needs a summary to show; (2) reject D outright. AC mapping is
clean: "13k→scannable" ← A, "typed chips" ← B, "semantic summary" ← C-lite.
Task is well-diagnosed and well-scoped.

## Status (2026-08-05)

**F is merged** — PR #379 merged to `develop` as `39111c44` on 2026-08-05.
**Not deployed.** Backend and frontend must ship TOGETHER: until the deployed
API returns `XdrEventDto.stage`, the `Where` column renders `—` for the two fee
events (verified against the live dev API, whose response carries no `stage`
key at all). The reply drafted for issue #378 should not go out with a
production link before that.

- **F — event taxonomy.** Landed as the two-channel split, not as the
  grouping the issue asked for: the consensus stream (`contract` + `system`)
  is the list and the count, the host debug channel is its own labelled
  disclosure. On the reported transaction the header went 27 → 3, because 27
  was counting a mirrored copy and 19 resource counters alongside 3 real
  events. `TransactionEvent.stage` is now carried from XDR to the wire and
  shown in a `Where` column beside the operation index — the protocol's only
  statement of when a tx-level event fired.
- **Resource counters** — out of the event list entirely, onto the invoke
  operation card behind a `Resources 19` disclosure. All nineteen, in host
  emission order, grouped numbers. They are one record with nineteen fields,
  not nineteen events; stellarchain shows a resources panel, stellar.expert
  shows none, and `getEvents` never returns diagnostics at all.

**Late corrections, 2026-08-05** — two commits after the ones above, both from
re-reading the shipped result rather than the plan:

- `c3679237` **show every resource counter, on every Soroban op.**
  `readResourceCounters` skipped any entry whose value was not a JSON number,
  so a big int arriving as a decimal string would vanish and the panel would
  read 18 of 19 without saying so. Now total: unexpected shapes pass through
  verbatim, an unnamed counter is labelled by its index. The panel was also
  gated on `INVOKE_HOST_FUNCTION`; footprint extend/restore run through the
  same host and are metered the same way, but raise no `fn_call` so they get
  no trace either — the narrow gate swallowed their counters entirely. Gated
  on the Soroban op set now. Totality here is load-bearing: it is what lets
  the raw diagnostics table omit `core_metrics`.
- `fe70d7a0` **correct what the diagnostics disclosure claims.** The blurb
  said the debug channel carries "a copy of each event above". Decoding the
  reported transaction's meta says otherwise — of its three consensus events
  only ONE is mirrored. The fee charge and refund are raised by ledger
  application, not the host VM, so there is nothing for the host to copy; the
  mirroring covers contract-emitted events only. Same overclaim was in the
  section doc and a test comment. Disclosure relabelled `N diagnostic
entries`: "execution diagnostics" read as a _category of event_, which is
  the distinction the issue asked for and the wrong way round.

**Already done elsewhere** — C-lite's `fn(args) → result` signature and nested
sub-call tree shipped as the execution trace in **0462**. Do not rebuild it.

**Open** — **B** (`ScValView`), **A** (collapse events/operations), and
C-lite's remaining half: the one-line semantic summary for known events, which
is also what fixes amounts. Note `13802682` still renders raw beside a headline
saying `1.3802682 VELO` on the same card: `ExecutionTrace.tsx` never calls
`formatAmount`/`scaleByDecimals`. B alone gives grouping, not the decimal
point — scaling needs the asset, so it belongs with the semantic line.

Two smaller items surfaced by review, not yet done: `EVENTS · 7` on the
operation card and `15 events` in the section use one word for two scopes; and
`transfer` appears in the trace both as a called function and as the event it
raised, told apart only by a glyph.

## Design Decisions

### Emerged (2026-08-05)

1. **The raw diagnostics table stays — deleting it was proposed and
   rejected.** Measurement across nine real mainnet transactions (the
   eight-fixture corpus plus #378's), replicating `buildExecutionTrace`'s
   stack walk in Rust, found **zero** diagnostic entries the trace cannot
   place — including the failed transaction, whose `error` / `log` /
   `host_fn_failed` all landed inside the call that raised them. That made
   the flat table look like pure duplication of a worse rendering, and it was
   removed. Reinstated on the explicit instruction that the raw record must
   always be reachable somewhere. The measurement stands and is why the
   removal was tempting; the ruling is that a readable view is an addition to
   the raw record, never a replacement for it.

2. **A `Copy` chip on mirrored rows was tried and reverted.** The diagnostic
   container's copy of a contract event renders with a blue `Contract` chip —
   exactly the label the issue asked to separate. Relabelling it `Copy` was
   implemented, then dropped: it renames a row instead of addressing why the
   row confuses, and the chip reports the XDR's own `type_`. The disclosure
   blurb now states plainly that the channel contains copies.

3. **`core_metrics` is the ONE thing the raw table omits.** Consistent with
   (1) only because `readResourceCounters` is total — the panel is a complete
   record of the counters, so nothing leaves the page. On a minimal Soroban
   transaction they are 19 of 24 rows and would bury the four that describe
   the execution.

4. **`stage` is informational, by design.** It drives no logic, no sorting,
   no filtering — one read site (`whereLabel`). It cannot order anything:
   per-operation events carry no stage, so there is no total order to build
   from it. Its job is to remove an information gap, not to compute.

## Future Work

- **The execution trace renders the diagnostic COPY, not the consensus
  event.** On the reported transaction the trace's `•` row is `#6` (debug
  channel), not `#2` (consensus). Same payload, but the copy carries no
  `op_index` — the consensus row is the richer record. Nothing differs on
  screen today, so this is latent: if a copy ever diverged, the trace would
  show debug data wearing a consensus event's clothes and nothing would flag
  it. Fix shape: use the diagnostic stream for POSITION only and render the
  matching consensus event. Zero visible change, so not done now.

## Notes

- Related: 0071 (original advanced tx-detail), 0352 (fail-reason banner — shared decoder), 0013 (shared xdr/scval parsing).
- Research artifact from the explorer comparison: `stellarchain-tx.jpeg` (worktree, disposable).
