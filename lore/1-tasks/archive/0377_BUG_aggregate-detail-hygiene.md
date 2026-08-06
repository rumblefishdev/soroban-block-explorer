---
id: '0377'
title: 'Aggregate/detail hygiene: archive-unavailable counts assert zero, participant list truncation, stale estimate docs'
type: BUG
status: completed
related_adr: []
related_tasks: ['0359', '0329', '0420', '0453', '0463']
tags: [priority-medium, effort-medium, layer-api, layer-web, aggregates]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles K4-1, K4-2/K1-4, K4-5.'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Promoted to active to begin implementation.'
  - date: 2026-08-04
    status: active
    who: karolkow
    note: >
      Re-scoped. All three spawned items (K4-1, K4-2/K1-4, K4-5) verified
      already-resolved — twice, 3 weeks apart, across 419 develop commits, with
      no regression. Replaced with six findings from a fresh aggregate/detail
      sweep of crates/api + web. Same theme, current defects.
  - date: 2026-08-04
    status: completed
    who: karolkow
    note: >
      14 commits. 6 findings closed: F1/F2 widened to 6 render sites on the tx
      page (HeavyUnavailable shared, +3 tests), F3 tripwire only (trigger
      measured absent 0/6010 and structurally unreachable), F4 docs corrected —
      the numbers had been exact since 0420, F6 stats_window on the list row,
      F7 7-of-8 sites. F5 handed to 0463, which owns the same SQL line and the
      evidence deciding its direction. 4 of 11 reported defects refuted: the 3
      spawned items were already resolved (re-verified across 419 develop
      commits), and F7 #8 was correct code — 36_279_761 prod payments show
      blank asset_code and absent issuer coincide exactly, so the XLM fallback
      stands. 208 web + 243 API tests green. No ADR.
  - date: 2026-08-05
    status: completed
    who: karolkow
    note: >
      Corrects the entry above, after three fresh-context reviews of PR #381.
      Two of its claims were wrong. "F3 structurally unreachable" is false —
      the search covered crates/ only; a prod runbook deletes accounts rows,
      rolling accounts back on last_seen_ledger while lp_positions rolls back
      on last_updated_ledger, which produces exactly the dangling surrogate.
      The invariant is operator-maintained, so the tripwire is better justified,
      not redundant. "36_279_761 payments show blank asset_code and absent
      issuer coincide exactly" is a tautology — split_asset_ref returns the
      pair all-or-nothing, so zero one-sided rows is forced by the writer; the
      XLM fallback still stands, on operation_asset_appearances instead.
      The reviews also found the headline fix incomplete: heavy == null misses
      a heavy block whose envelope is absent, so "0 signatures" still rendered.
      7 follow-up commits; 226 web tests (+6).
  - date: 2026-08-06
    status: completed
    who: karolkow
    note: >
      Scope cut on a file-by-file walkthrough of the PR. Six changes reverted to
      develop byte-for-byte: F6 (stats_window on the list row) with its two
      consumers, F7's pool-participants and contract-interface sites, a search
      hint line, an extracted status strip, the operations-section strip, a
      `variant` prop on the shared TableEmptyState, and the ledger empty-state
      copy — the last REFUTED, not merely cut: `ledgers` rows are the ingest
      commit marker, written only after every other table acks, so a header can
      never precede its transactions (0/200_000 measured on prod). The
      strict-send "Received" row was deleted outright rather than reworded.
      One behaviour reversed: operationEntries no longer substitutes folded DB
      rows for a missing archive answer — an archive miss now reads as a miss.
      Every cut had the same shape: a defence built against a case nobody
      measured, or a second copy of a signal already on screen (see Design
      Decisions 12-14). F7 lands at 4 shipped, 2 refuted, 2 cut. F7 lands at 5 of
      8, not 7. 224 web + 243 API tests green. Sections below
      reconciled with what actually ships.
---

# Aggregate/detail hygiene

