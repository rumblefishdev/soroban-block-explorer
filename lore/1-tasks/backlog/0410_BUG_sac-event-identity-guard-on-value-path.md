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
    who: karolkow
    note: 'Spawned from 0393 future work (H2 interim gate). Code/docs/PR #355 already reference this task id by number.'
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Deep review: the deterministic guard already exists in sac.rs (derive_sac_contract_id + sac_override_from_event_topics, unit-proven, used by the NFT path). Scope is WIRING not build. Added hybrid (ledger_balance_deltas for account-side legs) + net_id mismatch alarm.'
---

# BUG: SAC event-identity guard on value path — verify emitter == asset SAC (full H2 fix)

> **⚠️ SUPERSEDED (2026-07-20) — do NOT implement this.** This task tried to make
> the spoofable EVENT path trustworthy by crypto-gating it. The net-settled
> redesign removed the event path from value entirely: **value is now read from the
> authoritative LEDGER** (account / trustline / ContractData balance changes), which
> a contract cannot forge — so there is nothing to gate. The whole event-value path
> (incl. the guard, `policy_null`, `sac_rejected`, `net_id` in value) was DELETED.
> The concern (spoofable classic-asset value) is RESOLVED, better, by not trusting
> logs at all. See task 0393 "Value source: the LEDGER, not events" and the
> project-wide audit [0415](0415_AUDIT_authoritative-facts-ledger-not-logs.md).
> Close/archive this on merge — kept only for the trail.

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
- **The guard already exists — this is a WIRING task, not a build** (deep review,
  2026-07-20). `crates/xdr-parser/src/sac.rs` already implements the full
  deterministic binding end-to-end:
  - `derive_sac_contract_id` — `SHA256(XDR(HashIdPreimage::ContractId{ network_id,
ContractIdPreimage::Asset(asset) }))` → the canonical SAC C-StrKey (CAP-46-3).
    Unit-proven against published constants: XLM SAC `CAS3J7GY…`, USDC SAC
    `CCW67TSZ…`.
  - `sac_override_from_event_topics` — parses the trailing SEP-11 asset string,
    derives the SAC id, and gates on `if derived != emitter { return None }`.
    Already used by the live NFT path (`nft.rs`).
  - stellar-xdr 27 exposes the full preimage; `network_id` is already threaded to
    the ingest path (`process.rs`). No dependency bump, no schema change.

## Implementation

Primary — **wire the existing guard into the value path**:

- Thread the emitter **C-StrKey** (already in hand at the caller as
  `ev.contract_id`, currently only hashed to a surrogate) and `net_id` into
  `token_events_net_settled` (`stage.rs`).
- Replace the blanket H2 branch (`stage.rs:~2292`, `!matches!(…Contract)` →
  `policy_null`). For a `Native`/`Credit` event call
  `sac_override_from_event_topics(emitter_strkey, topics, net_id)`: `Some` (emitter
  == derived SAC) → build the `Movement` on the SEP-11 asset_id (`event_asset_surrogate`
  already keys it identically to the op-path presence rows); `None` → keep in
  `policy_null` (dash). Bespoke `Contract` path unchanged.
- Result: genuine SAC classic transfers inside Soroban txs regain their value;
  forgeries still dash. No new keying, no schema change.

Complementary — **hybrid with classic ledger deltas** (0393 deep review):

- For `has_soroban = 1` txs the account-side legs of a SAC classic transfer are
  already present as authoritative, **unspoofable** `AccountEntry`/`TrustLineEntry`
  deltas (`ledger_balance_deltas`), which `stage.rs` currently **computes then
  discards**. Prefer these for account-side SAC value (no trust gate needed); the
  SAC-address guard still covers the contract-side (`C…`) legs and bespoke tokens
  that the delta path structurally cannot see (`ContractData` → `entry_balance`
  returns `None`). Guard is primary; deltas are a complementary/authoritative
  cross-check, not a replacement.

Safety:

- **Alarm on the SAC-derivation mismatch rate.** A wrong network passphrase makes
  every `derived != emitter` → all SAC-in-Soroban values silently dash. Fail-closed
  but invisible — emit a metric/`warn!` so a misconfig or a real spoofing spike is
  visible rather than a silent field of dashes.
- Residual (inherent, out of scope): a **bespoke** token's amount is still
  attacker-chosen — the guard certifies the _classic_ identity, not a contract's
  self-reported amount.
- Backfill/history: value history comes from the full S3 re-ingest (0393); the
  guard runs inside `stage.rs`, so a re-ingest recomputes SAC-in-Soroban values.

## Acceptance Criteria

- [ ] `sac_override_from_event_topics` is wired into `token_events_net_settled`
      (emitter C-StrKey + `net_id` threaded through).
- [ ] A genuine SAC transfer inside a Soroban tx surfaces its net-settled value
      (no longer forced to `NULL`). **DONE** — test `genuine_sac_transfer_surfaces_its_value`.
- [x] A forged `"CODE:ISSUER"` topic from a non-SAC emitter still yields `NULL`
      (spoof stays closed) — proven by a mutation test
      (`forged_classic_claim_from_a_non_sac_emitter_is_null`).
- [x] Value and presence paths resolve the same SAC identity (no drift) — the
      value attaches on `ids::credit_asset_id(code, issuer)` / `NATIVE_ASSET_ID`,
      the exact keys `event_asset_surrogate` / the op path already use.
- [ ] ~~Account-side SAC legs cross-checked against `ledger_balance_deltas`
      (hybrid)~~ — **DEFERRED / not needed:** the guard values both account-side and
      contract-side SAC legs from the (crypto-verified) events, so the delta hybrid
      is redundant for correctness. Kept as an optional authoritative cross-check.
- [x] Mismatch-rate visibility — `sac_rejected` counter + `tracing::debug!` when a
      guard-active reduction rejects classic claims (spoof / net_id misconfig).
      Test `wrong_network_id_rejects_a_genuine_sac_transfer` proves fail-closed.
- [x] `stage.rs` "interim for 0410" comments updated; guard-off tests reworded.
      0393 task H2 section + architecture docs still to reflect "guard wired" (ADR 0032) — folded into the same PR.

## Implementation status

**Implemented in the `feat/0393` branch (this PR), 2026-07-20.** Wiring:
`xdr_parser::sac::net_id()` accessor (env `STELLAR_NETWORK_PASSPHRASE`, `None` →
conservative) → threaded through `tx_token_net_settled` → `token_events_net_settled`,
which calls `sac_override_from_event_topics(emitter, topics, net_id)` per classic
claim. 4 mutation tests (`sac_gate_tests`). Move this file to `archive/` on merge
(via `/lore-framework-tasks`). Only the optional delta-hybrid cross-check remains.
