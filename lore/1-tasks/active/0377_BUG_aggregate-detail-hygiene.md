---
id: '0377'
title: 'Aggregate/detail hygiene: archive-unavailable counts assert zero, participant list truncation, stale estimate docs'
type: BUG
status: active
related_adr: []
related_tasks: ['0359', '0329', '0420', '0453']
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

| Item                                                       | Where it was closed      | Proof on develop `7a99a1a9`                                                                                                                                                                                                       |
| ---------------------------------------------------------- | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **K4-1** invocations KPI 7d vs all-time list               | frontend labelling       | `ContractSummary.tsx:29-36` renders `Invocations (last ${stats.stats_window})`; `ContractsTable.tsx:76` header `Invocations (7d)`; `contracts/queries.rs:550-554` test locks label ↔ SQL                                          |
| **K4-2 / K1-4** `operation_count` vs folded `operations[]` | 0329, frontend           | `operationEntries.ts:41-48` drives the picker from unfolded `heavy.operations`, folded light rows only as fallback; `OperationsSection.tsx:23-25` takes the header from `operation_count`; 2 tests                                |
| **K4-5** nullable-aggregate decode 500                     | 0324 + review discipline | Every aggregate decode site guarded (`ifNull`/`coalesce`/`toNullable`/`toString`→`Option`/`count()` over non-Nullable); regression test `contracts/queries.rs:591-597`; **zero** new unguarded sites across the 419-commit window |

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

Fix for F1+F2 is one change: lift `HeavyUnavailable`
(`op-card/OperationJsonDetail.tsx:109-120`, currently file-local, the only copy
in the repo) into a shared module, thread `heavy_fields_status`, and swap the
meta to `—` with the warning EmptyState when unavailable.

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

Structural too, not luck: all four `ALTER TABLE accounts DELETE` sites in the
repo are inside `#[cfg(test)]` teardown, so production never removes an account
row and the surrogate always resolves.

Blast radius if it ever did break: 82 of 26_487 pools hold more than one page
(>20 participants), 17 hold >100, largest 684 — in the other 26_405 every
participant fits on one page, where truncation is impossible by definition.

**Verdict: latent defect, not a live bug.** Earlier framing as "the most severe
backend item" was a mis-call — the mechanism was traced but the trigger never
checked.

**Fix taken: the tripwire only.** Replaced the silent `?` with a
`tracing::error!` naming the surrogate and pool, and recorded the measurement in
the comment. Deliberately NOT restructuring pagination or re-deriving the count
from the resolved set — that is real complexity against a case that cannot
currently arise. The log makes the invariant observable, so if a write-path
change ever breaks it we get a CloudWatch hit
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

### F5 — accounts list vs detail disagree on what a zero balance is

List subquery `accounts/queries.rs:236-239` filters only `a.asset_type = 0`;
detail `:422` adds `AND b.amount != 0`. An account whose native row is exactly
zero shows `xlm_balance: "0"` in the list and has no native entry in the
detail's `balances[]`. `accounts/dto.rs:34-43` documents `null` when no native
row exists, without disclosing that the two endpoints disagree on "exists".

### F6 — contract list ships a windowed count with no window label

`ContractListItem` (`contracts/dto.rs:25-42`) carries `recent_invocations` but
no `stats_window`; the detail's `ContractStats` has one (`dto.rs:54`). Numbers
are consistent — both resolve the bound from `STATS_WINDOW_DAYS`
(`queries.rs:53,269,486`) — only the wire label is missing for list consumers.
`stats_window_label()` already exists at `queries.rs:60`.

### F7 — same pattern, eight more sites outside the transaction page

The sweep that found the `TransactionSummary` trio also found the shape
elsewhere. Not fixed here — listed so the next pass has the inventory.

| Sev | Site                                                             | Collapses into                                                                                                                             |
| --- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| MED | `OperationsSection.tsx:41-44` → `OperationCard.tsx`              | trace / authorized-calls / events / route strip silently vanish on archive miss — the call reads as "made no sub-calls"                    |
| MED | `operationEntries.ts:42-45`                                      | falls back to folded light rows while the card header says `operation_count`; also conflates "heavy present, zero ops" with "heavy absent" |
| MED | `ledgers/LedgerTransactions.tsx:33-38`                           | "This ledger closed without any transactions" while the same screen shows a non-zero `transaction_count`                                   |
| MED | `pool-detail/PoolParticipants.tsx:117-121`                       | "No participants yet" against a non-zero `participant_count` — F3 makes this a live failure mode, not hypothetical                         |
| MED | `search/useSearchResults.ts:85-90` + `SearchResultsTabs.tsx:118` | a capped bucket renders a hard `10` that reads as a total; `10+` would be honest                                                           |
| MED | `shared/humanizeOp.ts:110,122`                                   | asset defaults to `XLM` when `details` is absent — nothing promises `asset_code: null ⟺ native`                                            |
| LOW | `op-card/opFacts.ts:31`                                          | `Received: —` on a successful swap reads as "received nothing"                                                                             |
| LOW | `contracts/ContractInterface.tsx:208-216`                        | "No public interface" names a specific cause; a not-yet-parsed contract is told it is a SAC                                                |

## Acceptance Criteria

- [x] F1 — signatures section distinguishes "archive unavailable" from "no signatures"; no `0 signatures` meta when `heavy_fields_status == "unavailable"`
- [x] F2 — events + raw-data sections likewise; `HeavyUnavailable` extracted to a shared module and reused by all three
- [x] F1/F2 extension — memo, fee source and fail reason on the same page no longer collapse "not loaded" into a definite claim
- [x] F3 — trigger measured absent on prod (0/6010) and structurally unreachable; unresolved account no longer dropped silently — `tracing::error!` makes it observable. Pagination restructure and single-basis count deliberately skipped as unjustified complexity; revisit only if the log ever fires
- [x] F4 — `network/dto.rs` docs describe the actual mechanism (deduped read-time count, accounts lag MV refresh ≤2 min); API types regenerated; frontend left unchanged
- [ ] F5 — list and detail apply one zero-balance rule; DTO doc states it
- [x] F6 — `ContractListItem` carries the window label; API types regenerated
- [x] `nx run @rumblefish/api-types:generate` run and staged for every `crates/api/**` change (F4, F6 — re-run when F5 lands)

## Docs updated

- `docs/architecture/**` — N/A: no endpoint added/removed, no schema or pipeline
  change. F4/F6 alter DTO doc text and add one optional wire field; F1–F3/F5 are
  render-side and query-filter fixes.

## Future Work

- Search buckets are capped per group with no `has_more` flag, so a frontend
  total derived from array lengths is a floor, not a truth. Out of scope —
  spawn if the singleton-navigation heuristic proves wrong.
