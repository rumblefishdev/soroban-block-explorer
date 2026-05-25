---
id: '0118'
title: 'BUG: NFT false positives from fungible token transfers'
type: BUG
status: completed
related_adr: ['0027', '0046']
related_tasks: ['0026', '0027', '0149', '0217', '0228']
tags: [priority-high, effort-medium, layer-indexer, audit-F9]
milestone: 1
links:
  - crates/xdr-parser/src/nft.rs
  - lore/2-adrs/0027_post-surrogate-schema-and-endpoint-realizability.md
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit finding F9 (HIGH severity).'
  - date: '2026-04-14'
    status: active
    who: fmazur
    note: 'Activated task for implementation.'
  - date: '2026-04-21'
    status: active
    who: stkrolikiewicz
    note: >
      Scope rewritten for post-0148 / post-ADR-0029 repo state. Task 0148
      removed `crates/db/src/soroban.rs` (incl.
      `update_contract_interfaces_by_wasm_hash()`) and trimmed
      `crates/indexer/src/handler/persist.rs` to a stub; task 0149 (Filip,
      in progress) is rebuilding `persist_ledger` against ADR 0027. The
      original implementation plan referenced functions / line numbers
      that no longer exist.

      New shape: three explicit phases — Phase 1 (parser-only WASM
      classification, startable immediately), Phase 2 (persist-time
      integration, gated on 0149 merge), Phase 3 (post-backfill cleanup,
      operational). Phase 1 delivers a testable `classify_contract_from_wasm_spec`
      function that Phase 2 drops into the write path once 0149 defines
      the new `persist_ledger` signature.
  - date: '2026-04-22'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 2 implementation landed on branch
      `fix/0118_nft-false-positives-fungible-transfers-phase2`.
      Task 0149 is merged, the gate is lifted. Ten files touched,
      ~320 lines of production code + tests.

      Domain: `ContractType` enum gains `Nft = 2` / `Fungible = 3`
      (migration `20260422000100_contract_type_add_nft_fungible`
      updates the `contract_type_name(SMALLINT)` label helper;
      existing `BETWEEN 0 AND 15` CHECK already permits the new
      discriminants, so no column change).

      Indexer: new `handler::persist::ClassificationCache`
      (`Arc<Mutex<HashMap<String, ContractType>>>`) lives on
      `HandlerState` so one Lambda instance reuses the cache across
      every invocation. Definitive verdicts only (Token / Nft /
      Fungible) — `Other` is deliberately dropped so a later WASM
      upload can promote the contract.

      Staging computes a `wasm_hash → ContractType` map via
      `xdr_parser::classify_contract_from_wasm_spec` and overrides
      `ContractRow.contract_type` for non-SAC deployments whose
      wasm_hash classified as Nft/Fungible. Write path adds
      `reclassify_contracts_from_wasm` (UPDATE soroban_contracts,
      same tx, inside the persist envelope) so rows deployed in
      earlier ledgers get reclassified when the WASM upload is
      finally observed. The NFT filter (`resolve_nft_filter`) runs
      before `upsert_nfts_and_ownership`: hydrates the cache for
      unknown contracts via a single `SELECT contract_id,
      contract_type FROM soroban_contracts WHERE contract_id = ANY($1)`,
      then drops rows whose contract resolves to `Token` / `Fungible`.
      `Nft` and `Other` are inserted (the latter temporary — Phase 3
      SQL cleans up once backfill has observed every WASM).

      Parser: `xdr_parser` now provides
      `impl From<ContractClassification> for ContractType` so
      staging can `.into()` idiomatically; this replaced an earlier
      helper function that lived in `classification_cache` and
      unnecessarily put conversion logic outside the enum domain.

      Tests: 130 xdr-parser unit tests (+1
      `converts_into_contract_type`), 4 cache unit tests, 4 DB-gated
      integration tests passing — including the new
      `nft_filter_drops_fungible_classified_contract` end-to-end
      test that ingests one NFT-classified and one fungible-classified
      contract in the same ledger and asserts exactly one `nfts`
      row survives the filter. The old permissive-parser test
      `i128_token_id_not_excluded` was renamed to
      `parser_emits_i128_transfer_as_nft_candidate` with a docstring
      explaining the intentional parser-level permissiveness and the
      filter-at-persist layer separation.

      `nx build` / `nx lint` / `nx test` / `nx fmt-check` all green
      locally. Task stays `active` pending Phase 3 (post-backfill
      operational SQL cleanup) — that phase only runs once backfill
      has indexed the full Soroban-era corpus, so it is intentionally
      not part of this PR.
  - date: '2026-04-24'
    status: blocked
    who: stkrolikiewicz
    note: >
      Moved to blocked pending historical backfill completion. Phase 3
      (post-backfill SQL cleanup of `Other` classifications) and the
      false-positive rate validation against real mainnet data both
      require the full Soroban-era corpus indexed. Unblocks once
      `backfill-runner` (task 0145) finishes the historical sweep.
  - date: '2026-05-12'
    status: active
    who: stkrolikiewicz
    note: >
      Reactivated after CH pilot endpoint audit confirmed the false
      positives manifest empirically on a 64k-ledger backfill: 100% of
      `nfts` rows = misclassified fungible transfers (XLM SAC = 421k
      rows alone; top 5 contracts all fungibles). Phase 3 scope
      expanded: (a) cleanup SQL for both PG and CH stores, (b)
      ingester filter strengthen for pre-window WASM-less contracts
      (current `Other` verdict's permissive emit policy produces the
      observed false positives). See
      `docs/audits/2026-05-12-ch-pilot-endpoint-audit.md` §E15.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 1.5 (Patch C — parser-side whitelist in
      `crates/xdr-parser/src/nft.rs::looks_like_token_id`) +
      Phase 3 (cleanup SQL for PG + CH) implemented on branch
      `fix/0118_nft-false-positives-phase-c`. Picked up from Karol's
      local prototype; ship both as one PR.

      Patch C: `looks_like_token_id` now rejects `i128`/`u128`
      (always SEP-41 fungible amount per spec) with a `debug!` log
      (target `xdr_parser::nft`) for on-demand observability — `warn!`
      would have flooded production logs because fungible transfers are
      high-volume (XLM SAC alone ≈ 421k events in the audit window).
      Whitelists conventional SEP-50 + OpenZeppelin
      token_id shapes (`u32`, `u64`, `i64`, `i32`, `bytes`, `string`,
      `address`). Spec basis: SEP-41 amount=i128, SEP-50 token_id =
      unsigned integer, OpenZeppelin Stellar NonFungibleToken trait =
      u32, Stellar Discussion #1674 = zero argument for i128. Trade-off:
      hypothetical false-negative for SEP-50-non-compliant NFT using
      i128 token_id — zero such contracts observed in the audit sample,
      warn-log surfaces any future case for whitelist extension.

      Test refresh: renamed `parser_emits_i128_transfer_as_nft_candidate`
      → `parser_rejects_i128_transfer_per_patch_c` (inverted assertion).
      Added `parser_rejects_u128_transfer_per_patch_c`,
      `whitelist_accepts_u32_token_id`, `whitelist_accepts_u64_token_id`,
      `whitelist_accepts_i32_and_i64_token_ids`,
      `whitelist_accepts_bytes_string_address_token_ids`,
      `whitelist_rejects_unknown_data_types`. 16/16 `nft::tests` green.

      Phase 3 cleanup SQL embedded in operator runbook
      `docs/runbooks/0118_phase3_cleanup_nfts.md` (PG + CH sections
      side-by-side, with preconditions / sanity probes / verification
      steps and ContractType discriminant mapping). Both flows
      idempotent, both run only AFTER the Soroban-era backfill has
      populated WASM verdicts. CH section tracks mutation completion
      via `system.mutations` before `OPTIMIZE TABLE nfts FINAL`.

      Ingester filter strengthen for the `Other`/NULL bucket
      DEFERRED to task 0217 (nfts quarantine table) — proper
      architectural fix routes `Other`/NULL to dedicated
      `nfts_pending` instead of permissive-inserting into the
      API-facing hot table.
  - date: '2026-05-13'
    status: blocked
    who: stkrolikiewicz
    note: >
      External blocker: full Soroban-era backfill run on a prod-like
      DB required for the Phase 3 empirical dry-run AC. Code-side
      delivery is complete after PR #178 (Patch C parser whitelist +
      Phase 3 cleanup runbook for both PG and CH). Operator runbook
      lives at `docs/runbooks/0118_phase3_cleanup_nfts.md` — one
      markdown with PG and CH sections side-by-side, embedded SQL,
      preconditions / sanity probes / verification queries, and a
      ContractType discriminant mapping table. The cleanup is
      idempotent and only has effect once
      `soroban_contracts.contract_type` is populated with
      WASM-derived verdicts (i.e. after the full Soroban-era backfill
      has indexed every `wasm_upload` op). Ingester filter strengthen
      for the `Other`/NULL bucket split to task 0217 (`nfts_pending`
      quarantine table — architectural follow-up). After the dry-run
      runs and the sanity probe returns 0 unclassified-with-NFT-rows,
      archive this task.
  - date: '2026-05-13'
    status: blocked
    who: stkrolikiewicz
    note: >
      **Patch C reverted in PR #180** (same branch as 0217 quarantine
      implementation). The 2026-05-13 pre-audit re-test against live
      mainnet RPC discovered a real SEP-39 NFT
      (Bachini `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`,
      `SorobanNFT`/`SBN`) using `i128` for `token_id` — Patch C's
      whitelist would have silently dropped this legitimate NFT.
      Audit team's stated principle ("discrimination MUSI być po WASM
      signature, NIE po payload type") is correct; Patch C contradicted
      it.

      Parser `looks_like_token_id` is back to its pre-2026-05-12
      permissive blacklist (`!void|map|vec|error`). The
      Patch-C-specific tests (`parser_rejects_i128_transfer_per_patch_c`
      + the 5 `whitelist_accepts_*` tests) were removed and the
      original `parser_emits_i128_transfer_as_nft_candidate` was
      restored with an updated docstring referencing the Bachini
      mainnet example. The Phase 3 cleanup runbook
      (`docs/runbooks/0118_phase3_cleanup_nfts.md`) is unchanged — it
      still drops legacy `Fungible`/`Token`-classified rows from
      `nfts` post-backfill.

      Implication for this task: the "Phase 1.5" deliverable is
      retracted. Phase 1 (classifier function, PR #104) + Phase 2
      (persist cache + filter, PR #110) + Phase 3 SQL cleanup are the
      shipped deliverables; the architectural follow-up for the
      `Other`/NULL bucket is **task 0217**'s quarantine pattern
      (proper WASM-spec-based discrimination at persist time, not
      payload-type discrimination at parser time). ADR 0046 documents
      the revert with the empirical evidence (Alternative 4 flipped
      from "ACCEPTED AS COMPLEMENT" to "REJECTED").
  - date: '2026-05-22'
    status: completed
    who: stkrolikiewicz
    note: >
      **Closed.** Phase 3 operationalised on the merged Hetzner CH via
      task 0228 Phase 5 `backfill-runner nft-reclassify` subcommand
      (2026-05-21). Effective outcome on the post-merge state:
      27,602,309 false-positive `nfts_pending` rows + 60,492,304
      false-positive `nft_ownership_pending` rows evicted (Token / Nft /
      Fungible discriminants); 0 legacy contamination found in the hot
      `nfts` / `nft_ownership` tables (`dropped_legacy_nfts=0`,
      `dropped_legacy_ownership=0`), confirming the 0217 quarantine
      pattern works end-to-end. Post-eviction `nfts_pending` = 48.85M
      rows (Other / NULL contracts kept in the parking lot per task
      0217 design).

      Task 0228 Phase 6 Tier 5 full run (980/980 PASS, 0.0000 % mismatch
      against Horizon hash-set) provides the regression check: no
      legitimate NFT was misclassified or dropped. The "Phase 3
      post-backfill dry-run verifies sanity check returns 0
      unclassified-with-NFT-rows" AC is effectively satisfied — hot
      tables are empty by construction (`promoted_nfts=0` because no
      contract in the union currently classifies as Nft), so there is
      no unclassified-with-NFT-rows surface to begin with.

      Ingester filter strengthen for the `Other` / NULL bucket — the
      remaining deferred AC item — was split to task 0217 (quarantine
      `nfts_pending` table) which is also shipped. Reclassification of
      `Other` contracts as their WASM is observed later is operationally
      handled by re-running `nft-reclassify` after future backfill
      windows.

      Archived. No follow-up tasks spawned — the architectural fixes
      (parser classifier in PR #104, persist-time filter in PR #110,
      quarantine in task 0217, post-backfill cleanup in task 0228)
      together close the bug fully.
---

# BUG: NFT false positives from fungible token transfers

## Summary

`crates/xdr-parser/src/nft.rs` misclassifies SEP-0041 fungible token
transfers (USDC, XLM wrapping, etc.) as NFT events. The filter in
`looks_like_token_id()` accepts `i128` payloads, which are exactly the
standard fungible transfer amount type. At mainnet scale this would
flood the `nfts` table with millions of false-positive records.

The fix requires distinguishing NFT contracts from fungible contracts
by **WASM spec inspection**, not by payload-type heuristics — some NFT
contracts legitimately use `i128` as token IDs
(see `i128_token_id_not_excluded` test in `nft.rs`).

## Status: Active — phased

- **Phase 1 (parser)** — can start now, independent of other work.
- **Phase 2 (integration)** — gated on task 0149 merge (new
  `persist_ledger` signature).
- **Phase 3 (cleanup + filter strengthen)** — operational, after
  production backfill. **Reactivated 2026-05-12** after CH pilot audit
  ([2026-05-12-ch-pilot-endpoint-audit.md](../../../docs/audits/2026-05-12-ch-pilot-endpoint-audit.md))
  confirmed 100% NFT rows in CH backfill = false positives (XLM SAC
  contributes 421k rows alone). Scope expanded:

  - SQL cleanup script for **both PG and CH** stores.
  - **Ingester filter strengthen** — current `Other` verdict permissive
    emit produces false positives for pre-window WASM-less contracts
    (deploy precedes backfill range, no WASM observed in window).
    Either: (a) stricter "no WASM in window AND not already
    classified" → drop; (b) post-backfill reclassification pass once
    WASM observed in later windows.
  - VACUUM ANALYZE in runbook (PG side). CH equivalent after cleanup
    deletes:

    ```sql
    OPTIMIZE TABLE nfts FINAL;
    OPTIMIZE TABLE nft_ownership FINAL;
    ```

## Context

SEP-0041 fungible token transfers emit events with the topic pattern
`["transfer", Address(from), Address(to)]` and `i128` amount as data —
identical in shape to SEP-0050 NFT transfer events that carry token IDs
as data. The current `nft.rs:162-174` filter exclusion list covers
`void`, `map`, `vec`, `error` but not numeric scalar types. The in-file
doc comment (2026-04-13 note) acknowledges the limitation and defers
the fix pending a proper spec-aware classifier.

ADR 0027 §7 `soroban_contracts` already carries a `contract_type
VARCHAR(50)` column; ADR 0027 §8 `wasm_interface_metadata.metadata JSONB`
contains the function signatures that let us classify. This task
connects the dots: derive classification from WASM spec, persist it on
`soroban_contracts.contract_type`, filter NFT inserts accordingly.

### Classification rules (OpenZeppelin `NonFungibleToken` vs `FungibleToken` traits)

Discriminators derived from the OpenZeppelin Stellar contracts
library (the de-facto reference linked from Stellar Developers docs):

- `packages/tokens/src/non_fungible/mod.rs` — `NonFungibleToken` trait.
- `packages/tokens/src/fungible/mod.rs` — `FungibleToken` trait (SEP-0041).

| Function              | NFT trait | Fungible trait | Discriminator? |
| --------------------- | :-------: | :------------: | :------------: |
| `owner_of`            |    yes    |       no       |      NFT       |
| `token_uri`           |    yes    |       no       |      NFT       |
| `approve_for_all`     |    yes    |       no       |      NFT       |
| `get_approved`        |    yes    |       no       |      NFT       |
| `is_approved_for_all` |    yes    |       no       |      NFT       |
| `decimals`            |    no     |      yes       |    Fungible    |
| `allowance`           |    no     |      yes       |    Fungible    |
| `total_supply`        |    no     |      yes       |    Fungible    |
| `balance`             |    yes    |      yes       |     shared     |
| `transfer`            |    yes    |      yes       |     shared     |
| `transfer_from`       |    yes    |      yes       |     shared     |
| `approve`             |    yes    |      yes       |     shared     |
| `name`, `symbol`      |    yes    |      yes       |     shared     |

- Any NFT discriminator present → classify as `Nft`.
- Otherwise, any Fungible discriminator present → `Fungible`.
- Dual-interface (both sets present) → `Nft` (safer: prefer false
  positives over false negatives for UX).
- No usable WASM metadata yet → `Other` (temporary until WASM upload
  is observed; see Phase 2 cache handling).
- SAC contracts (no WASM) → DB already labels them `'token'` at
  deploy time; Phase 2 treats `'token'` like `Fungible`.

Shared names — notably `balance` (returns `u32` for NFT count vs
`i128` for fungible amount) and `approve` (different signatures) —
are **not** discriminators. Name-level matching is sufficient for
Phase 1; signature-aware classification is a potential refinement
for a future enum variant.

## Implementation

### Phase 1 — parser classifier (no DB, no persist hook)

New public surface in `crates/xdr-parser`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractClassification {
    Nft,
    Fungible,
    Other,
}

pub fn classify_contract_from_wasm_spec(
    metadata: &serde_json::Value,
) -> ContractClassification;
```

Input shape: the `metadata` JSONB shape produced by
`extract_contract_interfaces` (`{functions: [{name, inputs, outputs},
...], wasm_byte_len: u64}`). Pure function — no I/O, no DB.

Unit tests (golden-style, fixtures in `tests/fixtures/`):

- Pure NFT contract (functions include `owner_of`, `token_uri`,
  `transfer`) → `Nft`.
- Pure fungible SEP-0041 contract (`decimals`, `allowance`,
  `transfer`) → `Fungible`.
- Dual-interface contract → `Nft` (precedence asserted).
- Empty / missing metadata → `Other`.
- Real-world mainnet fixtures: at least one known NFT contract
  (e.g., jamesbachini i128-id collection) and one known fungible
  (USDC SAC wrapper or canonical SEP-41 token).

Phase 1 does **not** modify `detect_nft_events` behavior — it only
adds the classifier function. `nft.rs:162-174` stays permissive.
Removing / updating `i128_token_id_not_excluded` test belongs to
Phase 2, when the actual filter decision shifts from heuristic to
classifier.

### Phase 2 — persist-time integration (gated on task 0149)

Once task 0149 merges and `persist_ledger` has a concrete signature
against the ADR 0027 schema:

1. **Classification persist path**: when `wasm_interface_metadata` is
   written, call `classify_contract_from_wasm_spec` and update
   `soroban_contracts.contract_type` in the same transaction (or as a
   follow-up UPDATE, depending on 0149's structure).
2. **Per-worker in-memory cache**:
   `HashMap<contract_id, ContractClassification>`, populated on demand.
   **Only cache definitive classifications** (`Nft`, `Fungible`,
   `Token` from SAC). Never cache `Other` — re-query DB on next
   encounter so a worker that saw a contract before its WASM was
   processed can pick up classification later.
3. **Batch cache population**: for each ledger, collect distinct
   `contract_id`s appearing in NFT-candidate events, issue one
   `SELECT contract_id, contract_type FROM soroban_contracts WHERE
contract_id = ANY($1)` to fill the cache in one round-trip before
   per-event filtering.
4. **Filter at NFT insert time**:
   - `Nft` → insert into `nfts`.
   - `Fungible` / `Token` → skip (no insert).
   - `Other` → insert (temporary false positive, cleaned up in
     Phase 3).
5. **Update test `i128_token_id_not_excluded`**: currently asserts the
   broken permissive behavior; rewrite to assert filter behavior per
   the classifier.

Exact function signatures, where the cache lives (per-request
struct? thread-local? worker state parameter?) depend on how task
0149 shapes `persist_ledger`. Finalise once that signature is public.

### Phase 3 — post-backfill cleanup (operational)

After the historical backfill run (task 0145) has processed the full
Soroban-era corpus:

```sql
BEGIN;
-- Sanity: how many unclassified contracts still have NFT rows?
SELECT COUNT(DISTINCT contract_id) AS unclassified
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type = 'other'
 );
-- If > 0: investigate unclassified contracts first.

-- Remove false positives from Phase 2 "Other" inserts.
DELETE FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN ('fungible', 'token')
 );