## Summary

Summary numbers that disagree with the detail they summarise. Two classes:
counts asserted over data the response never loaded (archive-gated `heavy`),
and a count derived on a different basis than the list it heads.

Read-side + render-side only. No schema or ingest change.

## Context

Spawned from 0359's K4 cluster as a bundle of three items. Those three were
verified resolved before implementation started, and re-verified 3 weeks later
against 419 intervening develop commits — no regression. They are recorded
below as closed, and the task re-scoped onto what the same sweep found current.

The dominant new defect has a single root cause: **`heavy_fields_status` has
zero production reads in `web/src`** (only `TransactionSummary.test.tsx`
references it). The wire contract distinguishes "archive fetch failed" from
"the tx genuinely has none of these"; the render layer collapses both to `0`.

## Verified resolved (no work — do not re-fix)

| Item                                                       | Where it was closed      | Proof on develop `7a99a1a9`                                                                                                                                                                                                                                                           |
| ---------------------------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **K4-1** invocations KPI 7d vs all-time list               | frontend labelling       | `ContractSummary.tsx:29-36` renders `Invocations (last ${stats.stats_window})`; `ContractsTable.tsx:76` header `Invocations (7d)`; `contracts/queries.rs:550-554` test locks label ↔ SQL                                                                                              |
| **K4-2 / K1-4** `operation_count` vs folded `operations[]` | 0329, frontend           | `operationEntries.ts` drives the picker from unfolded `heavy.operations` only — the folded light rows are matched in for identity, never substituted for a missing archive answer (see Design Decisions 11); `OperationsSection.tsx` takes the header from `operation_count`; 2 tests |
| **K4-5** nullable-aggregate decode 500                     | 0324 + review discipline | Every aggregate decode site guarded (`ifNull`/`coalesce`/`toNullable`/`toString`→`Option`/`count()` over non-Nullable); regression test `contracts/queries.rs:591-597`; **zero** new unguarded sites across the 419-commit window                                                     |

## Implementation

### F1 — signatures render "0 signatures / No signatures recorded" on archive miss

`index.tsx:88-90` passes `heavy?.signatures ?? []` with no status check;
`SignaturesTable.tsx:32-46` prints a bare `0 signatures` meta plus the definite
copy "No signatures recorded.". Every valid Stellar transaction carries ≥1
signature, so on `heavy_fields_status: "unavailable"` the UI states something
that cannot be true. Renders unconditionally (not mode-gated).

### F2 — events / raw-data assert a zero they never loaded

Same root cause, same page. `EventsSection.tsx:47,55-64` → `${total} events` +
"No events emitted."; `RawDataSection.tsx:52-61` → `${entries.length} sections`

- "No raw XDR available for this transaction.". Weaker than F1 (a tx may
  legitimately emit zero events) but still asserts an unknown. Exposure widened
  by 0453, which dropped the advanced-mode gate — these now always render.

Fix for F1+F2 is one change: lift the file-local `HeavyUnavailable`
(`op-card/OperationJsonDetail.tsx:109-120`, the only copy in the repo) into a
shared module — it ships as `UnavailableSection` in
`transaction-detail/shared/Unavailable.tsx` — and swap the meta to `—` with that
state when the archive answer is missing.

A sweep for the same pattern found `TransactionSummary` reads `heavy` in three
more places on the **same page**, each collapsing "not loaded" into a confident
claim — so F1/F2 covers these too, or the page ships half-honest:

- **memo** (`TransactionSummary.tsx:93-94` → `MemoCell`) — renders the same
  `<Dash />` as a genuine `memo_type: 'none'`. A memo can carry an exchange
  deposit id, so a false "no memo" has consequences.
- **fee source** (`:205-220`) — the row is gated on the _light_ `inner_tx_hash`,
  so a fee-bump tx renders `Fee source: —`. Every fee-bump envelope has a fee
  source; the dash asserts an impossibility.
