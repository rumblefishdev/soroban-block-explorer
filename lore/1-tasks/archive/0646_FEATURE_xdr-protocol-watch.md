---
id: '0646'
title: "XDR protocol watch: page when our indexer's stellar-xdr lags the protocol core supports"
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0610', '0548', '0368']
tags: [xdr, ingestion, alarms, resilience, priority-high, effort-small]
links:
  - https://github.com/rumblefishdev/stellar-prices-api/issues/404
  - https://github.com/rumblefishdev/stellar-prices-api/blob/develop/tools/scripts/verify-xdr-protocol-gap.mjs
history:
  - date: '2026-10-16'
    status: active
    who: karolkow
    note: >
      Created after stellar-prices-api issue #404 (stellar-xdr 28 lags core
      30) reached us by hand: our Galexie watch (0610) stayed green, correctly,
      because it reads only the Galexie image, and nothing here reads the
      stellar-xdr pin. Decided: port the prices-api watch as it is.
  - date: '2026-10-16'
    status: active
    who: karolkow
    note: >
      Delivery moved from a GitHub workflow + issue to the 0610 Lambda and its
      Slack alarm: prices-api's issue was a workaround for a missing Slack path.
  - date: '2026-10-19'
    status: completed
    who: karolkow
    note: >
      Shipped in #669 and #678, deployed with production-2026.10.19-1 (Compute,
      STELLAR_XDR_MAJOR=30 on the indexer) and -2-CloudWatch. A manual run of
      the watch logged "OK: our indexer's stellar-xdr 30". Infra suite 78/78.
---

# XDR protocol watch

## Summary

Extend the Galexie protocol watch (0610) with a second check: the
`stellar-xdr` major our indexer was built with against Horizon's
`current_protocol_version` / `core_supported_protocol_version` and crates.io.
Same Lambda, same 3-hour schedule, same Slack alarm. The tiers (OK, WAITING,
BUMP, could-not-check) follow stellar-prices-api's
`verify-xdr-protocol-gap.mjs`.

## Context

The indexer decodes every ledger with `stellar-xdr` (workspace pin `28`).
The Galexie watch only compares captive-core versions of images, so a
protocol whose XDR our crate cannot decode would surface first as an
`ingestion-stall`. On 2026-10-16 Horizon reported core `30.0.0` while mainnet
runs 29, and `stellar-xdr 30.0.0` is on crates.io; prices-api opened its issue
on 2026-10-15, we had nothing.

## Decisions so far

- 2026-10-16 — port the prices-api watch (tiers, crate rule, test matrix).
- 2026-10-16 — deliver through the 0610 Lambda and its Slack alarm, not a
  GitHub workflow + issue: prices-api used an issue only because it had no
  Slack path; we have one.
- 2026-10-16 — the watch reads the pin from the deployed indexer
  (`STELLAR_XDR_MAJOR`, set from `Cargo.toml` by the Compute stack in the same
  deploy that builds the binary), so it measures what runs. Re-weighed
  2026-10-19 against a pin read from `develop`: kept, because the case the
  alarm must catch is a bump merged but not deployed (prices-api 0091/0094).
- 2026-10-16 — no lock-file checks: a caret pin cannot resolve another major,
  and a split between our crates and `soroban-env-host` is a compile error.
- 2026-10-16 — the bump target is a `soroban-env-host` release on a newer
  `stellar-xdr` (followed through `soroban-env-common`), not `stellar-xdr`
  alone: `contract-executor` moves the two together, and on 2026-10-16
  `stellar-xdr` 30 was out with no env-host on it — the watch would have paged
  red for a bump nobody could build, hiding a NEW CORE on the same alarm.
- 2026-10-16 — the BUMP runbook steps say to warn the stellar-prices-api
  team before the bump merges: their build takes our `xdr-parser` from
  `develop` and pins `stellar-xdr` exactly.

- 2026-10-19 — the two checks keep one alarm. While one holds it red, the
  other's news reaches Slack only through the function's log, which carries
  both lines; a second alarm was weighed and declined as more code for a
  rare overlap (both only before a vote whose protocol changes the XDR).
- 2026-10-19 — kept over a simpler "newer soroban-env-host major" rule: that
  rule pages for an env-host bump that changes no XDR (protocol 29) as if it
  were urgent, and cannot tell before the vote from after it.

- 2026-10-19 — one BUMP verdict instead of prices-api's LAGGING / BEHIND: the
  alarm only has to fire; whether mainnet has voted is read by hand.

## Out of scope

- A watch on testnet. The watch exists only where our own Galexie does, and
  reads mainnet Horizon; testnet reads the public lake. Belongs to the
  testnet environment task (0553).

## Acceptance Criteria

- [x] `xdr.mjs` cases and `stellarXdrMajor` tests pass (infra 78/78); the
      synth test shows the indexer env and the watch's one read permission.
- [x] Compute deployed before CloudWatch (tags `production-2026.10.19-1`,
      `-2-CloudWatch`); a run logged both lines, `OK: … core 29.0.0` and
      `OK: our indexer's stellar-xdr 30`.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §6, §8.2; runbooks `galexie-protocol-watch.md`, `health.md`.
- [x] **API types regenerated** — N/A — no `crates/api/**` or Cargo change.

## Implementation Notes

- `infra/lambdas/galexie-protocol-watch/xdr.mjs` (pure verdict: OK /
  WAITING / BUMP) and the I/O in `index.mjs`: the indexer's
  `STELLAR_XDR_MAJOR` via `GetFunctionConfiguration`, Horizon, and the
  crates.io listing of `soroban-env-common` (#678 cut the env-host hop).
- `infra/src/lib/stellar-xdr-pin.ts` reads the workspace pin at synth;
  `compute-stack.ts` sets it on the indexer; the watch gets the indexer from
  Compute through the CloudWatch stack.

## Issues Encountered

- `vitest`'s 5 s default failed a CDK synth test on the release PR (5.1 s
  beside the web tests); infra `testTimeout` raised to 30 s.

## Design Decisions

### Emerged

1. **Both checks always run** and a thrown check becomes "not ok", so one
   failing read never hides the other line.
2. **The watch reads the indexer through Compute's existing export**, not a
   copied function name.
