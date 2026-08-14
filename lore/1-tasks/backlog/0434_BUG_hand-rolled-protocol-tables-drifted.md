---
id: '0434'
title: 'BUG: hand-maintained protocol tables have drifted — 8 config-setting variants stored as "unknown", asset_type means two different things'
type: BUG
status: backlog
related_adr: ['0036', '0038', '0051']
related_tasks: ['0431', '0430', '0433']
tags:
  [
    priority-high,
    effort-medium,
    layer-xdr-parsing,
    layer-api,
    data-integrity,
    correctness,
  ]
links:
  - crates/xdr-parser/src/ledger_entry_changes.rs
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Found by a five-agent full-repo sweep (191 Rust files, 333 TS files, infra,
      SQL, scripts, workflows — 100% coverage) looking for hand-rolled protocol
      logic. These are the findings that are **already wrong today**, not
      hypothetical drift. Everything else the sweep found is recorded in 0431
      (duplication) and 0433 (frontend validation).
      The config-setting count I verified myself by diffing our match arms
      against the crate's enum, rather than trusting the agent's report.
---

# BUG: hand-maintained protocol tables have already drifted

Three defects, one cause: protocol vocabularies re-typed by hand instead of
derived from `stellar-xdr`, which we already compile.

## 1. `config_setting_key` — 8 variants silently stored as `"unknown"`

`crates/xdr-parser/src/ledger_entry_changes.rs:534-551` hand-types a
13-variant name table with a `_ => "unknown"` catch-all.

`stellar_xdr::ConfigSettingEntry` has **21** variants. Verified by diffing the
enum against our match arms:

```
ContractLedgerCostExtV0   ContractParallelComputeV0
FreezeBypassTxs           FreezeBypassTxsDelta
FrozenLedgerKeys          FrozenLedgerKeysDelta
LiveSorobanStateSizeWindow  ScpTiming
```

All eight land in the catch-all. **This is live, not a future risk** — these are
Protocol 22/23 settings that exist on mainnet now.

The library exposes `ConfigSettingEntry::name()` and **this crate already calls
`.name()` elsewhere** — `operation.rs:427`, `operation.rs:562`,
`transaction.rs:123`. So the correct pattern is in use three files away.

**Fix:** call `.name()`. The hand table cannot be right for longer than one
protocol release.

## 2. `asset_type` — one column, two incompatible meanings

The same `Int16` column is documented with two contradictory enums, and only
value `0` agrees:

| value | project registry (ADR 0036/0038) | XDR `AssetType`   |
| ----- | -------------------------------- | ----------------- |
| 0     | native                           | NATIVE            |
| 1     | classic_credit                   | CREDIT_ALPHANUM4  |
| 2     | SAC                              | CREDIT_ALPHANUM12 |
| 3     | Soroban-native                   | POOL_SHARE        |

Both readings are served to the API **from the same database column**:

- project meaning — `crates/api/src/assets/queries.rs:143-152`,
  `crates/api/src/search/queries.rs:128-137`, `init.sql:281-284`,
  `crates/audit-harness/sql/10_assets.sql:10-44`
- XDR meaning — `crates/api/src/accounts/queries.rs:102-110`,
  `crates/api/src/liquidity_pools/queries.rs:165-172`

`liquidity_pools/queries.rs:163` carries a comment stating a 9-character code is
`asset_type = 2` = credit_alphanum12 — directly contradicting
`10_assets.sql:17` which says type 2 is SAC.

**Someone is being served the wrong label.** Which one depends on the endpoint.
This needs a decision (one meaning, renamed columns if both are needed), not a
patch.

## 3. `format_claimable_balance_id` — matches neither Horizon nor the spec

Two in-crate copies — `ledger_entry_changes.rs:447-451` and
`operation.rs:576-582` — render the id as `hex::encode(hash.0)`, dropping the
4-byte type discriminant.

Result: our id matches **neither** Horizon's 72-character hex **nor** the SEP-23
`B…` StrKey that `stellar_xdr` produces via `Display for ClaimableBalanceId`
(`str.rs:454`). Any cross-tool lookup of a claimable balance by id fails.

## Also worth fixing while in here

- **`decimal7_string_to_i128`** (`db-clickhouse/src/persist/stage.rs:1855`)
  silently truncates input with more than 7 decimals —
  `&frac[..frac.len().min(7)]` — instead of rejecting it.
- **`scval.rs:29-44`** renders u128/i128 as **decimal** (via the library) but
  u256/i256 as **raw hex** (hand-formatted). Same JSON envelope, two number
  encodings; an i256's sign is unrecoverable downstream. The library has
  `Display for Int256Parts` / `UInt256Parts`.
- **`stellar-strkey` resolves to two versions at once** — `0.0.16` pinned
  directly in four crate manifests, `0.0.13` pulled transitively by
  `stellar-xdr 27.0.0` (`Cargo.lock:4207-4238`). StrKeys produced by the
  library's own conversions go through a different encoder build than our direct
  calls. Benign today; should be one version, declared once in
  `[workspace.dependencies]`.

## Acceptance Criteria

- [ ] `config_setting_key` uses `ConfigSettingEntry::name()`; a test asserts our
      output covers every variant the crate defines (so the next protocol bump
      fails the build, not production).
- [ ] `asset_type` has ONE documented meaning per column; if both vocabularies
      are genuinely needed, they live in separately-named columns.
- [ ] Claimable-balance ids render as SEP-23 `B…` StrKeys via the library, and a
      test looks one up against an external source.
- [ ] `decimal7_string_to_i128` rejects over-precision instead of truncating.
- [ ] u256/i256 render in the same encoding as u128/i128.
- [ ] `stellar-strkey` declared once in `[workspace.dependencies]`, one version
      in the lockfile.
- [ ] Docs updated — `docs/architecture/database-schema/**` if `asset_type`
      changes shape (ADR 0032).
- [ ] API types regenerated — required if `asset_type` labels change.
