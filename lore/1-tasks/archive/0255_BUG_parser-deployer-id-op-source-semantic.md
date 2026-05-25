---
id: '0255'
title: 'BUG: parser stores tx-source as deployer_id instead of op-source for Soroban CreateContract; backfill migration'
type: BUG
status: completed
related_adr: ['0027', '0044']
related_tasks: ['0118', '0228', '0252', '0256']
tags:
  [priority-high, effort-medium, layer-indexer, layer-parser, data-correctness]
milestone: 1
links:
  - crates/xdr-parser/src/op_source.rs
  - crates/xdr-parser/src/state.rs
  - crates/indexer/src/handler/process.rs
  - docs/runbooks/0255_phase1_parser_fix_design.md
  - docs/architecture/xdr-parsing/xdr-parsing-overview.md
  - docs/architecture/database-schema/database-schema-overview.md
  - 'PR #213 (Phase 1 parser fix, merged 2026-05-22)'
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
  - date: '2026-05-22'
    status: completed
    who: stkrolikiewicz
    note: >
      **Phase 1 (parser fix) shipped via PR #213 (squash merged into
      develop, `3f39c66b`).**

      New helper `xdr_parser::extract_op_source_per_contract` walks
      every InvokeHostFunction op + its SorobanAuthorizationEntry
      tree, emitting `(contract_id, deployer_strkey)` pairs. Auth
      credentials resolve as: SourceAccount → effective op source;
      Address(Account | MuxedAccount) → that account; Address(Contract
      | ClaimableBalance | LiquidityPool) → skipped (no human
      deployer). `extract_contract_deployments` accepts a new
      `deployer_by_contract: &HashMap<String, String>` override map;
      present entries win, absent fall back to tx source (preserves
      the ~88 % no-override case unchanged). Indexer call site at
      `process.rs` builds the map alongside `sac_identity_by_contract`.

      Counts:
        - 6 files changed, +556/-12 (Phase 1 commit); +16/-12 (review
          nits commit).
        - 6 unit tests (vs 3 in design) in `op_source.rs` covering
          plain top-level deploy, per-op override, factory with
          SourceAccount credentials, factory with Address(Account)
          credentials, factory with contract-signed credentials
          (skipped), fee-bump unwrap (asserts feeSource never reaches
          deployer slot).
        - `cargo test -p xdr-parser` → 237 unit pass + 6 new;
          `cargo test -p indexer` → 13 pass; clippy `-D warnings` +
          rustfmt clean.

      Live-mode ingestion (post task 0241 cutover) now consumes the
      fix. Phase 2 backfill state stays intact — no re-backfill
      required.

      Phase 3 (compare_e11.py re-run + sub-0.1 % deployer mismatch
      verdict) spawned as backlog task 0256 — out of scope here
      because it depends on live mode being deployed.
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

## Status: Completed

Phase 1 (parser fix) shipped via PR #213. Phase 2 (Hetzner CH backfill)
was executed 2026-05-22 prior to the Phase 1 dev session. Phase 3 (E11
re-validate post live-mode rollout) lives as backlog 0256.

## Context

### Stellar account semantics across the three ownership tiers

| Tier     | Field               | Authority                                  |
| -------- | ------------------- | ------------------------------------------ |
| Fee bump | `feeBump.feeSource` | pays fees                                  |
| Inner tx | `tx.sourceAccount`  | sequence consumed; default op source       |
| Per-op   | `op.sourceAccount`  | per-op override; if Some authority is here |

The XDR-side resolution rule is:
`effective_source = op.sourceAccount.or(tx.sourceAccount)`.

The pre-fix parser ignored `op.sourceAccount` for the
`deployer_account` attribution path.

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

Post-migration CH `soroban_contracts.deployer_id` now resolves to
**GCNP4JV…** (op source), aligned with stellar.expert and Horizon
canonical "creator".

### Scope on backfill snapshot at time of detection

| Bucket                                                        | Count            | Status                              |
| ------------------------------------------------------------- | ---------------- | ----------------------------------- |
| `soroban_contracts FINAL` total                               | 321,364          |                                     |
| with `deployer_id IS NOT NULL`                                | 23,730           | universe to consider                |
| internally correctable via `operations_appearances.source_id` | 3,020            | per-op override observed in CH data |
| confirmed mismatch (CH ≠ op source)                           | **2,825 (12 %)** | rows migrated by Phase 2            |
| op inherits tx source (no override)                           | ~20,905 (88 %)   | correct by accident — left alone    |

## Implementation

### Phase 1 — Parser fix (PR #213)

Files:

- **new** `crates/xdr-parser/src/op_source.rs` — `extract_op_source_per_contract`
  helper + 6 unit tests