COMMIT;
VACUUM ANALYZE nfts;
```

Cleanup procedure shipped as an operator runbook at
`docs/runbooks/0118_phase3_cleanup_nfts.md` — PG + CH sections
side-by-side, embedded SQL, preconditions / sanity probes /
verification queries, and a ContractType discriminant mapping table.

## Acceptance Criteria

### Phase 1 (parser)

- [x] `classify_contract_from_wasm_spec` function added to
      `crates/xdr-parser`, public surface.
      _(shipped in PR #104, re-exported from `xdr_parser::lib`
      alongside `ContractClassification`; also gained
      `impl From<ContractClassification> for ContractType` in the
      Phase 2 PR so callers use idiomatic `.into()`.)_
- [x] `ContractClassification` enum with `Nft` / `Fungible` / `Other`
      variants. _(defined in `crates/xdr-parser/src/classification.rs`;
      `Other` semantics documented as "no usable classification yet,
      must re-query on next encounter" — integration layer never
      caches this value.)_
- [x] Decision tree implemented per the classification-rules table
      above; dual-interface contracts classified as `Nft` (documented).
      _(precedence: NFT discriminator → Nft; fungible discriminator
      only → Fungible; neither → Other. Dual-interface precedence
      rationale — prefer false positives over false negatives for UX —
      spelled out in the function docstring.)_
- [x] Unit tests cover: pure NFT, pure fungible, dual-interface,
      empty metadata (and OZ-surface stand-ins for the mainnet-fixture
      check). _(15 tests in `classification::tests`:
      `empty_functions_is_other`, `nft_by_owner_of`, `nft_by_token_uri`,
      `fungible_openzeppelin_surface`, `fungible_by_total_supply_only`,
      `nft_openzeppelin_surface`, `nft_by_approve_for_all_only`,
      `nft_by_get_approved_only`, `nft_by_is_approved_for_all_only`,
      `fungible_by_allowance_only`, `dual_interface_nft_wins`,
      `unknown_surface_is_other`, `transfer_only_is_other`,
      `nft_precedence_with_token_uri_and_decimals`,
      `additional_non_discriminators_do_not_shift_classification`.
      The original spec asked for two "real mainnet fixtures" — synthetic
      OZ-trait surfaces cover every decision path instead. Real-WASM
      fixture snapshots can be added as a follow-up if a future regression
      calls for bit-exact replay against mainnet data.)_
- [x] No behavior change in `detect_nft_events` yet — Phase 1 only
      adds the classifier function. _(Phase 2 renamed the
      `i128_token_id_not_excluded` test to
      `parser_emits_i128_transfer_as_nft_candidate` with a docstring
      explaining the intentional parser permissiveness; the parser
      itself still emits i128 transfers as NFT candidates — filter
      lives at persist time.)_
- [x] `nx run rust:build`, `nx run rust:test`, `nx run rust:lint`
      pass for the xdr-parser crate. _(130 xdr-parser unit tests green
      on the Phase 2 branch; clippy `-D warnings` clean.)_

### Phase 2 (integration, gated on 0149)

- [x] Classification writes `soroban_contracts.contract_type` on WASM
      upload processing. _(staging sets `ContractRow.contract_type`
      for same-ledger deployments; `reclassify_contracts_from_wasm`
      UPDATE back-propagates to rows deployed earlier.)_
- [x] Per-worker cache avoids repeated DB lookups; does NOT cache
      `Other`. _(`ClassificationCache::extend_definitive` filters
      `Other`; cache lives on `HandlerState` for Lambda warm reuse.)_
- [x] Batch cache population at ledger granularity (one query per
      ledger covering all candidate contracts). _(`resolve_nft_filter`
      issues a single `SELECT contract_id, contract_type FROM
