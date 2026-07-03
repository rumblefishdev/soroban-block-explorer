---
id: '0352'
title: 'FEATURE: surface WHY a tx failed — prominent fail-reason banner (ScError type+code+message)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: []
tags:
  [frontend, transaction-detail, soroban, ux, priority-medium, effort-medium]
links: []
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
---

# FEATURE: prominent fail-reason banner on failed transactions

## Summary

When a Soroban transaction fails, the explorer shows only the `Failed` status;
the actual reason (a Soroban `ScError`) lives buried in the advanced-mode
"Diagnostic events" section as raw ScVals. Surface a clear, human-readable
**fail-reason banner** near the top of a failed tx detail — e.g.
`Failed · Auth/ExistingValue — "nonce already exists for address"`. The data is
already available on-demand (no indexing needed); this is presentation + a small
decoder fix.

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

## Acceptance Criteria

- [ ] Failed tx detail shows a prominent `Failed · <type>/<code> — <message>` banner
- [ ] Decoder surfaces the numeric code (not just the type name)
- [ ] Verified on all 4 fixtures above (Auth, Contract, Budget, Value)
- [ ] Advanced Diagnostic events section unchanged (still available)
- [ ] **Docs updated** — N/A unless a new API field is added (if the banner is
      server-composed, document it in the tx-detail contract docs).
- [ ] **API types regenerated** — required IF Step 1/2 change the `heavy` DTO
      shape (`crates/api/**` + `libs/api-types/**` → `nx run @rumblefish/api-types:generate`).

## Notes

- No indexing needed for the detail banner — heavy already decodes it on-demand.
- `WasmVm` is near-absent in prod; the banner should still handle it gracefully
  (generic "VM trap" with whatever message exists).