- **mod** `crates/xdr-parser/src/lib.rs` — re-export
- **mod** `crates/xdr-parser/src/state.rs` — new
  `deployer_by_contract: &HashMap<String, String>` param + lookup with
  tx-source fallback; 7 internal test call sites updated to pass
  `&HashMap::new()`
- **mod** `crates/indexer/src/handler/process.rs` — builds the map per
  envelope (`inner_transaction(env).source_account()` + helper call),
  same shape as the pre-existing `sac_identity_by_contract` walker
- **mod** `docs/architecture/xdr-parsing/xdr-parsing-overview.md` +
  `docs/architecture/database-schema/database-schema-overview.md` —
  record the deployer_id semantic per ADR 0032

### Phase 2 — Backfill migration on Hetzner CH

Executed 2026-05-22 (operator: stkrolikiewicz) via the SQL skeleton in
the second history note. 2,825 rows corrected; EXCHANGE TABLES atomic
swap; no-FINAL invariant preserved (raw = FINAL = 321,364).

The operator runbook at
`docs/runbooks/0255_deployer_id_backfill_migration.md` was NOT written
— Phase 2 was performed inline before Phase 1 formalized. The
operator notes are captured in the second history entry; a retroactive
formal runbook is deferred (low value now that the migration is done
and parser is fixed, would only matter for replicating on a fresh CH
spin-up).

### Phase 3 — Re-validate (spawned as backlog 0256)

Spawned out of scope here — requires live mode to be deployed (task
0241 cutover) before the deployer field can be re-measured against
post-fix ingestion.

## Acceptance Criteria

- [x] Parser fix lands on develop with three new unit tests covering
      single-source, multi-source override, and fee-bump cases.
      _Shipped via PR #213; 6 unit tests in `op_source.rs` (expanded
      beyond the planned 3 to also cover Address(Account) auth
      credentials, Address(Contract) skip, and the SourceAccount auth
      credential path)._
- [x] Backfill migration executed on Hetzner CH; EXCHANGE TABLES
      verified atomic; row count parity confirmed.
      _Completed 2026-05-22 — 2,825 corrected rows, no-FINAL invariant
      preserved (delta = 0)._
- [ ] Operator runbook at
      `docs/runbooks/0255_deployer_id_backfill_migration.md`
      committed. _Deferred — Phase 2 already done; operator notes
      captured in history; formal runbook only relevant for a CH
      respin which is not on the roadmap. Not blocking._
- [ ] Task 0252 E11 re-run shows deployer field mismatch rate
      < 0.1 % (allowing for stellar.expert classification edge cases).
      _Deferred to backlog 0256 — depends on live mode (post-0241
      cutover) so the new parser is exercised on fresh ingest before
      remeasuring._