- **fail reason** (`opFailReason`, `:38`) — `heavy?.operations ?? []` yields no
  failing op, returning `null`, whose documented meaning is "validation-level
  failure, no operation attempted". An archive miss is rendered as a _different
  failure class_.

### F3 — participant count vs list — LATENT, not live (measured)

`liquidity_pools/queries.rs:323-325` (detail) and `:1096-1099` (list) count
`lp_positions FINAL WHERE shares > 0`. `fetch_participants` then builds rows
through `filter_map` which **silently drops** any position whose account
surrogate does not resolve to a StrKey. The comment called the invariant
"a position always has its account" — unenforced and unobserved.

The mechanism is worse than a count mismatch: the drop happens inside
`fetch_participants`, i.e. **before** `finalize_page` (`handlers.rs:100`)
inspects the `limit + 1` sentinel. Dropping the sentinel row flips `has_next`
to false and ends pagination early — hiding the remainder of the list.

**Prod measurement (`chq`, 2026-08-04) — the trigger does not occur:**

| Probe (mirrors the code: `lp_positions FINAL WHERE shares > 0` vs `accounts … LIMIT 1 BY id`) | Result |
| --------------------------------------------------------------------------------------------- | ------ |
| distinct participants with `shares > 0`                                                       | 6010   |
| of those, absent from `accounts` (would be dropped)                                           | **0**  |
| of those, resolving to an empty StrKey (renders blank; `get()` does not drop it)              | **0**  |

**The invariant is operator-maintained, not structural.** An earlier draft of
this section claimed the opposite — "all four `ALTER TABLE accounts DELETE`
sites are `#[cfg(test)]` teardown, so production never removes an account row".
That was false: the search covered `crates/` only. There are 13 DELETE sites,
and `docs/runbooks/0225_backfill_crash_recovery.md` deletes `accounts` rows in
prod, rolling `accounts` back on `last_seen_ledger` while `lp_positions` rolls
back on `last_updated_ledger` — precisely the dangling surrogate this finding is
about. `repair_tier1::rebuild_accounts` also swaps the whole table via
`EXCHANGE TABLES`. So the 0/6010 measurement says the invariant holds _today_,
not that it cannot break.

Blast radius if it ever did break: 82 of 26_487 pools hold more than one page
(>20 participants), 17 hold >100, largest 684 — in the other 26_405 every
participant fits on one page, where truncation is impossible by definition.

**Verdict: latent defect, not a live bug.** Earlier framing as "the most severe
backend item" was a mis-call — the mechanism was traced but the trigger never
checked.

**Fix taken: the tripwire only.** Replaced the silent `?` with a
`tracing::error!` naming the surrogate and pool, and recorded the measurement in
the comment. Deliberately NOT restructuring pagination or re-deriving the count
from the resolved set — that is real complexity against a case measured absent.
The log makes the invariant observable, so if a backfill or repair procedure
ever breaks it we get a CloudWatch hit
(`/aws/lambda/production-soroban-explorer-api`, 30-day retention) instead of a
user silently seeing a truncated list. `error!` not `debug!` because the Lambda
runs at `RUST_LOG=info`.

### F4 — network totals: the docs are stale, not the numbers

`network/queries.rs:102-104` now counts exactly — `count() FROM accounts_recent`
(deduped MV, same source the `/accounts` list pages) and
`count() FROM soroban_contracts FINAL`. The `system.tables.total_rows` planner
estimate was removed under 0420 for measured RMT inflation (+4.3% accounts,
+11.6% contracts).

`network/dto.rs:14-15,32,34` still describe both as "planner estimates, not
exact counts", which propagates to `types.gen.ts:746-747,776-783`. The doc now
understates accuracy and names a mechanism the query no longer uses.

**Do not "fix" the frontend here** — `ChainOverview.tsx:67-92` rendering them
as exact totals is correct. Adding a `≈` marker would introduce the defect.

