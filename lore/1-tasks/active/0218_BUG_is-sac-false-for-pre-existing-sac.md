---
id: '0218'
title: 'BUG: is_sac=false for pre-existing SAC contracts (forward-derive from observed assets)'
type: BUG
status: active
related_adr: ['0027', '0030']
related_tasks: ['0118']
tags:
  [
    layer-indexer,
    postgres,
    clickhouse,
    pre-audit-2026-05-13,
    priority-high,
    effort-medium,
  ]
milestone: 2
links:
  - crates/xdr-parser/src/sac.rs
  - crates/xdr-parser/src/state.rs
history:
  - date: '2026-05-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from Karol's 2026-05-13 pre-audit Bug #4: pre-existing
      Stellar Asset Contracts (SAC = built-in host contract for
      wrapping classic Stellar assets like XLM, USDC) whose
      `create_contract` op happened BEFORE the indexed window persist
      as skeleton `soroban_contracts` rows with `is_sac=false`. The
      existing detection path (`extract_contract_deployments` in
      `crates/xdr-parser/src/state.rs`) only fires when an in-window
      `LedgerEntryChange` carries `executable=stellar_asset`; pre-window
      SACs never produce such a change in the indexed range, so they
      land as skeletons (driver creates the row when the contract is
      referenced as a `transaction_participant`).

      Audit team's note in `docs/audits/2026-05-12-ch-pilot-endpoint-audit.md`
      §Method-insights #5 already records the cross-check trick:
      `stellar_sdk.Asset(code, issuer).contract_id(PUBLIC)` byte-matches
      the live SAC contract_id. This task wires that derivation into
      the indexer as a one-way "forward-derive from observed asset"
      pass during persist.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Cross-reference: Karol's 0197 audit branch carries an
      independent finding doc for this same bug at
      `docs/audits/2026-05-13-pre-audit-finding-sac-detection-misses-pre-existing-contracts.md`
      (will land in the 0197 PR merge). Empirical pin in his doc:
      pubnet ledgers 50944000..50955110 backfill, contracts
      `CAS3J7GY…` (XLM SAC), `CCW67TSZ…` (USDC), `CB2XMFB6…` (yBTC),
      `CDOFW7HN…` (yUSDC) all carried `is_sac=false` until fixed.

      Karol's proposed fix differs in approach: he proposes a single
      RPC `getLedgerEntry { LedgerKey::ContractData(contract_id,
      ScVal::LedgerKeyContractInstance) }` per never-before-seen
      contract, decoded to extract the `ContractIdPreimage::FromAsset`
      preimage — same plumbing reused for bugs #1, #2, and #4. This
      task's forward-derive path is **complementary, not alternative**:

      - **Forward-derive (this task)** — free, no RPC. Catches every
        SAC whose underlying classic / native asset is observed
        in-window (most SACs in production: XLM, USDC, AQUA, EURC,
        every asset that ever touches a trustline change in the
        range).
      - **RPC fallback (future task)** — one call per first-seen
        contract that the forward-derive missed. Layered on top so
        we don't pay RPC cost for the common case.

      **Dependency on Bug #1 fix (task 0219, spawned in the same
      commit cluster)**: the helper consumes `ExtractedAsset`
      entries; production `detect_assets` emits only `Sac` +
      `Soroban` variants today (Karol's Bug #1) — classic-credit
      assets are never produced from observed trustlines. Until
      0219 lands, this task's production effect is **zero** (the
      helper has no input). The integration test passes because the
      fixture manually injects a `ClassicCredit` ExtractedAsset.

      Sequence to full end-to-end:

      1. Ship this PR (helper + persist routing + tests + docs).
         Production-inert until step 2.
      2. Ship 0219 — classic-credit ExtractedAsset emission from
         trustline observations.
      3. Production effect kicks in automatically: every observed
         trustline → classic-credit asset → forward-derived SAC
         contract_id → skeleton row flipped.
      4. (Future) RPC fallback for residual SACs whose classic
         asset never appears via trustlines in-window — separate
         task, coordinated with the 0214 RPC infrastructure
         (CH initial-snapshot account state) since both need the
         same Soroban RPC client layer.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Activated parallel to PR #180 (0217 quarantine + 0118 Patch C
      revert). Branch `fix/0218_is-sac-false-for-pre-existing-sac` cut
      from develop. Files touched are disjoint from PR #180's surface
      (sac.rs + staging contract path + persist post-upsert step vs.
      0217's NFT filter + `_pending` tables), so the two PRs can be
      reviewed independently. 0218 ships after #180 merges.
  - date: '2026-05-13'
    status: active
    who: stkrolikiewicz
    note: >
      Implementation shipped on the branch in four phases:

      A. **Parser helper** — `xdr_parser::sac::derive_sac_overrides_from_assets`
      pure-function helper + `SacOverride { contract_id, identity }`
      shape. 7 new unit tests pin native XLM SAC and USDC SAC against
      live mainnet contract_ids; cover skip paths (`Sac` /
      `Soroban` / missing code / missing issuer / invalid issuer
      StrKey) so derivation failures never abort a ledger.

      B. **Staging + persist** — `Staged.sac_overrides` populated in
      `Staged::prepare`. New persist step
      `apply_sac_overrides_for_skeleton_contracts` runs inside the
      persist tx between `insert_assets_from_reclassified_contracts`
      and `upsert_nfts_and_ownership`; the `WHERE is_sac = FALSE`
      guard makes it idempotent and a no-op on already-classified
      rows. Network passphrase hardcoded to mainnet — TODO: lift to
      config when testnet / futurenet ingest ships.

      C. **Integration tests** — two DB-gated tests:
      `sac_override_flips_is_sac_for_pre_existing_skeleton` (happy
      path: skeleton → is_sac=true + contract_type=Token) and
      `sac_override_leaves_already_is_sac_rows_alone` (idempotency:
      pre-flipped row not touched on replay).

      D. **Docs** — `database-schema-overview.md` §4.6 gains the
      3-path `is_sac` classification note; `clickhouse-pilot.md`
      gains a "Writer-only behaviours not yet ported to CH"
      subsection naming the SAC override as a CH parity follow-up.

      `cargo check --workspace` + `cargo clippy -p indexer -p
      xdr-parser --all-targets -- -D warnings` clean. Empirical
      replay (post-merge backfill rerun, count is_sac=true increase)
      is operational follow-up — not part of the PR.
---

# BUG: is_sac=false for pre-existing SAC contracts

## Summary

`crates/xdr-parser/src/state.rs::extract_contract_deployments` infers
`is_sac=true` only when it observes an in-window `LedgerEntryChange`
for a contract whose `executable.type == "stellar_asset"`. SAC contracts
deployed BEFORE the indexed window never produce such a change in the
range, so they get default `is_sac=false` skeleton rows from the
driver path (`accounts` → `soroban_contracts` row creation on first
`transaction_participants` reference).

Empirically: 100% of pre-window SACs in the audit DB land as
`is_sac=false, contract_type=Other, sac_asset=NULL` — they look
indistinguishable from genuinely-unknown contracts even though their
classification is deterministic and re-derivable from observed asset
metadata.

## Fix strategy

**Forward-derive SAC contract_ids from observed classic assets.** Every
classic asset observed in the indexed range (native XLM, classic-credit
trustlines, payments, offers, etc.) has a deterministic SAC contract_id
computed from `(asset_code, issuer, network_passphrase)`. The
derivation function already exists:
`xdr_parser::sac::derive_sac_contract_id` (verified during the
2026-05-12 audit's SAC byte-for-byte cross-check).

Persist-time UPDATE flips `is_sac=true, sac_asset=...` on any
`soroban_contracts` skeleton row whose `contract_id` matches a derived
SAC. Idempotent and additive — only flips rows that are currently
`is_sac=false` (so a re-classified `Token` row from the existing path
is left alone).

## Implementation plan

### Phase 1 — parser helper

New public surface in `crates/xdr-parser/src/sac.rs`:

```rust
#[derive(Debug, Clone)]
pub struct SacOverride {
    pub contract_id: String,         // StrKey
    pub asset: Asset,                // XDR Asset enum
}

#[instrument(skip(assets), fields(asset_count = assets.len()))]
pub fn derive_sac_overrides_from_assets(
    assets: &[ExtractedAsset],
    network_passphrase: &str,
) -> Vec<SacOverride>;
```

Pure function: takes the staged `ExtractedAsset` list, derives the
SAC contract_id per asset via `derive_sac_contract_id(
ContractIdPreimage::Asset(asset.to_xdr()), &network_id)`, returns
the override list. No I/O, no DB.

Unit tests (in `sac.rs::tests`):

- Native XLM → CAS3J7GY (known mainnet SAC for XLM).
- Classic credit asset (USDC issued by `GA5ZSEJYB3...`) → derived
  contract_id byte-matches mainnet wrapper.
- Soroban-only asset (`asset_type=3`, no SAC) → not emitted (or emitted
  with `sac_asset=None`; decide in impl).

### Phase 2 — staging + persist integration

`Staged` struct gains `sac_overrides: Vec<SacOverride>`. Populated
during the contract-deployment staging path alongside the existing
`contract_rows` build.

`crates/indexer/src/handler/persist/write.rs` gets a new step
`apply_sac_overrides_for_skeleton_rows` invoked between
`upsert_contracts` and the NFT filter:

```sql
UPDATE soroban_contracts sc
   SET is_sac = TRUE,
       sac_asset = t.asset_xdr,
       contract_type = COALESCE(sc.contract_type, 0)  -- Token = 0
  FROM UNNEST($1::VARCHAR[], $2::BYTEA[]) AS t(cid, asset_xdr)
 WHERE sc.contract_id = t.cid
   AND sc.is_sac = FALSE
```

Idempotent on replay (`is_sac = FALSE` guard short-circuits no-op
writes). Runs inside the persist transaction so the NFT filter step
that follows sees the corrected `is_sac` / `contract_type` and drops
those rows before they hit the quarantine.

### Phase 3 — integration test

`crates/indexer/tests/persist_integration.rs`:

- Fixture: known classic credit asset (USDC-like with a fixed test
  issuer) observed via a trustline change; no `create_contract` op
  in the test ledger for the corresponding SAC.
- Expectation: post-persist, `SELECT is_sac, contract_type, sac_asset
FROM soroban_contracts WHERE contract_id = $derived_sac` returns
  `(true, Token, <asset xdr>)`.
- Negative case: a Soroban-only asset (no SAC mapping) produces no
  override and the corresponding contract row is left alone.

### Phase 4 — CH writer parity

Decide alongside the CH writer parity follow-up for task 0217 (CH
`_pending` routing). The same `SacOverride` list can drive a CH
`ALTER TABLE ... UPDATE` mutation, but RMT semantics suggest the
cleaner path is to merge the override into the `ContractRow` at stage
time and rely on the `ReplacingMergeTree(wasm_uploaded_at_ledger)`
version semantics to absorb the corrected row. Document in the task's
implementation notes.

## Acceptance Criteria

- [x] `xdr_parser::sac::derive_sac_overrides_from_assets` public + unit-tested. _(7 new tests in `sac::tests`; pins against mainnet XLM SAC `CAS3J7GY…` + USDC SAC `CCW67TSZ…`.)_
- [x] `Staged.sac_overrides` populated from the asset staging path. _(`crates/indexer/src/handler/persist/staging.rs::Staged::prepare`; mainnet passphrase hardcoded — refactor to config when a testnet/futurenet variant ships.)_
- [x] `apply_sac_overrides_for_skeleton_contracts` UPDATEs `soroban_contracts` inside the persist tx; idempotent on replay. _(`crates/indexer/src/handler/persist/write.rs`; `WHERE is_sac = FALSE` guard makes the UPDATE a no-op on already-classified rows; wired between `insert_assets_from_reclassified_contracts` and `upsert_nfts_and_ownership` in `run_all_steps`.)_
- [x] Integration test: pre-existing SAC referenced + observed trustline → `is_sac=true` + `contract_type=Token`. _(Two DB-gated tests: `sac_override_flips_is_sac_for_pre_existing_skeleton` exercises the happy path; `sac_override_leaves_already_is_sac_rows_alone` exercises idempotency. **Note:** the original AC mentioned `sac_asset` populated — there is no `sac_asset` column in `soroban_contracts` (the asset identity lives in the `assets` table via the `(contract_id, asset_code, issuer_id)` row); flipping `is_sac`+`contract_type` is the full schema-side delivery here.)_
- [ ] **Empirical replay**: re-run a backfill window that previously held pre-existing SACs (e.g. XLM SAC) and verify `SELECT count(*) FILTER (WHERE is_sac = true) FROM soroban_contracts` increases by the expected delta (target: ≥ 5 pre-window SACs in a 10k-ledger window). _(Operational — run after the PR lands and a fresh backfill is kicked.)_
- [x] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md` §4.6 `soroban_contracts` gains a 3-path classification note (in-window deploy / forward-derive / future RPC fetch); `docs/architecture/database-schema/clickhouse-pilot.md` gains a "Writer-only behaviours not yet ported to CH" subsection that names the SAC override path explicitly as a CH parity follow-up.
- [x] **API types regenerated** — N/A (no API contract change — `is_sac` already in the contract response shape).

## Out of Scope

- CH writer parity beyond the schema-side documentation note — same
  follow-up bucket as task 0217's CH writer parity for `_pending` routing.
- Backfilling existing skeleton rows on already-indexed environments —
  separate operational runbook (analog to the 0217 initial-migration
  runbook); spawn alongside or after Phase 1–3 ship.
- Bug #1 / #5 / #6 from Karol's pre-audit (different scope — classic
  credit asset rows, enricher worker signature, transient classifier).

## Notes

- Side-effect on task 0217 quarantine: with this fix, pre-existing SACs
  classify as `Token` at first reference instead of staying `Other`, so
  they drop at filter time and never enter `nfts_pending`. The 0217
  post-backfill drain runbook §Part 2 therefore stops needing to TRUNCATE
  SAC stragglers; only true `Other` (genuinely unknown) contracts
  remain in pending pre-drain.
- `derive_sac_contract_id` is pure Rust (no external deps beyond
  `stellar-xdr` already in the crate). The function panics only on
  malformed `ContractIdPreimage`; the wrapping helper should `warn!`
  - skip on derivation failure rather than abort the ledger persist.
