---
id: '0382'
title: 'Automated verify-range vs Horizon/stellar.expert harness (never-silently-miss contract)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-testing, validation]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker §11 (MISSING pattern: automated verify-range). G-role-crossref = the contract.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Split it — the two named oracles are valid for opposite halves of what
      we index, and one of them is on a clock.** Findings from 0432's tooling
      survey (verified 2026-07-22 against upstream repos and release tags, not
      from memory):
      **Horizon — fine for classic, useless for Soroban.** It indexes classic
      Stellar (payments, trustlines, offers, accounts) authoritatively, so the
      classic half of this harness is sound. But Horizon has an announced EOL,
      `stellar/go` was archived 2025-12-16, and its `result_meta_xdr` field is
      marked for deprecation — which is the exact field a self-decoding check
      needs. Build the classic half knowing it expires.
      **stellar.expert is not a valid oracle at all.** Third-party, derivation
      closed-source, and its own two API specs contradict each other on the type
      of `creator`. Task 0256 is the cautionary case: it disagreed with us, we
      dismissed it, and only raw XDR settled the argument — in their favour.
      Never use it as the arbiter; at most as a tie-break prompt to go read
      bytes.
      **For the Soroban half, the oracle must be Galexie output, Hubble's
      `*_xdr` columns, or raw XDR decoded with the official `stellar` CLI.** The
      docs corpus contains exactly one source-of-truth statement — "The source of
      truth should always be the XDR defined in the protocol" — and no official
      document recommends validating an indexer against a hosted service.
      Coordinate with **0431**, which is building a differential oracle against
      `stellar-xdr`; these two overlap and should not be built twice.
---

# Automated verify-range vs Horizon harness

## Summary

Build an automated harness that verifies our indexed activity for a ledger range
against an external source of truth (Horizon / stellar.expert), so coverage gaps
are caught mechanically instead of by manual spot-check.

## Context

Spawned from 0359 §11 (MISSING pattern). The whole 0359 audit rested on
"never silently miss" — but there is no automated contract enforcing it. The
0359 `notes/G-role-crossref.md` is the informal spec of what SHOULD appear.

## Implementation

- For a ledger range, diff our per-asset / per-account / per-op activity against
  Horizon (and stellar.expert where it exposes more, e.g. per-asset native tx).
- Report missing / extra rows per dimension (asset, account, op).
- Wire as a CI-runnable check (or an ops script) so backfill + live drift is
  caught. This is the validation gate for 0379's backfill acceptance too.

## Acceptance Criteria

- [ ] harness diffs indexed activity vs Horizon for a ledger range
- [ ] reports missing/extra per dimension
- [ ] runnable in CI or as an ops validation script