### F5 — accounts list vs detail disagree on zero balances → owned by [[0463]]

**Do not fix here.** Same line of SQL, and 0463 decides the direction.

The divergence: the list subquery `accounts/queries.rs:236-239` filters only
`a.asset_type = 0`, while detail `:422` adds `AND b.amount != 0`. An account
whose native row is exactly zero shows `xlm_balance: "0"` in the list and has no
native entry in the detail's `balances[]`. `accounts/dto.rs:34-43` documents
`null` when no native row exists, without disclosing that the two endpoints
disagree on what "exists" means.

Read alone, this admits two opposite repairs — add the zero filter to the list
(hide zeros everywhere) or drop it from the detail (show them everywhere). 0463
settles it with prod + Horizon evidence on a reported account: `balances` holds
five rows, the page renders two, and the three hidden ones (AQUA, SHX, USDC) are
**live trustlines at 0.0000000**. An established trustline at zero is a fact
about the account — it can receive that asset — not an absence. So the detail
filter comes OFF; adding one to the list would deepen the reported bug.

Fixing 0463 dissolves this finding as a side effect: with `:422` relaxed, both
endpoints agree that a zero row exists, and the DTO doc gets corrected there.
F5 therefore carries no separate work — it is the API-consistency view of a
defect 0463 owns end to end (it also adds signers/thresholds from the same RPC
round-trip).

Caution when cross-referencing: 0463 was triaged from **GitHub issue #377**,
which is unrelated to lore task 0377 — the matching number is coincidence.

### F6 — contract list window label — CUT, not shipped

The finding: `ContractListItem` (`contracts/dto.rs:25-42`) carries
`recent_invocations` but no `stats_window`; the detail's `ContractStats` has one.
Numbers are consistent — both resolve the bound from `STATS_WINDOW_DAYS` — only
the wire label is missing for list consumers.

**Implemented, then reverted.** The field, its SQL projection and the
`ContractsTable.tsx` consumer are back to develop byte-for-byte (`0ea29e9b`).
The window is a compile-time constant, so the hardcoded `(7d)` header cannot
drift without someone editing `STATS_WINDOW_DAYS` in the same repo — the drift
the field was meant to prevent has never occurred and has no mechanism to occur
silently. Adding a required wire field, regenerating the API types and widening
the contract, all against that, is not a trade worth making. Revisit only if the
window ever becomes configurable at runtime.

### F7 — same pattern, eight more sites — 4 fixed, 2 refuted, 2 cut

The sweep that found the `TransactionSummary` trio also found the shape
elsewhere. Four ship. Two were false positives and are recorded as such so
nobody "fixes" them again. Two were reverted on the walkthrough — real instances
of the pattern, but the fix bought less than the diff cost.

**Refuted — `humanizeOp.ts` asset fallback.** The sweep flagged
`light.asset_code ?? 'XLM'` because the wire doc only says "asset code for
classic asset operations" and never states `null ⟺ native`. The doc gap is real;
the defect is not — but the first evidence cited for that was worthless, and is
kept here as a warning:

- **Discarded (tautology).** "Across 36_279_761 payment appearances, blank
  `asset_code` and absent `asset_issuer_id` coincide exactly, 0 one-sided rows."
  `split_asset_ref` returns that pair all-or-nothing, so zero one-sided rows is
  forced by the writer. It cannot distinguish native from a parse failure — the
  very hypothesis it was cited against.
- **Stands.** `operation_asset_appearances` resolves every blank-code single-op
  payment to the native asset id (11_168/11_168 native, 55_582/55_582
  non-native), an independent table the fallback does not feed. Plus Horizon
  spot-checks, plus the parser's own `SacAssetIdentity::Native` ("NULL
  `asset_code` + NULL `issuer_id`").