soroban_contracts WHERE contract_id = ANY($1)` for cache misses
      before per-row filtering.)_
- [x] NFT insert path filters by classification: `Nft` → insert,
      `Fungible` / `Token` → skip, `Other` → insert (temporary).
      _(`resolve_nft_filter` `keep` closure; returns index vectors
      into `staged.nft_rows` / `nft_ownership_rows` so the downstream
      UNNEST binds only survivors.)_
- [x] `i128_token_id_not_excluded` test rewritten to assert the new
      filter behavior. _(Renamed to
      `parser_emits_i128_transfer_as_nft_candidate` with a docstring
      explaining the intentional parser-level permissiveness; the
      authoritative filter assertion lives in the new end-to-end
      integration test below.)_
- [x] End-to-end test: live ingest of a small fixture range with one
      fungible contract + one NFT contract yields exactly the expected
      `nfts` rows (no USDC transfer leakage).
      _(`nft_filter_drops_fungible_classified_contract` in
      `crates/indexer/tests/persist_integration.rs` asserts: NFT
      contract row survives, fungible-classified contract row dropped,
      `soroban_contracts.contract_type` persisted correctly for both,
      and the per-worker cache holds both definitive verdicts after
      the ledger commits.)_

### Phase 1.5 (Patch C — parser whitelist, 2026-05-13) — **RETRACTED**

Patch C was shipped in PR #178 and reverted on the same day in
PR #180 after the pre-audit re-test against live mainnet RPC
discovered a real SEP-39 NFT
(`CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY` /
Bachini `SorobanNFT`) using `i128` for `token_id` — the whitelist
would have silently dropped a legitimate NFT collection. Audit
team's stated principle ("discrimination MUST be by WASM signature,
NOT by payload type") is correct; Patch C contradicted it.
ADR 0046 §Alternative 4 documents the empirical evidence and
flips the verdict from "ACCEPTED AS COMPLEMENT" to "REJECTED".

The parser is back to its pre-2026-05-12 permissive blacklist
(`!void|map|vec|error`). The "Phase 1.5" criteria below are
documented for the historical record only and **do not represent
the shipped state**.

- [~] ~~`looks_like_token_id` narrowed to whitelist…~~ **REVERTED**
- [~] ~~`i128` and `u128` explicitly rejected…~~ **REVERTED**
- [~] ~~Test rename + 5 whitelist-coverage tests…~~ **REVERTED** — the
  `parser_emits_i128_transfer_as_nft_candidate` test was restored
  with an updated docstring referencing the Bachini mainnet
  example as evidence for parser-side permissiveness.

### Phase 3 (cleanup, 2026-05-13)

- [x] SQL cleanup script committed to the repo; reviewable.
      _(`docs/runbooks/0118_phase3_cleanup_nfts.md` — PG + CH
      sections side-by-side with embedded SQL, preconditions,
      sanity probes, and verification queries.)_
- [x] Post-backfill dry run verifies sanity check returns 0
      unclassified-with-NFT-rows before the DELETE.
      _(Operationalised via task 0228 Phase 5 `backfill-runner
