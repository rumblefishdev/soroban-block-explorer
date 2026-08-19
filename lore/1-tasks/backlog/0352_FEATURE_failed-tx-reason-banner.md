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
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Core implemented on the 0462 branch (see "Implementation progress"):
      per-op result codes end-to-end (parser accessor → heavy DTO →
      regenerated API types → banner reason line), verified against the
      real archive on the 7af6d0ed… fixture. Steps 1-2 (ScError decode with
      numeric code, error-event picking) and Step 4 (code→name via ABI)
      remain. Fruit lands at the next backend deploy.
  - date: '2026-08-19'
    status: backlog
    who: karolkow
    note: >
      Deployed and hardened since the last note. The classic half and the banner
      line shipped in 645c0711 and went out with the 2026-08-17/18 production
      deploys, so "live page check pending backend deploy" is no longer pending.
      cdc07522 (2026-08-18) then fixed the op-to-result-code join: it mapped a
      1-based operation_index onto a 0-based array with saturating_sub, so an
      index of 0 would have shown operation 1's code beside the wrong operation —
      a real code in the wrong place, which reads as fact and nothing contradicts.
      checked_sub makes that an absence, and two offline unit tests now cover the
      arithmetic that only a network-gated #[ignore]d test had touched.
      Remaining scope is narrower than the step list implies: Step 2 is largely
      covered by 0462's execution trace, which already shows the diagnostic
      message and its numeric code at the stop point. What is genuinely left is
      Step 1 (scval.rs still renders an error ScVal as e.name() only, dropping the
      code) plus feeding that ScError into the existing banner line, so a Soroban
      failure reads Auth/ExistingValue - "nonce already exists for address"
      instead of the truthful but generic Invoke Host Function #1 - TRAPPED.
      Step 4 (contract code to name via ABI) stays optional and unstarted.
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
  (`web/src/pages/transaction-detail/sections/EventsSection.tsx`).
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

### Step 2 — pick the fail reason — 🟡 mostly covered by 0462

0462's execution trace already selects the failure diagnostic and renders its
message with the numeric code inline — `error("failing with contract error", 7)`
— and marks the node where execution stopped
(`web/src/pages/transaction-detail/op-card/ExecutionTrace.tsx`). So the _why_ is
on the page today, one level below the banner.

What is left of this step is promoting that reason into the top line: pick the
primary error event (topic `[Symbol("error"), Error(...)]`) and its `data`
message, preferring the innermost contract error over the wrapping "escalating
error to VM trap…" event, and hand it to the existing `opFailReason` slot in
`TransactionSummary.tsx`.

### Step 3 — banner UI — ✅ DELIVERED (645c0711, deployed 2026-08-17/18)

The failed-status strip renders on failed txs and carries the per-op reason via
`opFailReason` (`web/src/pages/transaction-detail/sections/TransactionSummary.tsx`):
`Create Account #2 — LOW_RESERVE`, with `(+N more failed)` when several ops
failed, `· reason unavailable` when `heavy` is missing, and the tx-level code
only when it says more than the sentence already did. The `<Type>/<code> —
"<message>"` shape for Soroban failures is what Steps 1–2 still add.

### Step 4 (optional, separate) — contract code → name

For `Contract` errors, map the u32 to the `#[contracterror]` variant name using
the ingested ABI (`wasm_interface_metadata`) → e.g. `Contract/7 (InsufficientBalance)`.

### Step 5 (optional, separate) — index for list/filter

If a fail-reason is wanted on the tx LIST or as a filter, index error type+code
into CH at ingestion (the list can't archive-fetch per row). Out of scope for
the detail banner.

### Step 6 (REQUIRED, added 2026-07-28) — classic operation failures — ✅ DELIVERED

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

## Implementation progress (2026-07-30, feat/0462 branch)

The classic half (Step 6) + the banner line are BUILT; awaiting deploy:

- **Parser** (`crates/xdr-parser/src/operation.rs`): `tx_op_results_any()` —
  companion accessor unwrapping the failed arms (`TxFailed`,
  `TxFeeBumpInnerFailed`); `tx_op_results` deliberately untouched so claim
  atoms stay success-gated (0261 phantom-crossing guard lives at the
  accessor, and the two consumers now use different accessors).
  `op_result_code()` — per-op name from the XDR library's own `name()`
  (27-arm unwrap of `OpInner`, op-level rejections pass through), no
  hand-rolled table (0431 lesson). Unit test mirrors the fixture shape.
- **Heavy path** (`extractors.rs` + `dto.rs`): `XdrOperationDto.result_code`
  (nullable), populated from `tx_op_results_any`; claims path unchanged.
  Off-by-one guarded: `operation_index` is 1-based, the XDR array 0-based.
  API types regenerated. Indexer untouched — codes are detail-page-only,
  no indexing (as planned).
- **E2E against the real archive**: network-gated test
  `e3_failed_tx_ops_carry_result_codes` fetches ledger 63687496 and asserts
  `["Success", "LowReserve", "OpNoAccount"]` on `7af6d0ed…` — the exact
  hand-decoded root-cause sequence.
- **Banner** (`TransactionSummary.tsx`): `opFailReason()` — first failing
  op as `Create Account #2 — LOW_RESERVE`, `(+N more failed)` when later
  ops also failed (a tx fails when ANY op fails — count, don't hide);
  falls back to the tx-level code passthrough when the per-op array is
  absent (validation-level failures, pre-deploy responses). Code display is
  a pure case transform (CamelCase → SCREAMING_SNAKE) of the library name.
- **Docs**: frontend-overview §6.4 (banner + consumed heavy fields),
  xdr-parsing-overview (accessor pair + who consumes which).

## Acceptance Criteria

- [x] Failed tx detail shows the reason on the failed strip (built; per-op
      code line — the `<type>/<code> — <message>` ScError refinement for
      Soroban failures is Steps 1-2, still open)
- [ ] Decoder surfaces the numeric code (not just the type name)
- [ ] Verified on all 4 fixtures above (Auth, Contract, Budget, Value) —
      those get `Invoke Host Function #1 — TRAPPED` from the per-op code
      today (truthful, less specific than the ScError line will be); the
      0462 execution trace already shows their error events with message +
      code at the stop point
- [x] **Classic-failure fixture also verified** — `7af6d0ed…` asserts
      `Success/LowReserve/OpNoAccount` end-to-end against the real archive
      (network-gated test); banner render pinned by unit test. Shipped to
      production with the 2026-08-17/18 deploys; the index-mapping bug found
      afterwards is fixed and covered offline (`cdc07522`).
- [x] Per-operation result codes exposed on the operation DTO
- [x] Advanced Diagnostic events section unchanged (still available)
- [x] **Docs updated** — frontend-overview §6.4 + xdr-parsing-overview
- [x] **API types regenerated** — `openapi.json` + `generated/*` in the
      same change as the DTO field

## Notes

- No indexing needed for the detail banner — heavy already decodes it on-demand.
- `WasmVm` is near-absent in prod; the banner should still handle it gracefully
  (generic "VM trap" with whatever message exists).