So on a payment a null code does mean native, and removing the fallback replaced
a correct "Sent XLM to …" with a vaguer "Sent a payment to …". The change was
written, caught by `OperationPicker`/`HumanizedSentence` tests, verified against
prod, and reverted; the surviving evidence lives in a comment at the call site.

Same failure mode as F4: a stale/incomplete doc-comment read as evidence about
behaviour. Second time this task — the wire docs are not a reliable oracle for
what the data actually contains. And once the doc gap was mistaken for a defect,
the first "measurement" reached for confirmed it by construction.

| Sev | Site                                                             | Collapses into                                                                                                                             | Outcome                                                                                                           |
| --- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| MED | `OperationsSection.tsx` → `OperationCard.tsx`                    | trace / authorized-calls / events / route strip silently vanish on archive miss — the call reads as "made no sub-calls"                    | **ships**, rewritten — one `UnavailableSection` when the list is empty, not a per-card strip                      |
| MED | `operationEntries.ts:42-45`                                      | falls back to folded light rows while the card header says `operation_count`; also conflates "heavy present, zero ops" with "heavy absent" | **ships, direction reversed** — the DB fallback was deleted, not annotated (see Design Decisions)                 |
| MED | `ledgers/LedgerTransactions.tsx:33-38`                           | "This ledger closed without any transactions" while the same screen shows a non-zero `transaction_count`                                   | **refuted** — the state is unreachable; see below                                                                 |
| MED | `search/useSearchResults.ts:85-90` + `SearchResultsTabs.tsx:118` | a capped bucket renders a hard `10` that reads as a total; `10+` would be honest                                                           | **ships** — cap sent explicitly, badge renders `N+`                                                               |
| LOW | `op-card/opFacts.ts:31`                                          | `Received: —` on a successful swap reads as "received nothing"                                                                             | **ships** — the row was deleted outright, taking `opFacts.ts`, its test and the card's fact grid with it          |
| MED | `shared/humanizeOp.ts:110,122`                                   | asset defaults to `XLM` when `details` is absent — nothing promises `asset_code: null ⟺ native`                                            | **refuted** — code unchanged, surviving evidence recorded in a comment                                            |
| MED | `pool-detail/PoolParticipants.tsx:117-121`                       | "No participants yet" against a non-zero `participant_count`                                                                               | **cut** — reachable only on a fetch failure, which the query layer already surfaces; the F3 route measured 0/6010 |
| LOW | `contracts/ContractInterface.tsx:208-216`                        | "No public interface" names a specific cause; a not-yet-parsed contract is told it is a SAC                                                | **cut** — copy polish, not a wrong claim about loaded data                                                        |

**Refuted — the ledger "not indexed yet" state.** The finding assumed a
two-phase ingest that writes the ledger header before its transactions, so a
recent ledger could report `transaction_count > 0` over an empty table. The
ingest does the opposite: `PartitionWriter::commit`
(`crates/db-clickhouse/src/persist/writer.rs:320-362`) drains all 22 other table
inserts and only then opens the `ledgers` insert — **the ledger row IS the commit
marker**. A mid-partition failure leaves no `ledgers` row at all, and the resume
path re-does the range. So a visible header implies its transactions are already
visible.

Measured on prod (`chq`, ledgers 63_626_966-63_826_965): of 200_000 ledgers with
`transaction_count > 0`, **0** had zero rows in `transactions`.
`LedgerTransactions.tsx` is back to develop and its test moved to `.trash/`.

Third assumption in this task taken as evidence without being checked — after
F4's stale doc-comment and F7 #8's tautology. All three concerned the write path,
and none needed prod access to disprove: the answer was in `crates/`, unread.

## Acceptance Criteria

