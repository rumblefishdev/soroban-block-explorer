---
id: '0255'
title: 'BUG: parser stores tx-source as deployer_id instead of op-source for Soroban CreateContract; backfill migration'
type: BUG
status: active
related_adr: ['0027', '0044']
related_tasks: ['0118', '0228', '0252']
tags:
  [priority-high, effort-medium, layer-indexer, layer-parser, data-correctness]
milestone: 1
links:
  - crates/xdr-parser/src/state.rs
  - crates/db-clickhouse/src/persist/stage.rs
  - docs/runbooks/0228_phase6_validation.md
history:
  - date: '2026-05-22'
    status: active
    who: stkrolikiewicz
    note: >
      Spawned from task 0252 Phase B pilot E11 finding. The
      compare_e11.py script (CH ↔ stellar.expert per-field diff on
      `/contracts/:id`) flagged a real deployer mismatch on contract
      CB5GADATQJPVXS5MSWUDYA3HGU56DJZF4H35S3OL5P7W7JZE7IAIEXZ6:
      CH stored `GA2TGTW...` (= tx-level source), stellar.expert
      reports `GCNP4JV...` (= op-level source). Horizon
      `/transactions/{hash}/operations` confirmed: tx source =
      GA2TGTW (matches CH), op source = GCNP4JV (matches SE),
      function = InvokeContract (factory-style sub-deploy).

      Root cause located at `crates/xdr-parser/src/state.rs:91`:

          deployer_account: Some(tx_source_account.to_string()),

      The parser unconditionally uses `tx_source_account` rather than
      reading per-op `source_account` from the XDR operation envelope
      (with fallback to tx source when the op inherits).

      Scale probe across the full backfill (Hetzner CH, 2026-05-22):
        - 23,730 soroban_contracts with deployer_id IS NOT NULL.
        - 91 % of all Soroban ops (type=24) have source_id NULL in
          operations_appearances — parser writes source_id only on
          explicit per-op override.
        - 3,020 contracts have an explicit per-op override available
          via CH internal data; 2,825 of those (93.5 %) have CH's
          stored deployer ≠ op source — bug manifests.
        - The remaining 20,710 contracts have op-inherit-tx-source
          semantics → stored value is correct by accident.

      Net: ~2,825 contracts (12 % of the deployer universe) have
      wrong attribution today and ARE migratable from internal CH
      data alone — no Horizon / S3 XDR re-parse needed.

      Spawned to fix the parser AND back-fill the 2,825 misattributed
      rows in one shot. Task 0252 keeps the divergence as tolerance
      until 0255 lands; the final Phase B report will note the
      finding and the resolution.
  - date: '2026-05-22'
    status: active
    who: stkrolikiewicz
    note: >
      **Phase 2 (backfill migration) COMPLETE on Hetzner CH.**

      Operator session: temporary bump of
      `users.d/timeouts.xml` `max_memory_usage` to 80 GiB →
      docker restart → build `soroban_contracts_staging_0255` via
      the migration query in the task body (≈ 15 min wall, partial-
      merge JOIN) → row count parity verified (live = staging =
      321,364) → diff count = 2,825 rows (matches the scale probe) →
      spot-check `CB5GADATQJPVXS5MSWUDYA3HGU56DJZF4H35S3OL5P7W7JZE7IAIEXZ6`
      confirmed corrected from `GA2TGTW…` (tx source) to
      `GCNP4JVZFDAQFBPZ76VD6YARZNURD6DIC43HMZAFGBIZ2OLEHYKEPAO2`
      (op source per stellar.expert canonical) → atomic
      EXCHANGE TABLES → no-FINAL invariant verified (raw =
      FINAL = 321,364, delta = 0) → staging dropped →
      profile cap reverted to 6 GB + container restart.

      ~2,825 contracts now carry the correct op-source attribution
      in the current backfill state.

      Phase 1 (parser fix) deferred to a dedicated dev session —
      requires walking SorobanAuthorizationEntry credentials per-tx
      to extract op-level source, building a
      `deployer_by_contract: HashMap<contract_id, strkey>` override
      map analogous to the existing `sac_identity_by_contract`
      pattern, threading it into `extract_contract_deployments`,
      plus three XDR test fixtures (single-source / multi-source /
      fee-bump). Estimated ~200-400 LoC + tests = half-day dev cycle;
      not appropriate to interleave with the active task 0252 Phase B
      validation runs.

      Until Phase 1 lands, every fresh deploy ingested via live mode
      with an explicit per-op `source_account` override will continue
      to land with `deployer_id = tx_source` (wrong). The migration
      we just ran corrects the EXISTING backfill snapshot only —
      it does not preempt future writes. Live mode is gated behind
      task 0241 cutover, so the window before Phase 1 must close is
      bounded by 0241's go-live.