- [x] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md`
      and `docs/architecture/xdr-parsing/xdr-parsing-overview.md`
      record the deployer_id semantic explicitly.
- [x] **API types regenerated** — N/A: no `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**` change required by
      the parser fix.

## Implementation Notes

### Where the deployer lives in XDR — and how the helper reads it

Two surfaces produce a `(contract_id, deployer_strkey)` pair:

1. **Top-level `CreateContract` / `CreateContractV2`** — deployer is
   the operation's effective source
   (`op.source_account.or(tx_source)`). Plain wallet-deploy shape.

2. **Auth-tree `CreateContractHostFn` / `CreateContractV2HostFn`**
   (factory pattern) — deployer is the signer of the enclosing
   `SorobanAuthorizationEntry`. The signer derives from
   `auth_entry.credentials`:

   - `SorobanCredentials::SourceAccount` → effective op source (op
     override OR tx source)
   - `SorobanCredentials::Address(ScAddress::Account(a))` → `a`'s
     G-strkey
   - `SorobanCredentials::Address(ScAddress::MuxedAccount(med))` →
     bare ed25519 G-strkey of `med` (canonicalised per ADR 0026)
   - `SorobanCredentials::Address(ScAddress::Contract | ClaimableBalance
| LiquidityPool)` → skipped (no human deployer)

### Test surface (6 unit tests in `op_source.rs`)

1. `top_level_create_contract_without_override_uses_tx_source` — the
   88 % case
2. `top_level_create_contract_with_op_override_uses_op_source` — the
   primary bug case
3. `factory_source_account_credentials_uses_effective_op_source` —
   common factory pattern (deployer's wallet authed the deploy)
4. `factory_address_account_credentials_uses_credentials_account` —
   third-account signed factory deploy
5. `factory_address_contract_credentials_skipped` — contract-signed
   auth produces no human deployer; downstream fallback to tx source
6. `fee_bump_unwraps_to_inner_then_op_source_wins` — guards against
   `feeSource` ever reaching the deployer slot (the original concern
   that motivated keeping fee-bump fixtures in the design)

### Counts

| Metric                    | Value      |
| ------------------------- | ---------- |
| Phase 1 PR (#213) commits | 2          |
| Files changed             | 6 + 2      |
| Lines added / removed     | +572 / -24 |
| Tests added (new)         | 6          |
| Tests still green         | 237 + 13   |
| Phase 2 rows corrected    | 2,825      |
| Phase 2 wall time         | ≈ 15 min   |

## Issues Encountered

- **`ScAddress` non-exhaustive match** — first compile of
  `credentials_signer` only handled `Account` + `Contract` variants.
  The compiler caught `MuxedAccount`, `ClaimableBalance`, and
  `LiquidityPool`. Fixed by handling `MuxedAccount` explicitly
  (canonicalise to bare G-strkey) and routing the two non-signer
  variants to `None` alongside `Contract`. Not a regression; the
  design draft simply did not enumerate the full enum.
- **Husky pre-commit on `nx format:write` failed in worktree** —
  `node_modules` was absent in the `modest-lewin-6db7d2` worktree.
  Fixed with `npm ci`. Not a code issue; worth remembering when
  spawning fresh worktrees that touch `.md` files.
- **Copilot review nits** — (a) the Phase 1 design runbook still
  carried a "design draft / half-day estimate" header after the PR
  landed; (b) the `push_preimage_deployer` doc said "silently skipped"
  but the implementation logs a `tracing::warn!`. Both addressed in
  the second commit (`f118355f`) — runbook header rewritten as
  "implemented — landed in PR #213", doc comment reworded to reflect
  the warn log.

## Design Decisions

### From Plan

1. **Design A (caller builds map) over Design B (refactor
   `extract_contract_deployments`)**: surgical and mirrors the
   existing `sac_identity_by_contract` walker shape. Design B would
   have touched `extract_account_states`, `extract_liquidity_pools`,
   `detect_assets`, etc. Locked in by the Phase 1 design draft.

2. **Fallback to tx source preserved**: present map entry wins,
   absent inherits the tx-source arg — keeps the 88 % no-override
   path bit-identical to pre-fix behaviour.

3. **Contract-signed auth credentials skipped**: contract-as-signer
   yields no human deployer; the downstream tx-source fallback
   applies so the row still lands populated.

### Emerged

4. **`MuxedAccount` ScAddress variant handled explicitly**: design
   draft enumerated only `Account` + `Contract`. Compiler revealed
   the full set; chose to canonicalise muxed signers to the bare
   G-strkey via `MuxedAccount::Ed25519(...).to_string()` to match the
   `accounts.account_id` shape per ADR 0026. `ClaimableBalance` and
   `LiquidityPool` (theoretically impossible signers but legal
   `ScAddress` variants) route to the same skip path as `Contract`.

5. **6 unit tests rather than 3 hand-built XDR fixture files**:
   design suggested `crates/xdr-parser/tests/fixtures/0255_*.xdr` and
   real-mainnet captures for the fee-bump case. Implemented as Rust
   struct-builder tests in `op_source.rs` instead — faster to write,
   easier to extend, and the auth-credentials variants (cases 3-5)
   were impractical to generate via hand-built XDR. Coverage strictly
   wider than the original plan (single-source, per-op override,
   factory SourceAccount creds, factory Address(Account) creds,
   factory contract-creds skip, fee-bump unwrap).

6. **Separate `op_source.rs` module rather than extending `sac.rs`**:
   the design draft allowed either ("extend `crate::sac` if logically
   close"). Chose separate module because the SAC walker and deployer
   walker have orthogonal output types (SacAssetIdentity vs deployer
   strkey) and bundling them would muddle the test surface.

7. **Phase 2 operator runbook not written**: AC was unchecked on
   commit. Phase 2 had already happened inline before the dev
   session; the operator history note carries the playbook. A formal
   runbook only matters if the migration needs replaying on a fresh
   CH — not currently planned. Deferred without spawning a follow-up.

## Future Work

- **Backlog 0256** — Phase 3: re-run `compare_e11.py` once live mode
  is deployed (post task 0241 cutover) and confirm the deployer field
  mismatch rate drops below 0.1 %.
- Retroactive operator runbook for Phase 2 — out of scope (see AC
  note). Only needed if CH is respun from scratch.

## Notes

- The 12 % wrong-attribution rate was an upper bound on the visible
  bug; the 88 % "correct by accident" subset never deviated because
  those deploys had no per-op override. Phase 1 closes the
  accumulation surface for live-mode ingestion.
- Why this didn't surface in 0118, 0228, 0207: prior validations were
  count- or hash-set-based; field-level CH ↔ external cross-source
  diff is task 0252's contribution and revealed the issue.
- `soroban_contracts.deployer_id` is read by API endpoint
  `/contracts/:contract_id` (E11) and indirectly by the contract
  explorer UI. The deployer column now reflects op-source semantics
  end-to-end.