- [x] F1 — signatures section distinguishes "archive unavailable" from "no signatures"; no `0 signatures` meta when `heavy_fields_status == "unavailable"`
- [x] F2 — events + raw-data sections likewise; the unavailable pair extracted to `transaction-detail/shared/Unavailable.tsx` and reused across the page
- [x] F1/F2 extension — memo, fee source and fail reason on the same page no longer collapse "not loaded" into a definite claim
- [x] F3 — trigger measured absent on prod (0/6010) but the invariant is operator-maintained, not structural; unresolved account no longer dropped silently — `tracing::error!` makes it observable. Pagination restructure and single-basis count deliberately skipped as unjustified complexity; revisit only if the log ever fires
- [x] F4 — `network/dto.rs` docs describe the actual mechanism (deduped read-time count, accounts lag MV refresh ≤2 min); API types regenerated; frontend left unchanged
- [x] F5 — no work here: same SQL line as [[0463]], which owns the fix and settles its direction (show zero trustlines). Handing it over, with the reasoning recorded, IS the outcome — fixing it independently risked the opposite repair
- [x] F7 — 4 of 8 same-class sites ship (operations section, operations picker, search cap badge, strict-send Received — the last by deleting the row); 2 refuted against prod (`humanizeOp`'s XLM fallback, the ledger "not indexed yet" state); pool participants and contract-interface copy cut on the walkthrough
- [x] F6 — implemented, then **cut**: the window is a compile-time constant, so the wire label defends against a drift with no mechanism to occur
- [x] `nx run @rumblefish/api-types:generate` run and staged for every `crates/api/**` change (F4; re-run after the F6 revert so the generated types match)

## Docs updated

- `docs/architecture/**` — N/A: no endpoint added/removed, no schema or pipeline
  change, and after the F6 revert no wire field either. F4 alters DTO doc text
  only; F1–F3 and F7 are render-side fixes; F5 ships under [[0463]], which
  carries its own checklist.

## Implementation Notes

36 commits on `fix/0377_aggregate-detail-hygiene`, cut from develop `7a99a1a9`.
Read-side and render-side only — no schema, ingest or endpoint change. The last
six commits are reverts, from a file-by-file walkthrough that cut scope; the
history is kept intact rather than squashed, so each cut is reviewable on its own.

- **Render (web)** — `HeavyUnavailable` lifted from `OperationJsonDetail` into
  `transaction-detail/shared/Unavailable.tsx` and split by scale into
  `UnavailableSection` (a whole section is missing) and `UnavailableValue` (one
  cell in a summary table). Signatures, events, raw-data and operations sections
  plus the memo / fee-source / fail-reason cells distinguish "archive miss" from
  "none". The strict-send "Received" row is gone, and with it `opFacts.ts`, its
  test and the card's fact grid — a slot whose only content was an absence.
  Search
  badge renders `N+` at the cap, with the cap sent explicitly.
- **API (rust)** — `network/dto.rs` docs rewritten to the actual mechanism;
  `fetch_participants` logs an unresolved account instead of dropping it
  silently.
- **Codegen** — `api-types:generate` run for each DTO-touching commit in
  isolation, including the F6 revert, so every commit is self-consistent under
  `git bisect`.
- **Verification** — 231 web tests, 243 API tests, typecheck, lint, `fmt`,
  `clippy`, and `api-types:check-generated` all green at HEAD.

## Issues Encountered

- **Doc-comments used as evidence about behaviour — twice, both times wrong.**
  F4: docs still said "planner estimate" long after 0420 made the count exact;
  following them would have added a `≈` marker to a correct number. F7 #8: the
  wire doc never states `asset_code: null ⟺ native`, so the fallback looked like
  a defect. Root cause is the same in both: a doc describes intent at writing
  time, not what the data now contains. Only code + prod data settled either one
  — and the first prod query reached for on F7 #8 was a tautology that would have
  "confirmed" the fallback either way (see F7).
- **Tests caught a regression I introduced.** Removing the XLM fallback broke
  `OperationPicker` and `HumanizedSentence`. They were NOT edited to pass —
  they were correct, and the change was reverted (see Broken/modified tests).
- **`api-types:check-generated` fails until the regen is staged.** It is a
  `git diff --exit-code`, so freshly regenerated-but-unstaged output reads as
  drift. Not a real failure; staging clears it.
- **Hooks order in `EventsSection`.** The early return for the unavailable state
  had to go _after_ `useMemo`/`useState`, unlike the other two sections which
  have no hooks. Rules of Hooks, not style.
- **Worktree invisibility.** All work happened in
  `.claude/worktrees/fix-0377-…`, while the main checkout stayed on an unrelated
  branch. From the editor the task looked replaced by other work. Nothing leaked
  across branches, but the worktree should be stated up front next time.
- **Number collision.** GitHub issue #377 (which spawned 0463) is unrelated to
  lore task 0377.

**Broken/modified tests:**

- `opFacts.test.ts` — first updated from `'—'` to `'not derivable'`, then
  removed with the code it covered.
- `OperationPicker.test.tsx` / `HumanizedSentence.test.tsx` — failed mid-task
  and were deliberately **left untouched**. They were asserting correct
  behaviour; the code change under them was wrong and got reverted. Recorded
  because a future reader may otherwise wonder why they were not "fixed".

## Design Decisions

### From Plan

1. **Re-scope rather than re-fix.** The three spawned items were verified
   resolved twice, 3 weeks apart, across 419 develop commits. Recorded with
   proof in "Verified resolved" instead of being re-implemented.

### Emerged

2. **`heavy == null` as the status check, not `heavy_fields_status`.** The wire
   contract makes them equivalent, the null is already in scope, and the
   existing good pattern (`OperationJsonDetail`) tests it the same way. No new
   state threaded through the page.
3. **Two visual weights for "unavailable", one file.** `UnavailableSection` (an
   `EmptyState` card) where a whole section disappears, `UnavailableValue` (a
   dimmed "Not available") inside a summary table cell — a card does not fit in a
   cell, and both must read differently from a plain dash. Named by scale rather
   than by mechanism, and kept in one `shared/Unavailable.tsx` because splitting
   two five-line components across two files buys nothing.
4. **F1/F2 widened to six sites, not three.** A sweep found `TransactionSummary`
   reading `heavy` in three more places on the same page. Shipping a page that
   is honest about signatures but still fakes the memo was not worth the smaller
   diff.
5. **F3 got a tripwire, not a repair.** The trigger is measured absent (0/6010),
   so restructuring pagination would be complexity against a case that does not
   occur; `tracing::error!` makes it observable if an operator procedure ever
   creates one. Reversal of an earlier "most severe backend item" call — and the
   justification survived the discovery that the invariant is operator-maintained
   rather than structural, because the fix never depended on that claim.
6. **F4 fixed the docs and deliberately left the frontend alone.** The numbers
   were already correct; the caveat the original finding asked for would have
   introduced the defect.
7. **F7 #8 reverted after prod measurement.** Kept as a comment at the call site
   so the next audit stops at the evidence rather than repeating the change.
8. **F5 handed to [[0463]] rather than fixed.** Same SQL line; 0463 carries the
   evidence that decides the direction, and the opposite repair was equally
   available from F5's description alone.
9. **Search cap sent explicitly instead of inherited.** The badge needs the cap
   to render `N+`; hardcoding a copy of the server default would drift, so the
   frontend now states the value it reasons about.
10. **Kept as a single file**, past the ~200-line guideline. The content is one
    continuous record with inline evidence, not research notes needing a
    `notes/` split; converting an archived task would be churn.
11. **The DB index is not a stand-in for the archive.** `operationEntries` used
    to fall back to the folded `operations_appearances` rows when
    `heavy.operations` was empty. Those rows are deduplicated by shape — four
    identical payments collapse to one — so the fallback could render a single
    operation under a header saying "4 Operations", which is a worse lie than
    saying nothing. An archive miss now reads as an archive miss. Deleted, not
    annotated.
12. **Colour is not a second signal.** The ledger empty state briefly carried a
    new `variant="warning"` prop on the shared `TableEmptyState`. Cut before the
    state itself was refuted, and for its own reason: the title and description
    already said it, `libs/ui/src/states/empty/EmptyState.tsx` already had that
    exact prop, and the whole mechanism existed for one call site.
13. **A row whose only content is an absence should not exist.** `opFacts` was
    reworded (`'—'` → `'not derivable'`) and gated on `applied` before the
    simpler question got asked: the function produced exactly one row, ever,
    saying the explorer does not have the number. Deleted with its test and the
    card's fact grid. When the `net_settled` read path lands, a row with a real
    value is three lines. "Not derivable" was also an overclaim — the amount is
    unrecorded, not underivable.
14. **Eight changes cut on a file-by-file walkthrough.** Every cut had one of two
    shapes: a defence against a case nobody measured (`PoolParticipants`
    truncation, a `stats_window` drift with no mechanism), or a second copy of a
    signal already on screen (a status strip above sections that each already say
    they are unavailable, a search hint next to the badge that carries the same
    fact). Both shapes look like diligence in review and read as noise in use.
    The measurement, not the mechanism, is what justified keeping anything.

## Review response (PR #381, three independent fresh-context reviews)

Self-review was skipped as worthless here — the same reasoning that wrote the
code would have re-approved it. Three reviewers got the diff and no rationale.
They found one hole in the headline fix and refuted two of the evidence claims
in this very document. Everything below is fixed on the branch.

**The headline fix did not close its own defect.** `heavy == null` is the wrong
signal: `align_envelopes` yields `None` per transaction on a hash miss, and
`extract_e3_heavy` still returns a heavy block carrying `signatures: []`. So a
partial archive answer walked straight past the guard and rendered "0
signatures" — the exact impossible claim F1 exists to kill. Worse, the test
added with F1 asserted that state as correct. `SignaturesTable` now treats ANY
empty list as unreadable and the test pins that instead. The same widening was
applied to the operations section, which had the same hole.

**Two evidence claims in this file were wrong** — F3's "structurally
unreachable" (a grep scoped to `crates/`, missing a prod runbook) and F7 #8's
coincidence count (a tautology forced by the writer). Both are corrected in
place, in F3 and F7 above, so a reader hits the correction and not the claim.
Neither fix changed; both are better justified. Recorded here because the common
cause is worth naming: a search whose scope was never stated, and a measurement
whose independence was never checked.

**Also fixed:** an `Alert` that violated a rule documented six lines away in a
file this task edited (no `MuiAlert` theme style — in light mode its border
derives from a fill token and disappears); a `participantCount` warning that fired on page 2 and on stale cursors; a
dead disjunct in `operationEntries` whose comment claimed a split the code did
not make; "not derivable" shown on FAILED path payments, where zero is a fact
and not an unknown; an `N+` badge in a 22px circle sized for two glyphs; italic
copy that appears nowhere else in the app; "Heavy XDR fields" leaking internal
vocabulary to users; and the absence of a retry on a transient fetch.

Two of the review's fixes were themselves cut afterwards, on the walkthrough:
sharpening `ContractInterface`'s empty-state copy, and giving `stats_window` a
consumer in the contracts table. Both were sound as review responses; neither
was worth the wire-contract change and the extra surface underneath them.

**Clean under review:** Rules of Hooks, the develop merge (verified additive
against both parents — nothing from the 0363 rework was dropped), the `let-else`
refactor, and the `network/dto.rs` doc rewrite.

## Future Work

- **Search `has_more` — mitigated, not eliminated.** Buckets are still capped
  per group with no server-side flag, so a total derived from array lengths
  remains a floor. The `N+` badge removes the misleading part, which was the
  defect; the exact count stays unavailable by choice. No backlog task spawned —
  revisit only if a consumer needs true totals, at which point it belongs with
  whatever search work is live then.