---

# BUG: parser stores tx-source as deployer_id instead of op-source

## Summary

The XDR parser at
[`crates/xdr-parser/src/state.rs:91`](../../../crates/xdr-parser/src/state.rs)
writes `deployer_account = tx_source_account` unconditionally for every
extracted Soroban contract deployment. The correct semantic — per
Stellar protocol + canonical references (Horizon, stellar.expert) —
is to use the per-op `source_account` override from the XDR operation
envelope, with a fallback to tx source only when the op inherits (the
common case).

The bug went unnoticed through tasks 0118, 0228, and 0207 because none
of those did field-level external cross-source validation; task 0252
Phase B E11 (CH ↔ stellar.expert per-contract field diff) is what
surfaced it.

## Status: Active

Phase 1 (parser fix) + Phase 2 (one-shot backfill migration on
Hetzner CH) sized as one task because the migration is internal-CH
trivia (≈ 30 min EXCHANGE TABLES pass) and Phase 1 is the
preventive correction for live mode going forward.

## Context

### Stellar account semantics across the three ownership tiers

| Tier     | Field               | Authority                                  |
| -------- | ------------------- | ------------------------------------------ |
| Fee bump | `feeBump.feeSource` | pays fees                                  |
| Inner tx | `tx.sourceAccount`  | sequence consumed; default op source       |
| Per-op   | `op.sourceAccount`  | per-op override; if Some authority is here |

The XDR-side resolution rule is:
`effective_source = op.sourceAccount.or(tx.sourceAccount)`.

Our parser ignores `op.sourceAccount` for the `deployer_account`
attribution path.

### Empirical reference point

Contract `CB5GADATQJPVXS5MSWUDYA3HGU56DJZF4H35S3OL5P7W7JZE7IAIEXZ6`,
deployed in ledger 62,461,877 via factory contract
`CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA` in tx
`029fe1ca5d9c6b8d5354ece52cb29c5471c431e42a573c56a1d508a06bd87a16`:

| Tier                                | Account                                                  |
| ----------------------------------- | -------------------------------------------------------- |
| `feeBump.feeSource`                 | GA74RB6LOJL6NUHEDYPTDPL3BAVP5Q6GAJT32ALGGDJD52LMKWIX7MSJ |
| `tx.sourceAccount` (inner tx)       | GA2TGTWGX2MSY3GHZBSQFMOND3S2BP3XXV3IMLWHEEHYF6TS3LZSC6LJ |
| `op.sourceAccount` (InvokeContract) | GCNP4JVZFDAQFBPZ76VD6YARZNURD6DIC43HMZAFGBIZ2OLEHYKEPAO2 |

CH `soroban_contracts.deployer_id` resolves to **GA2TGTW…** (tx
source). stellar.expert and Horizon both treat the op-level source
**GCNP4JV…** as canonical "creator".

### Scope on current backfill

| Bucket                                                        | Count            | Status                              |
| ------------------------------------------------------------- | ---------------- | ----------------------------------- |
| `soroban_contracts FINAL` total                               | 321,364          |                                     |
| with `deployer_id IS NOT NULL`                                | 23,730           | universe to consider                |
| internally correctable via `operations_appearances.source_id` | 3,020            | per-op override observed in CH data |
| confirmed mismatch (CH ≠ op source)                           | **2,825 (12 %)** | rows to migrate                     |
| op inherits tx source (no override)                           | ~20,905 (88 %)   | correct by accident — leave alone   |

## Implementation Plan

### Phase 1 — Parser fix

`crates/xdr-parser/src/state.rs:91` (and any sibling helper that
constructs `ExtractedContractDeployment`):

```rust
// Before
deployer_account: Some(tx_source_account.to_string()),

// After (pseudocode — adapt to whatever access the existing
// extraction loop has to the current Soroban op):
let op_source = op
    .source_account
    .as_ref()
    .map(|sa| account_id_strkey(sa));
deployer_account: op_source.or_else(|| Some(tx_source_account.to_string())),
```

Equivalent for fee-bump unwrap: the inner-tx loop should pass through
the inner-tx source as `tx_source_account`; the outer-tx fee-source
must never reach the deployer slot.

#### Test fixtures

Add a `crates/xdr-parser/tests/fixtures/` XDR with:

1. Plain single-source CreateContract — assert deployer = tx source.
2. Multi-source: op.source_account explicit override — assert
   deployer = op source.
3. Fee-bump: outer fee_source ≠ inner tx source ≠ op source — assert
   deployer = op source (never fee account).

### Phase 2 — Backfill migration on Hetzner CH

Internal-CH EXCHANGE TABLES pass mirroring `repair_tier1` style.
Selection of corrected `deployer_id` uses
`operations_appearances.source_id` for the deploy tx, fallback to
existing value when the op had no explicit override (parser stored
NULL there).

Migration SQL skeleton (verified shape at scale-probe time):

```sql
DROP TABLE IF EXISTS soroban_contracts_staging_0255;
CREATE TABLE soroban_contracts_staging_0255 AS soroban_contracts;

INSERT INTO soroban_contracts_staging_0255
WITH affected AS (
  SELECT sc.id AS contract_surrogate,
         argMin(oa.source_id, oa.application_order) AS correct_deployer_id
    FROM soroban_contracts AS sc FINAL
    INNER JOIN transactions AS t FINAL
      ON t.ledger_sequence = sc.deployed_at_ledger
     AND t.has_soroban = true
     AND t.source_id = sc.deployer_id
    INNER JOIN operations_appearances AS oa
      ON oa.transaction_id = t.id
   WHERE sc.deployer_id IS NOT NULL
     AND oa.type = 24
     AND oa.source_id IS NOT NULL
   GROUP BY sc.id
)
SELECT sc.id, sc.contract_id, sc.wasm_hash, sc.wasm_uploaded_at_ledger,
       ifNull(a.correct_deployer_id, sc.deployer_id) AS deployer_id,
       sc.deployed_at_ledger, sc.contract_type, sc.is_sac, sc.name
  FROM soroban_contracts AS sc FINAL
  LEFT JOIN affected AS a ON a.contract_surrogate = sc.id
 SETTINGS max_memory_usage = 80000000000,
          max_bytes_before_external_group_by = 16000000000,
          join_algorithm = 'partial_merge';

-- Verify row count parity, then:
EXCHANGE TABLES soroban_contracts AND soroban_contracts_staging_0255;
DROP TABLE soroban_contracts_staging_0255;
```

Operator runbook: write to
`docs/runbooks/0255_deployer_id_backfill_migration.md` with
preconditions (free disk, server profile cap raised temporarily,
Snapshot B captured), sanity probes (count before/after, sample of
corrected rows), and rollback (Snapshot B restore).

### Phase 3 — Re-validate

Re-run task 0252 `compare_e11.py` (E11 — /contracts/:contract_id) on
the migrated state. Expected: deployer field mismatch rate drops from
~93 % (within sampled cohort) to near 0.

## Acceptance Criteria

- [ ] Parser fix lands on develop with three new unit tests covering
      single-source, multi-source override, and fee-bump cases.
- [x] Backfill migration executed on Hetzner CH;
      EXCHANGE TABLES verified atomic; row count parity confirmed.
      _Completed 2026-05-22 — 2,825 corrected rows, no-FINAL invariant
      preserved (delta = 0)._
- [ ] Operator runbook at
      `docs/runbooks/0255_deployer_id_backfill_migration.md`
      committed.
- [ ] Task 0252 E11 re-run shows deployer field mismatch rate
      < 0.1 % (allowing for stellar.expert classification edge cases).
- [ ] **Docs updated** — `docs/architecture/database-schema/canonical-tables.md`
      (or the appropriate canonical doc for `soroban_contracts`)
      records the deployer_id semantic explicitly: "op.source_account
      from the CreateContract / InvokeHostFunction op, fallback to
      tx.source when not overridden".
- [ ] **API types regenerated** — N/A: no `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**` change required by
      the parser fix.

## Notes

- The 12 % wrong-attribution rate is an upper bound on the visible
  bug; the 88 % "correct by accident" subset never deviated because
  those deploys had no per-op override. Live-mode parser fix (Phase 1)
  is what stops the bug accumulating going forward — without Phase 1,
  every fresh deploy via override re-introduces a mis-attributed row.
- Why this didn't surface in 0118, 0228, 0207: prior validations were
  count- or hash-set-based; field-level CH ↔ external cross-source
  diff is task 0252's contribution and revealed the issue.
- `soroban_contracts.deployer_id` is read by API endpoint
  `/contracts/:contract_id` (E11) and indirectly by the contract
  explorer UI. Migration unblocks the deployer column going correct
  in production.
