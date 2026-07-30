---
id: '0352'
title: 'FEATURE: surface WHY a tx failed — fail-reason banner for both Soroban ScErrors and classic operation results'
type: FEATURE
status: backlog
related_adr: []
related_tasks: []
tags:
  [
    frontend,
    backend,
    api,
    transaction-detail,
    soroban,
    ux,
    priority-high,
    effort-medium,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/364'
history:
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      From a Slack thread: on a failed Soroban tx the reason (Soroban error)
      is only visible buried in advanced-mode Diagnostic events as raw ScVals.
      We already HAVE the data (heavy block, on-demand archive decode) — the
      ask is to surface a clear "why failed" banner higher up, plus a decoder
      fix so the numeric code isn't dropped. Root-caused with real examples.
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      **Scope hole — this task as written covers only Soroban failures.** It
      assumes every failure is an `ScError` in a diagnostic event, which is
      false for classic operations. Counter-example from an external report:
      transaction `7af6d0edad166f2ec276fc75e13d0613c70d9476c164db943ce64e183a44f6c5`
      is `txFAILED` with three classic operations and no contract at all —
      `BEGIN_SPONSORING_FUTURE_RESERVES` succeeded, `CREATE_ACCOUNT` failed
      with `LOW_RESERVE`, the third was rejected `opNO_ACCOUNT` (decoded from
      `result_xdr`; Horizon agrees `successful: false`). No contract means no
      diagnostic events, so Advanced view genuinely shows nothing — the
      reporter said exactly that and was right. The reason lives in the
      per-operation result codes, which the API does not expose: the parser
      reads `op_results` only for pool claims and counterparties
      (`crates/xdr-parser/src/operation.rs`), and the DTO carries
      transaction-level `result_code` plus raw `result_xdr`, nothing between
      (`crates/api/src/runtime_enrichment/stellar_archive/dto.rs:32-42`).
      So covering classic failures needs a backend change this task has not
      scoped. Decide whether that belongs here or in a sibling task before
      starting; shipping only the Soroban half would leave the reported
      transaction just as unexplained as it is today.
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Decision on the above: **widened, not split.** Classic operation failures
      join this task as Step 6, and the task grows from frontend-only to
      frontend + backend + API (tags updated). Reasoning: a sibling task would
      let the Soroban half ship on its own and invite closing the reporting
      issue on it, while the transaction that was actually reported stayed
      blank — the banner has one job, "say why", and which XDR structure the
      reason came from is our problem, not the reader's. Cost of the choice:
      this is no longer a small frontend change and should not be sized as one.
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Priority raised to high: the reason a transaction failed must be
      visible at the very top of the page — the 0453 redesign shipped the
      failed-status strip (0460 #8), which is exactly where the reason line
      from this task should land. The strip already passes through tx-level
      `result_code` when it says more than txFAILED; this task replaces that
      with the real per-op / ScError reason.
---

# FEATURE: prominent fail-reason banner on failed transactions

## Summary

When a transaction fails, the explorer shows only the `Failed` status. Surface a
clear, human-readable **fail-reason banner** near the top of a failed tx
detail — e.g. `Failed · Auth/ExistingValue — "nonce already exists for address"`
for a Soroban error, or `Failed · CREATE_ACCOUNT — LOW_RESERVE` for a classic
one. Two different XDR sources, one line for the reader.

For Soroban failures the reason is already available on-demand (no indexing
needed) and this is presentation plus a small decoder fix. **Classic failures
are the harder half** — the per-operation result codes that carry the reason are
not exposed by the API at all. See Step 6.

## Context — what we already have vs what's missing

- **Have it (on-demand):** the tx detail endpoint's `heavy` block (ADR 0029,
  archive XDR decode) already returns `diagnostic_events` + `result_code`. The
  advanced view renders diagnostic events
  (`web/src/pages/transaction-detail/advanced/EventsSection.tsx`).
- **NOT indexed:** diagnostic events / the error are **not** in ClickHouse — no
  result/error column on `transactions`. They are decoded live per request. (We
  index _contract_ events in `soroban_events`, but not _diagnostic_ events.)
  → So a detail-page banner needs **no indexing**; only list/filter would.
- **Decoder drops the code:** `crates/xdr-parser/src/scval.rs:19` renders an
  error ScVal as `("error", e.name())` — i.e. only the TYPE NAME (`"Auth"`,
  `"Contract"`), dropping the numeric code. So the UI shows `Error("Auth")` but
  not `Auth/ExistingValue`, and `Error("Contract")` but not the contract's u32.

## Soroban error model (reference)

Every failure is an `ScError = (type, code)`:

- **`SCErrorType`** (10): `Contract`(0, user u32 code), `WasmVm`, `Context`,
  `Storage`, `Object`, `Crypto`, `Events`, `Budget`, `Value`, `Auth`(9).
- **`SCErrorCode`** (10, for all types except Contract): `ArithDomain`,
  `IndexBounds`, `InvalidInput`, `MissingValue`, `ExistingValue`,
  `ExceededLimit`, `InvalidAction`, `InternalError`, `UnexpectedType`,
  `UnexpectedSize`.
- Rule: `Contract` → code is the contract's own `u32` (`#[contracterror]`
  variant). Any other type → code is an `SCErrorCode` (host/standard).
