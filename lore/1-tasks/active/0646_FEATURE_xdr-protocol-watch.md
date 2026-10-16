---
id: '0646'
title: "XDR protocol watch: page when our indexer's stellar-xdr lags the protocol core supports"
type: FEATURE
status: active
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
---

# XDR protocol watch

## Summary

Extend the Galexie protocol watch (0610) with a second check: the
`stellar-xdr` major our indexer was built with against Horizon's
`current_protocol_version` / `core_supported_protocol_version` and crates.io.
Same Lambda, same 3-hour schedule, same Slack alarm. The tiers (OK, WAITING,
LAGGING, BEHIND, could-not-check) are stellar-prices-api's
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
  deploy that builds the binary), so it measures what runs.
- 2026-10-16 — no lock-file checks: a caret pin cannot resolve another major,
  and a split between our crates and `soroban-env-host` is a compile error.
- 2026-10-16 — the bump target is a `soroban-env-host` release on a newer
  `stellar-xdr` (followed through `soroban-env-common`), not `stellar-xdr`
  alone: `contract-executor` moves the two together, and on 2026-10-16
  `stellar-xdr` 30 was out with no env-host on it — LAGGING would have paged
  red for a bump nobody could build, hiding a NEW CORE on the same alarm.
- 2026-10-16 — the LAGGING runbook steps say to warn the stellar-prices-api
  team before the bump merges: their build takes our `xdr-parser` from
  `develop` and pins `stellar-xdr` exactly.

## Acceptance Criteria

- [ ] `xdr.mjs` tier matrix and `stellarXdrMajor` tests pass; synth test
      shows the indexer env and the watch's read permission.
- [ ] Compute deployed before CloudWatch; the watch logs both lines.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §6, §8.2; runbooks `galexie-protocol-watch.md`, `health.md`.
- [ ] **API types regenerated** — N/A — no `crates/api/**` or Cargo change.