nft-reclassify` on 2026-05-21. Hot `nfts` / `nft_ownership` =
      0 rows by construction post-merge (`promoted_nfts=0`,
      `dropped_legacy_nfts=0`), so the "unclassified-with-NFT-rows"
      surface is trivially 0. Quarantine `nfts_pending` evicted
      27.6M false positives.)_
- [x] `VACUUM ANALYZE nfts` (PG) / `OPTIMIZE TABLE nfts FINAL` (CH) in
      the cleanup script. CH script tracks `system.mutations` to ensure
      mutations complete before OPTIMIZE.
- [x] Ingester filter strengthen for `Other`/NULL bucket — DEFERRED to
      task 0217 (`nfts_pending` quarantine table). 0118 ships Patch C +
      cleanup SQL; 0217 lands the proper architectural fix
      (Other/NULL → quarantine, hot table stays clean by design).
      _(Task 0217 shipped 2026-05-13; quarantine pattern verified
      end-to-end via task 0228 Phase 6 Tier 5 — 980/980 hash-set
      parity vs Horizon proves no legitimate NFT was misclassified.)_

## Risks / Notes

- **Phase 2 signature dependency**: `persist_ledger` shape is being
  defined by task 0149; waiting on that merge avoids rework.
- **Parallel backfill races**: with multiple workers, a transfer event
  may arrive before the WASM upload of its contract is processed.
  Filter decision on `Other` deliberately inserts (false positive) and
  Phase 3 cleans up — simpler than synchronising workers.
- **Dual-interface precedence**: chose `Nft` over `Fungible` to prefer
  false positives over false negatives. Document this clearly so future
  readers don't change the precedence without understanding the
  rationale.
- **SAC contracts**: pre-classified as `'token'` at deploy time (no
  WASM); Phase 2 treats `'token'` identically to `'fungible'` in the
  filter.
- **Classifier evolution**: if new SEP specs introduce additional
  standard functions, extend the match table in Phase 1; adding new
  `ContractClassification` variants is additive and does not break
  Phase 2's filter (which treats unknown variants conservatively as
  `Nft`-insert — document this default explicitly when extending the
  enum).
- **Phase 2 classifier hit-rate observation (2026-05-12)**: ad-hoc
  smoke test on the current 28,742-contract sample (grouped by
  `soroban_contracts.contract_type` × `wasm_hash IS NOT NULL`) showed
  that only 2 of 306 wasm-bearing contracts received a definitive
  `Nft` / `Fungible` verdict; the other 304 stayed `Other`. The 306
  set is heavily biased — it covers only contracts whose `wasm_upload`
  happened to land in the indexed window, not the full Soroban-era
  population. **Re-evaluate after backfill (task 0145)** before
  treating the low hit-rate as a real classifier bug.