- The event topic is `[ Symbol("error"), Error(<type>) ]`; the message +
  numeric code live in the event `data`.

## Real examples (verified live, use as fixtures)

Sampled ~74 real failed Soroban txs — distribution: `Auth` + `Contract` dominate;
`Budget`/`Storage`/`Value` appear; **`WasmVm` (raw panic/VM trap) = 0** (near-absent
in prod — a Rust panic usually surfaces as a typed host error or `Contract`, not
`WasmVm`).

| Type                       | tx hash                                                            | data (message / code)                                                |
| -------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------- |
| **Auth** (host)            | `716e50119efa7705a5a213d70c164d1a52eedb9182e631b1e10c6bb76e2c0b01` | "nonce already exists for address" + address                         |
| **Contract** (user code)   | `9bdcaba0777e481301c2191538f8e13fea8489aebe3088368a4be51e55922432` | "failing with contract error", u32 `7`                               |
| **Budget** (host)          | `c699738038906921e32d7e5d76e6ffd2258fa5ac49d6c3e977bce10080117d00` | "operation instructions exceeds amount specified", 9812442 > 9812025 |
| **Value** (host, archival) | `2a06fd61f5a275941ecbefb4d5fc8461efe72505c08ec3c95cab40d5f9322255` | "trying to access an archived contract data entry" + address         |

## Root cause — verified end-to-end 2026-07-30 (fixture `7af6d0ed…`)

Traced the live request on the dev proxy against production data. The reason is
present in every response and is dropped by our own code at layer 2 of 5:

1. **XDR has it.** `heavy.result_xdr` (48 bytes, shipped raw in the response)
   decodes to `feeCharged=300, txFAILED, 3 op results`:
   `BEGIN_SPONSORING_FUTURE_RESERVES=0 (success)`,
   `CREATE_ACCOUNT=-3 (LOW_RESERVE)`, `op-level disc -2 (opNO_ACCOUNT)`.
   `TransactionResultResult::TxFailed(VecM<OperationResult>)` — the failing
   union arm carries the same per-op array as the successful one.
2. **The parser throws it away.** `tx_op_results()`
   (`crates/xdr-parser/src/operation.rs:97-106`) matches only `TxSuccess` and
   `TxFeeBumpInnerSuccess` → returns `None` for `TxFailed` **and**
   `TxFeeBumpInnerFailed`. It was written for pool claims (which only exist on
   success), but it is the single gateway to op results, so on a failed
   transaction the reason array never reaches any consumer — indexer or the
   live heavy path alike.
3. Even with the array in hand, `extract_op_details` consumes `op_result` only
   for `poolIds`/`claimedAtoms` and counterparties — no result code is emitted.
4. The DTO has no per-op result field; only tx-level
   `result_code = "TxFailed"` (`transaction.rs:123`, `.name()` of the union arm).
5. The banner suppresses `"TxFailed"` as noise
   (`web/src/pages/transaction-detail/sections/TransactionSummary.tsx`) — correct
   given it means only "some operation failed", so the strip shows no reason.

Also confirmed on this fixture: `diagnostic_events: 0` (classic tx, no
contract) — the #364 reporter was right that Advanced view shows nothing, and
this is why the Soroban-only half of the plan cannot cover it.

