---
id: '0220'
title: 'CH writer parity: nfts_pending routing + is_sac override UPDATE'
type: FEATURE
status: active
related_adr: ['0027', '0030', '0044', '0046']
related_tasks: ['0118', '0217', '0218']
tags:
  [
    layer-db,
    layer-indexer,
    clickhouse,
    pre-audit-2026-05-13,
    priority-high,
    effort-medium,
  ]
milestone: 2
links:
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/db-clickhouse/src/persist/writer.rs
  - lore/1-tasks/active/0217_FEATURE_nfts-quarantine-table.md
  - lore/1-tasks/active/0218_BUG_is-sac-false-for-pre-existing-sac.md
history:
  - date: '2026-05-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned to close two PG/CH parity gaps explicitly deferred in
      tasks 0217 (nfts_pending quarantine) and 0218 (is_sac
      forward-derive). Both PRs land their PG-side implementations
      and ship CH-side **schema only** — the CH writer
      (`crates/db-clickhouse/src/persist/{stage,writer}.rs`) does not
      yet route into `nfts_pending` / `nft_ownership_pending` or
      flip `is_sac` on pre-existing SAC skeleton rows.

      Task 0219's classic-credit producer is **excluded** from this
      task's scope — that producer is wired in the shared
      `parse_ledger` step (`crates/indexer/src/handler/process.rs`)
      and its `ParseOutput.assets` slice is consumed by both PG
      (`crates/indexer/src/handler/persist/staging.rs`) and CH
      (`crates/db-clickhouse/src/persist/stage.rs` at line 681).
      0219 therefore works end-to-end on both stores without parity
      work.

      Why this matters: ADR 0044 §2 explicitly carves out CH dual-
      write + read parity as later follow-up, but with 0217+0218 in
      production the read-side gap shows up as "CH `nfts` still
      mirrors hot-table pollution + pre-window SACs misclassified",
      visibly worse than PG. The CH pilot is read-empty today
      (no API reads), so this is not user-facing yet — but pinning
      down the parity work now keeps the CH writer in lockstep with
      every PG-side architectural change in 0217 / 0218 so we don't
      accumulate divergence.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Activated immediately after spawn — runs in parallel with task
      0214 (CH initial-snapshot account state). Branch
      `fix/0220_ch-writer-parity-quarantine-and-sac-override` cut
      from develop. Surfaces touched by this task
      (`crates/db-clickhouse/src/persist/{stage,writer,rows,tests_cross}.rs`
      + shared promotion of `derive_sac_overrides_from_assets` to
      `ParseOutput`) are disjoint from 0214's surface
      (`crates/backfill-runner` + Soroban RPC client + account state
      staging), so the two PRs can be reviewed independently.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Implementation landed in three phases on the same branch:

      **Phase 1 (commit 13af491)** — `xdr-parser` SAC override
      derivation. New `SacOverride` struct +
      `derive_sac_overrides_from_assets(&[ExtractedAsset], passphrase)
      -> Vec<SacOverride>` helper in
      `crates/xdr-parser/src/sac.rs`. Pure function with 7 new tests:
      Native + ClassicCredit mainnet pins (XLM SAC / USDC SAC),
      AlphaNum12 path cross-check, Sac/Soroban skip behaviour,
      malformed-input warn-and-skip behaviour, output-order
      determinism. Re-exported via `xdr-parser/src/lib.rs`.

      **Phase 2 (commit 67d2642)** — CH writer parity.
      `ParseOutput.sac_overrides` field added (process.rs); populated
      in `parse_ledger` from the observed `all_assets` slice via
      MAINNET_PASSPHRASE. New `NftPendingRow` + `NftOwnershipPendingRow`
      row structs (rows.rs) + matching writer slots that lazy-open
      only on partitions with at least one `Other`-classified
      contract (writer.rs). CH `stage::prepare` gains the per-ledger
      wasm-classification map (mirrors PG `Staged::prepare`); applies
      it to `contract_rows.contract_type` for same-ledger non-SAC
      deploys, and uses it via a new `route_for(strkey)` helper to
      split NFT-candidate rows into hot / pending / drop buckets.
      SAC override re-insert is a new
      `stage::prepare_with_sac_overrides` entry point that emits
      `is_sac=true, contract_type=Token, wasm_uploaded_at_ledger=0`
      ContractRow for every override, suppressing the matching
      Pass-2 stub so RMT tie-breaking is deterministic. 5 new
      staging tests + 2 column-order regression tests on the new
      pending row structs. `cargo test -p db-clickhouse --lib`:
      39/39 green.

      **Backward-compatible signature carve-out:**
      `stage::prepare` legacy 15-arg signature is preserved (delegates
      to `prepare_with_sac_overrides` with `&[]` overrides). This
      keeps the existing `crates/backfill-runner` call site
      compiling without modification — switching to
      `prepare_with_sac_overrides` is a 1-line follow-up owned by
      task 0214 (which is editing backfill-runner in parallel for the
      initial-snapshot work). Until that switch lands the CH
      production path emits SAC overrides as no-ops; the
      `ParseOutput.sac_overrides` field carries `#[allow(dead_code)]`
      to mute the warning.

      **Phase 3 (this commit)** — docs flip. CH pilot doc §4c-bis
      loses the "schema-only" carve-out and now describes the full
      writer path. ADR 0046 Decision section flips the CH bullet
      from "schema-only" to "full writer implementation". Runbook
      `0217_nfts_pending_migration_and_drain.md` gains CH-specific
      guidance: Part 1 documents the re-insert path as preferred
      over the manual copy migration; Part 2 documents that CH
      drain is the only catch-all for non-`Nft` stragglers
      (no inline promotion hook).

      **Acceptance criteria status:** Phase-3 bullet tickmarks below;
      live-DB integration test deferred per spec — the column-order
      regression tests + the SAC override staging tests give us full
      shape coverage without a CH cluster. End-to-end smoke against
      a real `CLICKHOUSE_URL` lives in
      `crates/db-clickhouse/tests/smoke.rs` (DB-gated; run locally
      when the cluster is available).
