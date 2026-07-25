---
title: 'Handoff — MVP pure-presence asset fan-out (4-commit stack, pre-backfill)'
type: generation
status: mature
spawned_from: '0359'
spawns: []
tags: ['handoff', 'mvp', 'pre-backfill', 'ops']
links: []
history:
  - date: 2026-07-09
    status: mature
    who: karolkow
    note: >
      Pick-up brief after the minimal rewrite + the /devils-advocate pass on
      B1/B2/B3. Supersedes the role-tagged build described in the README
      step-by-step sections.
---

# Handoff — MVP pure-presence asset fan-out

## What shipped (branch `feat/0359_asset-participation-index-remodel`, NOT pushed)

4-commit stack, each clippy-clean + tests green (`xdr-parser` 296):

| commit     | grain  | what                                                                                                                                            |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `acafe7a4` | —      | minimal body-declared fan-out: schema + rows + stage + writer + read-seek + tests                                                               |
| `bf17e87a` | BODY   | payment, path (send/hops/dest), offers, change-trust incl. PoolShare, allow_trust (issuer = op source), revoke-trustline, create/merge = native |
| `7f1ef50d` | META   | claim-CB / clawback-CB / LP deposit+withdraw — assets recovered from same-op `LedgerEntryChanges`; no match → no row                            |
| `f1bbffa5` | RESULT | claim-atom crossings, both legs — **devils-advocate: REDUNDANT, drop it** (see below)                                                           |

**Model:** pure presence, 1:1 on `transaction_participants`.
`operation_asset_appearances (asset_id, ledger_sequence, transaction_id)` — RMT,
`PARTITION BY intDiv(ledger,500000)`, `ORDER BY (asset_id, ledger, tx)`. NO role /
app_order / leg_index / amount / pool_id / type.

**Emitter:** `xdr-parser/src/asset_appearances.rs` —
`emit_asset_appearances(body, op_source, op_result, op_changes) -> Vec<AssetRef>`
(`AssetRef::{Native, Credit{code,issuer}}`). Surrogate hashed later in staging
(`ids::asset_id`; native = `ids::asset_id(0,"",0,0)`).

**Files touched:** `asset_appearances.rs` (new) + `operation.rs` (call site +
`op_meta_changes` + extracted `claim_atoms`) + `types.rs`/`lib.rs` +
`schema/init.sql` (+1 table, guard 27→28) + `persist/{rows,stage,writer}.rs` +
`assets/{queries,handlers}.rs` (read swap) + `tests_cross.rs` + backfill fixture.

## Safety facts

- **Write:** staging only APPENDS `op_asset_rows` (new table). Zero change to any
  existing table's rows.
- **Read:** `fetch_transactions` swapped to seek `operation_asset_appearances`
  (single arm) + `max(sequence)` fence + `LIMIT 1 BY`. ⚠️ Until the backfill runs,
  `/assets/:id/transactions` shows only post-deploy classic history → **run the
  backfill in the SAME rollout as the deploy.**
- **Dedup:** RMT (disk, eventual) + `LIMIT 1 BY ledger,tx` (read, immediate).

## Devils-advocate verdict (2026-07-09) — Ship with changes

Body + meta hold (complete + minimal, tried to break them, couldn't). Findings:

1. **[High] Drop B3 / result grain.** Claim-atom assets are ALWAYS ⊆ the op's
   body-declared assets — offers trade only their `selling`/`buying` pair;
   path-payments execute exactly their declared `send → path[] → dest` route, all
   of which B1 already emits. So B3 emits only duplicate `(asset, tx)` rows that
   dedup away = zero marginal presence, while being THE unbounded row-write swell
   driver. It was load-bearing only in the old role-tagged model (realized
   amounts / trade legs). → drop `f1bbffa5`.
2. **[Med, High before P24] meta V5 silent-miss.** `op_meta_changes` `_ => &[]` →
   a future `TransactionMeta::V5` yields empty op_changes → claim-CB / LP assets
   (their ONLY source) silently absent. Fix: explicit `V0|V1|V2 => &[]` +
   fail-loud on unknown.
3. **[Med] `<invalid>` asset-code collision.** `asset_code_str` = strict-utf8 +
   `<invalid>`; two distinct malformed codes from one issuer collide to one
   `asset_id`. Surrogate changes if fixed later → must ride THIS backfill.
4. **[Med] claim-CB `State`-before-`Removed` invariant.** `claimed_cb_asset`
   returns None for `Removed` (key-only) and relies on the `State` entry carrying
   the asset. Holds under standard Stellar meta but untested — add a `[State, Removed]`
   test.
5. **[Low/watch] failed-tx body rows.** Body/meta emit for failed txs (only result
   is success-gated). Matches `operations_appearances` parity — confirm product
   wants failed txs on asset pages.
6. **[Low/watch] native page needs a native `assets` row.** Emitter keys native
   consistently, but the handler resolves the id from `assets`; no native row →
   `/assets/native/transactions` still can't resolve (the F2 symptom).

## Open / pre-backfill quick-wins

1. `op_meta_changes` explicit `V0|V1|V2` arm (concern 2).
2. ~~Legacy drop~~ — **NOT dead, do NOT drop.** `operations_appearances.asset_code`
   - `asset_issuer_id` are still read by `fetch_operations`
     (`transactions/queries.rs:829`, tx-detail op list) + `audit-harness`. Only
     `idx_oa_asset_issuer_id` (bloom, 0334) is droppable; the columns need
     `fetch_operations` migrated to another op-asset source first. (Corrects audit R3.)
3. Sort HashMap output before return (`state.rs`, `nft.rs`) — determinism for the
   backfill differential, 2 lines.

## NOT in this MVP (stashed / deferred)

`meta.rs` central `TransactionMeta` accessor and `asset_code.rs` unification did
NOT survive the reset (in `git stash`). SAC-invocations union (F-F), LP native leg
(F-B), account roles (F-C), `soroban_events` L2 (F), amount column — all out.

## OPS runbook (when DB size is decided)

1. Manual `CREATE TABLE operation_asset_appearances …` on prod (init.sql is
   fresh-install only, no migration mechanism).
2. Backfill via `backfill-runner Run` over the Soroban era (from ledger
   50,457,424) — same `parse_ledger → stage::prepare` as live, populates the new
   table for free. In the SAME rollout as the read swap.
3. Validate a sample of assets (incl. native) vs Horizon / stellar.expert.
4. `docs/architecture/**` (ADR 0032) + API-types check (no wire-shape change
   expected → likely no regen).
