---
id: '0410'
title: 'BUG: SAC event-identity guard on value path — verify emitter == asset SAC (full H2 fix)'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0393']
tags:
  [
    'clickhouse',
    'indexer',
    'xdr-parser',
    'security',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-07-18
    status: backlog
    who: claude
    note: 'Spawned from 0393 future work (H2 interim gate). Code/docs/PR #355 already reference this task id by number.'
---

# BUG: SAC event-identity guard on value path — verify emitter == asset SAC (full H2 fix)

## Summary

Task 0393 shipped an **interim** trust gate ("H2"): a Native/Credit asset
identity taken from a Soroban token event's trailing SEP-11 string topic
(`"USDC:GISSUER…"`) is **not cryptographically bound to the emitting contract**,
so any contract could forge a real-asset amount. Until this task, the value path
writes such claims as `NULL` (a dash) rather than attribute a spoofable figure.
This task closes the gate properly so genuine SAC transfers inside Soroban txs
surface their value again.

## Context

- Interim gate lives in `crates/db-clickhouse/src/persist/stage.rs`
  (`token_events_net_settled`, `~line 2279`): only a bespoke `EventAsset::Contract`
  identity — which IS the emitter — is attributed a value; Native/Credit event
  claims → `policy_null` → `NULL`.
- Cost of the interim: a genuine SAC transfer of a classic asset inside a Soroban
  tx loses its event-derived value (shows a dash). The classic-op path
  (`has_soroban = 0`) is unaffected.
- The full fix was sketched in 0393 as `sac_override_from_event_topics`: prove
  the emitting contract equals the asset's canonical SAC before trusting the
  string topic.

## Implementation

- For a Native/Credit claim from an event, derive the asset's canonical SAC
  contract id (deterministic from the asset + network passphrase, per CAP-46-6 /
  SAC address derivation) and compare against the emitting contract surrogate.
- Match → attribute the value on the classic asset id (as today's presence path
  already resolves). Mismatch → keep `NULL` (now a real spoof signal, worth a
  counter/warn).
- Share the check with the presence path if one already resolves SAC identity,
  so value and presence agree.
- Backfill: re-run the affected reduction so historical Soroban-SAC values fill
  in (subject to the same `max()` ratchet + `OPTIMIZE FINAL` caveat noted in
  0393 Operations).

## Acceptance Criteria

- [ ] A genuine SAC transfer inside a Soroban tx surfaces its net-settled value
      (no longer forced to `NULL`).
- [ ] A forged `"CODE:ISSUER"` topic from a non-SAC emitter still yields `NULL`
      (spoof stays closed) — proven by a mutation test.
- [ ] Value and presence paths resolve the same SAC identity (no drift).
- [ ] `crates/db-clickhouse/src/persist/stage.rs` "interim for 0410" comments
      removed/updated; 0393 task + architecture docs updated (ADR 0032).