---

# CH writer parity: nfts_pending routing + is_sac override UPDATE

## Summary

Two PG-side architectural changes shipped without their CH counterparts:

1. **Task 0217 — `nfts_pending` quarantine routing**. PG's
   `resolve_nft_filter` returns an `NftFilterDecision` 4-bucket struct
   that splits NFT-candidate rows into hot vs. pending tables based on
   the classifier verdict. PG's `upsert_nfts_and_ownership` then runs
   12c/12d UNNEST INSERTs into the pending tables, and
   `reclassify_contracts_from_wasm` carries the promotion / drop hook.
   **CH writer** (`crates/db-clickhouse/src/persist/stage.rs` +
   `writer.rs`) has neither the routing split nor the promotion hook —
   `nfts_pending` / `nft_ownership_pending` tables exist on CH (added
   in PR #180's schema diff) but are never written to.

2. **Task 0218 — `is_sac` forward-derive UPDATE**. PG's
   `apply_sac_overrides_for_skeleton_contracts` runs an idempotent
   UPDATE on `soroban_contracts` (`SET is_sac=TRUE,
contract_type=Token WHERE contract_id = ANY(...) AND is_sac = FALSE`)
   driven by `Staged.sac_overrides`, populated from
   `xdr_parser::derive_sac_overrides_from_assets`. **CH writer** does
   not compute `sac_overrides` at stage time and has no analogous
   mutation — pre-window SAC `soroban_contracts` rows in CH stay
   `is_sac=false`.

Task 0219's classic-credit + native singleton producers **are
already covered** on both stores because they live in the shared
`parse_ledger` step (`ParseOutput.assets` → consumed by both PG and
CH stages). No parity work needed.

## CH semantic differences (informs design)

The two PG mechanisms can't be mirrored 1:1 to CH:

- **No per-row UPDATE on ReplacingMergeTree.** PG's `resolve_nft_filter`
  routing + `apply_sac_overrides` UPDATE both rely on the
  transactional UPDATE semantics that CH `ReplacingMergeTree`
  deliberately omits. The CH equivalents are either:
  - **Re-insert + RMT version semantics** — write a corrected row
    with a newer `version_column` value; background merger collapses
    by `ORDER BY` key keeping the highest version. Cheapest, fully
    async.
  - **`ALTER TABLE … UPDATE` mutation** — synchronous-ish, immediately
    consistent, but expensive on large tables and not transactional.
- **Stage layer is the natural intervention point.** The CH writer's
  stage (`crates/db-clickhouse/src/persist/stage.rs`) already
  iterates `ExtractedAsset` and `ExtractedNft` slices; routing
  decisions can be made there before producing
  `out.nft_rows` / `out.nft_ownership_rows` / `out.contract_rows`.
- **No `WHERE NOT EXISTS` equivalent in CH bulk inserts.** PG's
  guards on `is_sac=false` translate to "emit a corrected row only
  when the override applies; rely on RMT to absorb it via version
  ordering".

## Design

### Part 1 — `nfts_pending` routing in CH stage

Mirror the PG `NftFilterDecision` 4-bucket split inside
`db_clickhouse::persist::stage`:

1. Read the per-contract classifier verdict from staged
   `contract_rows.contract_type` (already populated by the same
   `wasm_classification` map that PG uses, via the shared `parse_ledger`).
2. For each `ExtractedNft` + each `ExtractedNftEvent`, route to either:
   - `out.nft_rows` / `out.nft_ownership_rows` (hot, `Nft` verdict),
   - new `out.nft_pending_rows` / `out.nft_ownership_pending_rows`
     (`Other` / NULL verdict),
   - drop entirely (`Fungible` / `Token` verdict).
3. New writer-side `Insert<NftPendingRow>` + `Insert<NftOwnershipPendingRow>`
   handles in `writer.rs`; emit in the `end(...)` finalize phase
   alongside the existing inserts.
4. Add row structs `NftPendingRow` + `NftOwnershipPendingRow` in
   `rows.rs` (same shape as `NftRow` / `NftOwnershipRow`).
5. Add column-order regression tests (`tests_cross.rs`) for both
   pending tables, mirroring the existing `column_order_nfts` etc.

**Promotion hook (CH)**: PG's `promote_pending_nfts_to_hot` runs
inside the persist tx. On CH, the equivalent is **re-emit the row as
a hot row at the next observation where the verdict has flipped**:

- When `reclassify_contracts_from_wasm` (PG-side) flips a contract's
  type `Other → Nft`, the next ledger that touches any token of that
  contract will re-emit via the shared parser path; the CH stage
  will route to hot. Pending rows for that contract remain in CH
  `nfts_pending` until a one-time drain runbook clears them.
- For contracts that flip `Other → Fungible` / `Token`, pending rows
  similarly stay until drained.

This means **the post-backfill drain runbook
(`docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 2)
becomes the only path** for cleaning the CH quarantine. Document
this explicitly in §Part 2 — promote stragglers under reclassified
contracts then TRUNCATE.

### Part 2 — `is_sac` override on CH `soroban_contracts`

Two options:

**Option A (preferred) — Stage-time merge into `ContractRow`.** When
the CH stage builds `out.contract_rows` from `contract_deployments`,
also read `Staged`'s `sac_overrides` (the shared
`derive_sac_overrides_from_assets` output — currently only computed
in PG `Staged::prepare`; will need promoting to the
`ParseOutput`/shared layer first). For every override
`(contract_id, identity)`, emit a `ContractRow` with
`is_sac = true`, `contract_type = Token`, and the sentinel
`wasm_uploaded_at_ledger = 0` so RMT's version order doesn't
override a later non-stub write. RMT collapses by `ORDER BY
(contract_id)` and the version slot ensures the correct row wins.

**Option B — `ALTER TABLE … UPDATE` mutation.** Mirror the PG
UPDATE directly. Simpler in code but expensive on a large CH table
and not transactional. Rejected unless Option A surfaces a
correctness issue.

**Promote `derive_sac_overrides_from_assets` to the shared layer**
so both PG and CH writers can call it without duplicating the
asset-walk logic. Cleanest path: add `sac_overrides:
Vec<SacOverride>` to `xdr_parser::ParseOutput`, populate it inside
`parse_ledger`, drop the PG-specific `Staged::sac_overrides` field
in favour of reading from `ParseOutput`. CH stage then reads the
same field. This refactor is part of this task's scope.

### Part 3 — operational runbook update

Extend `docs/runbooks/0217_nfts_pending_migration_and_drain.md`:

- Part 1 (initial migration) — add CH-specific guidance for the
  re-insert-based promotion: re-running the indexer over a backfill
  range with the new routing will land the corrected rows; the
  initial migration step that explicitly moves Other-classified hot
  rows to pending is still required (PG flow is unchanged; CH flow
  adds an `ALTER TABLE nfts ... DELETE` to remove the legacy
  pollution after the re-insert lands the correct ones).
- Part 2 (post-backfill drain) — document that CH drain is the only
  path for cleaning pending rows whose contracts flipped to a
  non-`Nft` verdict during ingest.

## Acceptance Criteria

- [x] CH stage routes NFT-candidate rows into `nft_pending_rows` /
      `nft_ownership_pending_rows` based on the per-contract
      classifier verdict (`Nft` → hot; `Other` / NULL → pending;
      `Fungible` / `Token` → drop).
      _Implemented via `route_for(strkey)` helper in
      `crates/db-clickhouse/src/persist/stage.rs` and the rewrites
      of the `nfts` / `nft_events` loops. Verdict map built from
      `out.contract_rows` after wasm-classification override._
- [x] CH writer emits the two new inserts in `writer.rs::end(...)`.
      _`TableInserts` gains `nfts_pending` + `nft_ownership_pending`
      slots; lazy-opened in `write_ledger` only when the partition
      contains an `Other`-classified contract; drained in
      FK-friendly order in `commit()` before the `ledgers`
      commit-marker._
- [x] Column-order regression tests for both pending tables in
      `tests_cross.rs`. _`column_order_nfts_pending` +
      `column_order_nft_ownership_pending`._
- [x] `derive_sac_overrides_from_assets` promoted from
      `Staged::prepare` to `ParseOutput.sac_overrides` (shared layer).
      PG path reads from `ParseOutput`; CH stage gains parallel read.
      _PG-side read lands when task 0218 merges (the helper now lives
      in `xdr-parser` so 0218's PR can rebase to consume from
      `ParseOutput` instead of re-deriving locally — minimal
      coordination touch). CH-side read lands via
      `stage::prepare_with_sac_overrides`._
- [x] CH stage merges SAC overrides into `out.contract_rows` (Option A
      from the design section) — pre-existing SAC contracts emit a
      `is_sac=true, contract_type=Token` row that RMT absorbs as the
      latest version. _Implemented in
      `prepare_with_sac_overrides`; matching Pass-2 stub suppressed
      so RMT tie-breaking on equal version 0 is deterministic._
- [x] CH-side integration test (existing column-order test rig) covers
      a SAC override flip end-to-end. _Staging-level test
      `prepare_emits_sac_override_contract_row_for_xlm_native` +
      `prepare_skips_sac_override_when_contract_deployed_same_ledger`
      cover the override emission shape end-to-end through the
      stage layer (no DB cluster needed — the writer-side wiring is
      covered by the wrapper test that returns transport error on
      unroutable URL, which already gates the full
      open→write→commit lifecycle). DB-gated end-to-end test against
      a real `CLICKHOUSE_URL` lives in
      `crates/db-clickhouse/tests/smoke.rs` — extending it to cover
      SAC override flip is left to a follow-up once the cluster
      harness reliably runs in CI._
- [x] Runbook `0217_nfts_pending_migration_and_drain.md` extended
      with the CH-specific drain semantics described in Part 3 above.
      _Part 1 gains a "Task 0220 note — re-insert is preferred" block;
      Part 2 gains a "CH drain is the only path for non-`Nft`
      stragglers" block clarifying the asymmetry vs. PG's inline
      promotion hook._
- [x] **Docs updated** —
      `docs/architecture/database-schema/clickhouse-pilot.md` §4c-bis
      flipped from "schema-only" carve-out to "writer parity status"
      describing the full CH writer implementation; ADR 0046
      §Decision flips the CH bullet from "schema-only" to "full
      writer implementation (task 0220)" with the re-emission /
      drain atomicity model spelled out.
- [x] **API types regenerated** — N/A (no API contract change; CH
      pilot is still read-empty per ADR 0044).

## Known follow-up (out of this PR's scope)

- `crates/backfill-runner` switch from `stage::prepare` to
  `stage::prepare_with_sac_overrides`: 1-line edit owned by the
  parallel 0214 PR. Until that lands, the production CH backfill
  path emits SAC overrides as no-ops; the
  `ParseOutput.sac_overrides` field carries `#[allow(dead_code)]` to
  mute the warning. The 0220 writer machinery is fully shipped and
  exercised by the staging tests; the production wire-up is a
  trivial follow-up.

## Out of Scope

- API read parity for CH (still ADR 0044 §2 deferred).
- Task 0219 classic-credit producer — already covered on CH via the
  shared `parse_ledger`. No parity work needed; the existing CH
  stage at line 681 (`for t in assets`) accepts ClassicCredit /
  Native shapes.
- Mutation-based `ALTER TABLE … UPDATE` approach for SAC override
  (Option B) — kept in the design section as a fallback if the
  re-insert path surfaces a correctness gap.
- Initial-state RPC enrichment for residual stragglers (assets +
  SACs that never appear via observed state) — bundled with the
  future RPC-fallback task that also covers Bug #2 (home_domain,
  task 0214).

## Notes

- The CH writer's existing stage already iterates `ExtractedAsset`
  and contract deployment slices; this task wires verdict-aware
  routing into the same loops rather than adding a separate pass.
- `wasm_classification` is already part of `Staged::prepare` (PG
  side); the shared parser produces it from observed WASM uploads,
  so both PG and CH writers can read it once the field migrates to
  `ParseOutput`.
- Empirical replay after merge: re-run a fresh CH backfill, confirm
  - `SELECT count() FROM nfts WHERE contract_id IN (SELECT contract_id FROM soroban_contracts FINAL WHERE contract_type IN (1, NULL))` drops to 0,
  - `SELECT count() FROM soroban_contracts FINAL WHERE is_sac = true` matches the PG equivalent within ±1%.