**Fix order implied:** widen `tx_op_results` to the failed arms FIRST (one
match arm; everything downstream is blocked on it), then emit the code, then
the DTO field, then the banner line.

## Implementation Plan

### Step 1 — decoder: keep the code (`scval.rs`)

Change the error ScVal render so it carries `type` + `code` (not just
`e.name()`): `Contract` → `{ type: "Contract", code: <u32> }`; others →
`{ type: "<Type>", code: "<ScErrorCode>" }`. Keep back-compat for existing
consumers (add code alongside, don't break the `value` field).

### Step 2 — pick the fail reason

Server- or FE-side: from `heavy.diagnostic_events`, select the primary error
event (topic `[Symbol("error"), Error(...)]`) and its `data` message; combine
with `result_code`. Prefer the innermost contract error over the wrapping
"escalating error to VM trap…" event.

### Step 3 — banner UI (transaction-detail, normal view)

Render a compact banner near the `Failed` status:
`Failed · <Type>/<code> — "<message>"`. Only on failed txs. Keep the full
Diagnostic events list in advanced as-is.

### Step 4 (optional, separate) — contract code → name

For `Contract` errors, map the u32 to the `#[contracterror]` variant name using
the ingested ABI (`wasm_interface_metadata`) → e.g. `Contract/7 (InsufficientBalance)`.

### Step 5 (optional, separate) — index for list/filter

If a fail-reason is wanted on the tx LIST or as a filter, index error type+code
into CH at ingestion (the list can't archive-fetch per row). Out of scope for
the detail banner.

### Step 6 (REQUIRED, added 2026-07-28) — classic operation failures

Steps 1–3 only ever explain Soroban failures. A transaction with no contract in
it has no diagnostic events, so the banner would stay blank on exactly the case
that prompted this — see the history note for the decoded counter-example. The
reason for a classic failure lives in the per-operation result codes, which the
API does not expose at all today.

- **Backend.** Surface each operation's result code. The parser already receives
  `op_results` (`crates/xdr-parser/src/operation.rs`) but consumes them only for
  pool claims and counterparties; the DTO carries transaction-level `result_code`
  and raw `result_xdr` with nothing between
  (`crates/api/src/runtime_enrichment/stellar_archive/dto.rs:32-42`). Add a
  per-operation `result_code` to the operation DTO.
- **Naming.** These are XDR enum discriminants per operation type
  (`CREATE_ACCOUNT_LOW_RESERVE`, `PATH_PAYMENT_STRICT_SEND_UNDER_DESTMIN`, …),
  plus the operation-level rejections (`opNO_ACCOUNT`, `opBAD_AUTH`). Prefer the
  library's own names over a hand-rolled table — the same lesson as 0431.
- **Banner.** One reason line whichever the source: `Failed · CREATE_ACCOUNT —
LOW_RESERVE` reads the same as `Failed · Auth/ExistingValue — …`. The reader
  does not care which XDR structure it came from.
- **Do not ship Steps 1–3 alone.** Half a banner that stays blank on classic
  failures is indistinguishable from the current behaviour for the transaction
  that was actually reported.

## Acceptance Criteria

- [ ] Failed tx detail shows a prominent `Failed · <type>/<code> — <message>` banner
- [ ] Decoder surfaces the numeric code (not just the type name)
- [ ] Verified on all 4 fixtures above (Auth, Contract, Budget, Value)
- [ ] **Classic-failure fixture also verified** — `7af6d0ed…` renders
      `CREATE_ACCOUNT — LOW_RESERVE`, not a blank banner
- [ ] Per-operation result codes exposed on the operation DTO
- [ ] Advanced Diagnostic events section unchanged (still available)
- [ ] **Docs updated** — N/A unless a new API field is added (if the banner is
      server-composed, document it in the tx-detail contract docs).
- [ ] **API types regenerated** — required IF Step 1/2 change the `heavy` DTO
      shape (`crates/api/**` + `libs/api-types/**` → `nx run @rumblefish/api-types:generate`).

## Notes

- No indexing needed for the detail banner — heavy already decodes it on-demand.
- `WasmVm` is near-absent in prod; the banner should still handle it gracefully
  (generic "VM trap" with whatever message exists).
