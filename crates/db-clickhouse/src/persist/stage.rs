//! Synchronous pre-write transform from parser `Extracted*` slices to
//! CH-shaped row structs in [`super::rows`].
//!
//! ## Design — hybrid surrogate / natural keys
//!
//! Three high-cardinality FK hubs (`accounts`, `soroban_contracts`,
//! `transactions`) carry surrogate `id: Int64` derived via
//! [`super::ids`] (`cityhash64(natural_key)`, deterministic). All FK
//! columns pointing at these tables are `Int64` referencing those
//! ids. Cheaper joins, smaller storage, faster scans vs natural-key
//! everywhere — see `ids.rs` module docs for the why.
//!
//! Other tables (`assets`, `nfts`, `liquidity_pools`,
//! `liquidity_pool_snapshots`, `operations_appearances`,
//! `transaction_participants`, `nft_ownership`, `lp_positions`,
//! `account_balances_current`) keep natural / composite primary
//! keys where they're already cheap.
//!
//! ## Three structural differences from PG path
//!
//! 1. **`soroban_events` is unfolded** per ADR 0044 §Decision §4a —
//!    one CH row per `ExtractedEvent` (vs PG's per-`(contract, tx,
//!    ledger)` fold).
//! 2. **`Decimal128(7)` scaling happens here.** Parser emits decimal
//!    strings or i64 stroops; CH RowBinary expects underlying `i128`
//!    scaled by 10⁷ (1 stroop = 1 i128 unit).
//! 3. **Stub-rowing for `soroban_contracts`.** Pass 2 emits NULL-rich
//!    rows for contracts referenced by events/ops/invocations but
//!    not deployed in this batch. Mirrors PG `write::upsert_contracts_
//!    returning_id` Pass 2 — keeps mid-stream backfill ranges
//!    JOIN-able even without preceding history.
//!
//! ## Trustline removal handling
//!
//! Parser-emitted `removed_trustlines` are translated to a unified
//! `BalanceRow` with `amount = 0` and current ledger as
//! `last_updated_ledger`. `ReplacingMergeTree(last_updated_ledger)`
//! keeps the zero-balance row (newest version wins). Read-time
//! convention: `WHERE amount > 0` to recover "active trustlines"
//! semantics.

use std::collections::{HashMap, HashSet};

use domain::{AssetType, ContractEventType, ContractType, OperationType};
use serde_json::Value;
use xdr_parser::ExtractedContractMetadata;
use xdr_parser::ExtractedSorobanBalance;
use xdr_parser::SacOverride;
use xdr_parser::types::{
    EventSource, ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment,
    ExtractedContractInterface, ExtractedEvent, ExtractedInvocation, ExtractedLedger,
    ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft,
    ExtractedNftEvent, ExtractedOperation, ExtractedTransaction, SacAssetIdentity,
};

use xdr_parser::event::extract_executable_update_new_wasm_hash;

use super::ids;
use super::rows::*;
use crate::SchemaError;

/// Sum `gross_volume_a` (asset-A-side trade volume, stroops) per pool from the
/// `claimedAtoms` the parser attaches to path-payment / offer ops (the 0261
/// claim-atom extractor emits `amountA` per atom). Keyed by raw 32-byte pool id
/// to match [`LiquidityPoolSnapshotRow`]. Trades only — LP deposits/withdrawals
/// carry no claimed atoms. Shared by live ingest (via [`prepare`]) and the 0266
/// backfill worker, so the value is identical on either path.
pub fn gross_volume_a_by_pool(
    operations: &[(String, Vec<ExtractedOperation>)],
) -> HashMap<[u8; 32], i128> {
    let mut gross: HashMap<[u8; 32], i128> = HashMap::new();
    for (_tx, ops) in operations {
        for op in ops {
            let Some(atoms) = op.details.get("claimedAtoms").and_then(Value::as_array) else {
                continue;
            };
            for atom in atoms {
                let (Some(pool_hex), Some(amount_a)) = (
                    atom.get("poolId").and_then(Value::as_str),
                    atom.get("amountA").and_then(Value::as_i64),
                ) else {
                    continue;
                };
                let Ok(bytes) = hex::decode(pool_hex) else {
                    continue;
                };
                let Ok(pool_id) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                    continue;
                };
                *gross.entry(pool_id).or_insert(0) += i128::from(amount_a);
            }
        }
    }
    gross
}

#[derive(Debug, Default)]
pub struct StagedLedger {
    pub ledger_sequence: i64,

    pub ledger_rows: Vec<LedgerRow>,
    pub account_rows: Vec<AccountRow>,
    pub wasm_rows: Vec<WasmInterfaceMetadataRow>,
    pub contract_rows: Vec<SorobanContractRow>,
    /// On-chain Soroban token metadata side table (task 0297). Populated inside
    /// [`prepare_with_sac_overrides`] via [`build_metadata_rows`] from the
    /// `StageInputs.contract_metadata_writes` slice.
    pub metadata_rows: Vec<SorobanContractMetadataRow>,
    pub transaction_rows: Vec<TransactionRow>,
    pub hash_index_rows: Vec<TransactionHashIndexRow>,
    pub participant_rows: Vec<TransactionParticipantRow>,
    pub pool_rows: Vec<LiquidityPoolRow>,
    pub snapshot_rows: Vec<LiquidityPoolSnapshotRow>,
    pub lp_position_rows: Vec<LpPositionRow>,
    pub op_rows: Vec<OperationAppearanceRow>,
    pub event_rows: Vec<SorobanEventRow>,
    pub invocation_rows: Vec<SorobanInvocationAppearanceRow>,
    pub asset_rows: Vec<AssetRow>,
    /// SAC facet rows (ADR 0051) → `asset_sac` AggregatingMergeTree side table.
    pub asset_sac_rows: Vec<AssetSacRow>,
    pub nft_rows: Vec<NftRow>,
    pub nft_ownership_rows: Vec<NftOwnershipRow>,
    /// Task 0217 / 0220 — quarantine bucket for NFT rows whose
    /// contract is still `Other` / NULL-classified at staging time.
    /// Routed alongside `nft_rows` via the per-contract verdict
    /// computed from observed WASM interfaces in this ledger plus the
    /// parser-emitted `contract_type` on each deployment. CH has no
    /// per-row UPDATE, so promotion happens only via the post-backfill
    /// drain runbook.
    pub nft_pending_rows: Vec<NftPendingRow>,
    pub nft_ownership_pending_rows: Vec<NftOwnershipPendingRow>,
    /// Unified `balances` rows for ALL asset types (task 0331 Option A). Type-3
    /// tokens are built in [`prepare_with_sac_overrides`] via [`build_balance_rows`]
    /// from `StageInputs.soroban_token_balances`; classic + native per-account
    /// balances are appended straight from `account_states` (single-write — the
    /// legacy `account_balances_current` staging was removed).
    pub unified_balance_rows: Vec<BalanceRow>,
}

/// Named, borrowed inputs to [`prepare_with_sac_overrides`].
///
/// Replaces the former ~18 positional arguments: many are `&[T]` slices and a
/// few share types, so a positional call could silently transpose two. Named
/// fields make the call site readable and a wrong order a compile error. `Copy`
/// (every field is a shared reference) so the stage body can destructure it
/// back into locals with zero ceremony.
#[derive(Clone, Copy)]
pub struct StageInputs<'a> {
    pub ledger: &'a ExtractedLedger,
    pub transactions: &'a [ExtractedTransaction],
    pub operations: &'a [(String, Vec<ExtractedOperation>)],
    pub events: &'a [(String, Vec<ExtractedEvent>)],
    pub invocations: &'a [(String, Vec<ExtractedInvocation>)],
    pub contract_interfaces: &'a [ExtractedContractInterface],
    pub contract_deployments: &'a [ExtractedContractDeployment],
    pub account_states: &'a [ExtractedAccountState],
    pub liquidity_pools: &'a [ExtractedLiquidityPool],
    pub pool_snapshots: &'a [ExtractedLiquidityPoolSnapshot],
    pub assets: &'a [ExtractedAsset],
    pub nfts: &'a [ExtractedNft],
    pub nft_events: &'a [ExtractedNftEvent],
    pub lp_positions: &'a [ExtractedLpPosition],
    /// On-chain Soroban token metadata writes (task 0297). Threaded through to
    /// `metadata_rows` via [`build_metadata_rows`] inside
    /// [`prepare_with_sac_overrides`]. Empty `&[]` for legacy callers.
    pub contract_metadata_writes: &'a [ExtractedContractMetadata],
    /// Per-holder Soroban token (type-3) balances from `ContractData`
    /// `Balance(Address)` entries (task 0331). Threaded to the unified
    /// `unified_balance_rows` via [`build_balance_rows`]. Empty `&[]` for
    /// legacy callers.
    pub soroban_token_balances: &'a [ExtractedSorobanBalance],
    /// Task 0331 + ADR 0051 — SAC contract surrogate → wrapped classic/native
    /// `asset_id` (from `asset_sac`, via [`crate::persist::fetch_sac_classic_map`]).
    /// [`build_balance_rows`] keys a contract-held SAC balance onto the classic
    /// asset it wraps instead of the SAC surrogate (which has no `assets` row of its
    /// own). Empty map for legacy callers (SAC balances keep their surrogate key).
    pub sac_classic: &'a HashMap<i64, i64>,
    /// Crypto-proven un-deployed-SAC overrides for this ledger's events
    /// (task 0323, `xdr_parser::detect_undeployed_sac_overrides`). Each
    /// suppresses the Pass-2 FK stub (no contract row) + seeds a SAC `assets`
    /// row. Empty for legacy callers.
    pub sac_overrides: &'a [SacOverride],
    /// Task 0283 live G1 — cross-ledger WASM verdicts by `wasm_hash`. Empty map
    /// for legacy callers (behaves exactly as pre-0283).
    pub prior_wasm_verdicts: &'a HashMap<[u8; 32], ContractType>,
    /// Task 0283 live G9 — cross-ledger contract verdicts by `contract_id`.
    pub prior_contract_verdicts: &'a HashMap<String, ContractType>,
    /// Task 0320 live WASM-upgrade — prior `soroban_contracts` rows (read back
    /// in full) for the contracts that emit an `executable_update` this ledger,
    /// so [`build_wasm_upgrade_rows`] can carry identity forward when it rewrites
    /// `wasm_hash`. Empty map for legacy callers (no upgrade rows emitted).
    pub prior_contract_rows: &'a HashMap<String, SorobanContractRow>,
}

#[allow(clippy::too_many_arguments)]
pub fn prepare(
    ledger: &ExtractedLedger,
    transactions: &[ExtractedTransaction],
    operations: &[(String, Vec<ExtractedOperation>)],
    events: &[(String, Vec<ExtractedEvent>)],
    invocations: &[(String, Vec<ExtractedInvocation>)],
    contract_interfaces: &[ExtractedContractInterface],
    contract_deployments: &[ExtractedContractDeployment],
    account_states: &[ExtractedAccountState],
    liquidity_pools: &[ExtractedLiquidityPool],
    pool_snapshots: &[ExtractedLiquidityPoolSnapshot],
    assets: &[ExtractedAsset],
    nfts: &[ExtractedNft],
    nft_events: &[ExtractedNftEvent],
    lp_positions: &[ExtractedLpPosition],
) -> Result<StagedLedger, SchemaError> {
    // Convenience wrapper: no SAC overrides, no cross-ledger verdicts (the
    // legacy / test path). Behaves exactly as the pre-StageInputs `prepare`.
    prepare_with_sac_overrides(&StageInputs {
        ledger,
        transactions,
        operations,
        events,
        invocations,
        contract_interfaces,
        contract_deployments,
        account_states,
        liquidity_pools,
        pool_snapshots,
        assets,
        nfts,
        nft_events,
        lp_positions,
        contract_metadata_writes: &[],
        soroban_token_balances: &[],
        sac_classic: &HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &HashMap::new(),
        prior_contract_verdicts: &HashMap::new(),
        prior_contract_rows: &HashMap::new(),
    })
}

/// Build corrected `soroban_contracts` rows for contracts that emitted an
/// `executable_update` SYSTEM event this ledger (task 0320, live path).
///
/// Per `executable_update` event with a parseable new WASM hash AND a known
/// prior row, emit a full row that **overrides** `wasm_hash` +
/// `wasm_uploaded_at_ledger` (= the upgrade ledger, the RMT version that wins
/// the merge) and **carries forward** the identity columns. `contract_type` is
/// carried forward unchanged — the class never net-changes on upgrade for
/// current data; the rare flip is task 0325.
///
/// Events without a parseable hash, without a contract address, or **without a
/// prior row are skipped** — emitting a partial row would clobber the identity
/// columns to NULL under RMT. Multiple upgrades of one contract in the same
/// ledger collapse to the last (on-chain application order).
pub fn build_wasm_upgrade_rows(
    events: &[(String, Vec<ExtractedEvent>)],
    prior: &HashMap<String, SorobanContractRow>,
    ledger_sequence: i64,
) -> Vec<SorobanContractRow> {
    // Keyed by contract_id so multiple upgrades of one contract in the same
    // ledger collapse to the last seen (= on-chain application order).
    let mut by_contract: HashMap<String, SorobanContractRow> = HashMap::new();
    for (_tx_hash, evs) in events {
        for ev in evs {
            // Only consensus events drive state. The diagnostic container holds
            // byte-identical copies of consensus events AND events from FAILED
            // transactions (an upgrade that never applied) — acting on those would
            // write a `wasm_hash` the chain never adopted. Mirror the `soroban_events`
            // staging guard (this is the same population the backfill reads, post-drop).
            if is_diagnostic(ev.source) {
                continue;
            }
            // Only the host emits `executable_update`, and always as a SYSTEM
            // event. A contract can emit a Contract-typed event with the same
            // topic shape; requiring System blocks that spoof of its own
            // `wasm_hash` (and never drops a real upgrade — all are System).
            if ev.event_type != ContractEventType::System {
                continue;
            }
            let Some(addr) = ev.contract_id.as_deref() else {
                continue;
            };
            // `extract_…` returns `Some` only for a well-formed executable_update.
            let Some(new_hash) = extract_executable_update_new_wasm_hash(&ev.topics) else {
                continue;
            };
            // Skip-on-miss: without the prior row we cannot carry identity
            // columns forward, and a partial row would NULL them under RMT.
            let Some(prior_row) = prior.get(addr) else {
                continue;
            };
            // Carry the prior row forward verbatim, overriding only what the
            // upgrade actually changes: the new WASM hash and the RMT version
            // (= upgrade ledger, so it wins the merge). `id` / `contract_id` /
            // deployer / name / contract_type / is_sac all ride along from the
            // read-back row — matching the backfill SQL, which also passes
            // `is_sac` through (no upgrader is a mislabeled SAC on current data).
            let mut row = prior_row.clone();
            row.wasm_hash = Some(new_hash);
            row.wasm_uploaded_at_ledger = ledger_sequence;
            by_contract.insert(addr.to_string(), row);
        }
    }
    by_contract.into_values().collect()
}

/// Map parser-extracted token-metadata writes to `soroban_contract_metadata`
/// rows (task 0297). Called inside [`prepare_with_sac_overrides`] from the
/// `StageInputs.contract_metadata_writes` slice. SAC filtering already happened
/// in the producer (`xdr_parser::extract_contract_metadata_writes`); `version` =
/// observed ledger.
pub fn build_metadata_rows(
    writes: &[ExtractedContractMetadata],
) -> Vec<SorobanContractMetadataRow> {
    writes
        .iter()
        .map(|w| SorobanContractMetadataRow {
            contract_id: w.contract_id.clone(),
            name: w.metadata.name.clone(),
            symbol: w.metadata.symbol.clone(),
            decimals: w.metadata.decimals,
            version: i64::from(w.ledger),
        })
        .collect()
}

/// Map parser-extracted Soroban token balances to unified `balances` rows
/// (task 0331, Option C). `holder_id` = `ids::address_id(holder)`; `amount` raw.
///
/// `asset_id` is resolved from the STORING contract's surrogate: a SAC is NOT a
/// distinct asset (ADR 0051 retired `asset_type=2`) and has no `assets` row of its
/// own, so a contract-held SAC balance must key by the classic/native asset it
/// wraps or it would orphan. `sac_classic` maps a SAC contract surrogate → its
/// classic/native `asset_id` (from the `asset_sac` facet, via
/// [`crate::persist::fetch_sac_classic_map`]); a storing contract ABSENT from the
/// map is a type-3 token and keeps its own surrogate (`ids::asset_id(3, …)`). The
/// resolution happens HERE, at build time — the live indexer (`prepare_with_sac_overrides`)
/// and the RPC seed both call this one shared fn with the same map, so neither
/// post-mutates the staged rows.
pub fn build_balance_rows(
    balances: &[ExtractedSorobanBalance],
    sac_classic: &HashMap<i64, i64>,
) -> Vec<BalanceRow> {
    // Dedup by (holder_id, asset_id) keeping the LAST occurrence, position-stable:
    // two txs in one ledger can touch the same holder+asset, producing rows that
    // share the RMT version (`last_updated_ledger`) — a tie the merge would resolve
    // nondeterministically. Ledger/tx order puts the final state last, so last-wins
    // is correct; the first-seen position is preserved for deterministic output.
    let mut rows: Vec<BalanceRow> = Vec::with_capacity(balances.len());
    let mut idx: HashMap<(i64, i64), usize> = HashMap::with_capacity(balances.len());
    for b in balances {
        let contract = ids::contract_id(&b.contract_id);
        let holder_id = ids::address_id(&b.holder);
        let asset_id = sac_classic
            .get(&contract)
            .copied()
            .unwrap_or_else(|| ids::asset_id(3, "", 0, contract));
        let row = BalanceRow {
            holder_id,
            asset_id,
            amount: b.balance,
            last_updated_ledger: i64::from(b.ledger),
        };
        match idx.get(&(holder_id, asset_id)) {
            Some(&i) => rows[i] = row,
            None => {
                idx.insert((holder_id, asset_id), rows.len());
                rows.push(row);
            }
        }
    }
    rows
}

/// Same as [`prepare`] but also consumes `sac_overrides` — the crypto-proven
/// un-deployed-SAC emitters for this ledger (task 0323). An un-deployed SAC is
/// modelled as an ASSET, not a contract: each override (a) suppresses the
/// Pass-2 FK stub so NO `soroban_contracts` row is written for it, and (b)
/// seeds a SAC `assets` row from its `identity`. (Replaces the task-0220
/// `is_sac=true` skeleton re-insert, which wrote a contract row instead.)
///
/// Production callers that have a `ParseOutput.sac_overrides` slice
/// (PG-side bridge for task 0218 + the CH backfill path) call this
/// directly; legacy callers via [`prepare`] get a no-op override list
/// and behave exactly as before — the override mechanism stays opt-in
/// at the call site, so the addition is fully backward-compatible.
///
/// `prior_wasm_verdicts` (task 0283 live G1 fix) carries cross-ledger
/// WASM verdicts the pure stage cannot see: on Soroban `uploadContractWasm`
/// and `createContract` are separate transactions in (almost always)
/// different ledgers, so a deploy's WASM is invisible to the same-ledger
/// `wasm_classification` map below and the contract would persist the parser
/// default `Other`. The writer pre-fetches the verdict for such hashes from
/// the already-persisted `wasm_interface_metadata` (see
/// `persist::fetch_prior_wasm_verdicts`) and passes it here; the deploy
/// override consults it as a fallback after the same-ledger map. Legacy
/// callers via [`prepare`] pass an empty map and behave exactly as before.
pub fn prepare_with_sac_overrides(input: &StageInputs<'_>) -> Result<StagedLedger, SchemaError> {
    // Destructure back into locals (StageInputs is `Copy`) so the body below is
    // unchanged from the positional-argument era — every name matches.
    let StageInputs {
        ledger,
        transactions,
        operations,
        events,
        invocations,
        contract_interfaces,
        contract_deployments,
        account_states,
        liquidity_pools,
        pool_snapshots,
        assets,
        nfts,
        nft_events,
        lp_positions,
        contract_metadata_writes,
        soroban_token_balances,
        sac_classic,
        sac_overrides,
        prior_wasm_verdicts,
        prior_contract_verdicts,
        prior_contract_rows,
    } = *input;

    let ledger_sequence_i64 = i64::from(ledger.sequence);
    let ledger_hash = decode_hash(&ledger.hash, "ledger.hash")?;
    let ledger_closed_at_ms = ledger.closed_at.saturating_mul(1_000);

    let mut out = StagedLedger {
        ledger_sequence: ledger_sequence_i64,
        ..Default::default()
    };

    out.ledger_rows.push(LedgerRow {
        sequence: ledger_sequence_i64,
        hash: ledger_hash,
        closed_at: ledger_closed_at_ms,
        protocol_version: i32::try_from(ledger.protocol_version)
            .map_err(|_| staging_err("protocol_version overflow"))?,
        transaction_count: i32::try_from(ledger.transaction_count)
            .map_err(|_| staging_err("transaction_count overflow"))?,
        base_fee: i64::from(ledger.base_fee),
    });

    // ---- Accounts universe + per-tx participant union ----
    let mut account_keys: HashSet<String> = HashSet::new();
    let mut participants_per_tx: HashMap<String, HashSet<String>> = HashMap::new();
    let has_soroban: HashMap<String, bool> = tx_has_soroban_map(operations);
    // O(1) per-tx op count lookup. Built once over `operations` so the
    // transactions loop stays linear in `transactions.len()` rather than
    // the prior O(tx_count × op_groups) `iter().find()` scan.
    let op_count_by_tx: HashMap<&str, i16> = operations
        .iter()
        .map(|(h, ops)| (h.as_str(), i16::try_from(ops.len()).unwrap_or(i16::MAX)))
        .collect();

    for tx in transactions {
        account_keys.insert(tx.source_account.clone());
        participants_per_tx
            .entry(tx.hash.clone())
            .or_default()
            .insert(tx.source_account.clone());
    }

    for (tx_hash, ops) in operations {
        let entry = participants_per_tx.entry(tx_hash.clone()).or_default();
        for op in ops {
            if let Some(src) = &op.source_account {
                account_keys.insert(src.clone());
                entry.insert(src.clone());
            }
            for key in op_participant_str_keys(op.op_type, &op.details) {
                account_keys.insert(key.clone());
                entry.insert(key);
            }
        }
    }

    for (tx_hash, evs) in events {
        let entry = participants_per_tx.entry(tx_hash.clone()).or_default();
        for ev in evs {
            if let Some((from, to)) = xdr_parser::transfer_participants(&ev.topics) {
                for participant in [from, to] {
                    if is_strkey_account(&participant) {
                        account_keys.insert(participant.clone());
                        entry.insert(participant);
                    }
                }
            }
        }
    }

    for (tx_hash, invs) in invocations {
        let entry = participants_per_tx.entry(tx_hash.clone()).or_default();
        for inv in invs {
            if let Some(caller) = &inv.caller_account
                && is_strkey_account(caller)
            {
                account_keys.insert(caller.clone());
                entry.insert(caller.clone());
            }
        }
    }

    for dep in contract_deployments {
        if let Some(deployer) = &dep.deployer_account {
            account_keys.insert(deployer.clone());
        }
    }

    for st in account_states {
        account_keys.insert(st.account_id.clone());
        for b in st.balances.as_array().into_iter().flatten() {
            if let Some(issuer) = b.get("issuer").and_then(Value::as_str)
                && !issuer.is_empty()
            {
                account_keys.insert(issuer.to_string());
            }
        }
        for rm in &st.removed_trustlines {
            if let Some(issuer) = rm.get("issuer").and_then(Value::as_str)
                && !issuer.is_empty()
            {
                account_keys.insert(issuer.to_string());
            }
        }
    }
    for pool in liquidity_pools {
        if let Some(issuer) = asset_issuer(&pool.asset_a) {
            account_keys.insert(issuer);
        }
        if let Some(issuer) = asset_issuer(&pool.asset_b) {
            account_keys.insert(issuer);
        }
    }
    for asset in assets {
        if let Some(issuer) = &asset.issuer_address {
            account_keys.insert(issuer.clone());
        }
    }
    for nft in nfts {
        if let Some(owner) = &nft.owner_account {
            account_keys.insert(owner.clone());
        }
    }
    for ev in nft_events {
        if let Some(owner) = &ev.owner_account
            && is_strkey_account(owner)
        {
            account_keys.insert(owner.clone());
            participants_per_tx
                .entry(ev.transaction_hash.clone())
                .or_default()
                .insert(owner.clone());
        }
    }
    for lpp in lp_positions {
        account_keys.insert(lpp.account_id.clone());
    }

    let total = account_keys.len();
    account_keys.retain(|k| k.len() <= 56 && k.starts_with('G'));
    let dropped = total - account_keys.len();
    if dropped > 0 {
        tracing::debug!(
            ledger_sequence = ledger.sequence,
            dropped,
            kept = account_keys.len(),
            "dropped non-G-prefix or oversize StrKeys from CH accounts staging"
        );
    }

    let overrides = merge_account_state_overrides(account_states);

    for key in &account_keys {
        let ov = overrides.get(key);
        let last_seen_ledger = ov
            .and_then(|o| o.last_seen_ledger)
            .unwrap_or(ledger_sequence_i64);
        let first_seen_ledger = ov
            .and_then(|o| o.first_seen_ledger)
            .unwrap_or(last_seen_ledger);
        let sequence_number = match ov {
            Some(o) if o.sequence_number >= 0 => o.sequence_number,
            _ => 0,
        };
        out.account_rows.push(AccountRow {
            id: ids::account_id(key),
            account_id: key.clone(),
            first_seen_ledger,
            last_seen_ledger,
            sequence_number,
            home_domain: ov.and_then(|o| o.home_domain.clone()),
        });
    }

    // ---- wasm_interface_metadata (deduped by wasm_hash) ----
    //
    // Task 0118 Phase 2 (PG-side mirror) — run the wasm-spec classifier
    // alongside the metadata dedup. The resulting per-hash verdict
    // feeds two downstream consumers in this same `prepare` call:
    //   * `contract_rows.contract_type` override for non-SAC deploys
    //     whose WASM is uploaded in the same ledger (matches PG
    //     `Staged::prepare` behaviour at staging.rs:578-585).
    //   * NFT-candidate routing (`nft_rows` / `nft_pending_rows`,
    //     task 0217 / 0220) — `Other`/NULL verdict routes to
    //     quarantine; `Fungible` / `Token` drops the row entirely.
    let mut wasm_seen: HashSet<[u8; 32]> = HashSet::new();
    let mut wasm_classification: HashMap<[u8; 32], ContractType> =
        HashMap::with_capacity(contract_interfaces.len());
    for iface in contract_interfaces {
        let hash = decode_hash(&iface.wasm_hash, "wasm_hash")?;
        if !wasm_seen.insert(hash) {
            continue;
        }
        let classification = xdr_parser::classify_contract_from_wasm_spec(&iface.functions);
        wasm_classification.insert(hash, classification.into());

        // Task 0327: persist the mutability bit so the API can surface the
        // Upgradeable/Immutable badge. Read back via
        // `JSONExtractBool(metadata,'upgradeable')`; rows written before this
        // (no key) read as Unknown → chip renders nothing.
        let metadata = serde_json::json!({
            "functions": iface.functions,
            "wasm_byte_len": iface.wasm_byte_len,
            "upgradeable": iface.upgradeable,
        });
        out.wasm_rows.push(WasmInterfaceMetadataRow {
            wasm_hash: hash,
            metadata: serde_json::to_string(&metadata)
                .map_err(|e| staging_err(&format!("wasm metadata serialize: {e}")))?,
        });
    }

    // ---- soroban_contracts (deduped by contract_id) ----
    let mut contract_seen: HashSet<String> = HashSet::new();
    for dep in contract_deployments {
        if !contract_seen.insert(dep.contract_id.clone()) {
            continue;
        }
        let wasm_hash = match &dep.wasm_hash {
            Some(h) => Some(decode_hash(h, "deployment.wasm_hash")?),
            None => None,
        };
        let deployed = i64::from(dep.deployed_at_ledger);
        // Task 0118 Phase 2 (PG-side mirror) — if this deployment's
        // wasm carries a definitive `Nft` / `Fungible` verdict, override
        // the parser default (`Other` for non-SAC) before the row reaches
        // CH. SAC deploys stay `Token` (is_sac short-circuits WASM
        // classification — SACs have no WASM).
        //
        // Verdict source, in precedence order:
        //   1. `wasm_classification` — WASM uploaded in THIS ledger.
        //   2. `prior_wasm_verdicts` — WASM uploaded in an EARLIER ledger,
        //      pre-fetched by the writer from `wasm_interface_metadata`
        //      (task 0283 live G1). This is the common Soroban case
        //      (upload + deploy are separate txs / ledgers); without it
        //      the contract would persist `Other` and its NFT events would
        //      route to quarantine until the batch backstop drains them.
        let mut contract_type = dep.contract_type;
        if !dep.is_sac
            && let Some(hash) = wasm_hash
            && let Some(classified) = wasm_classification
                .get(&hash)
                .or_else(|| prior_wasm_verdicts.get(&hash))
                .copied()
            && matches!(classified, ContractType::Nft | ContractType::Fungible)
        {
            contract_type = classified;
        }
        out.contract_rows.push(SorobanContractRow {
            id: ids::contract_id(&dep.contract_id),
            contract_id: dep.contract_id.clone(),
            wasm_hash,
            wasm_uploaded_at_ledger: deployed,
            deployer_id: dep.deployer_account.as_deref().map(ids::account_id),
            deployed_at_ledger: Some(deployed),
            contract_type: Some(contract_type as i16),
            is_sac: dep.is_sac,
            name: dep.name.clone(),
        });
    }

    // (task 0297) On-chain token name/symbol/decimals → the dedicated
    // `soroban_contract_metadata` side table. A pure 1:1 map of the producer's
    // output (extraction + SAC-skip already done), built here like the other
    // `out.*` rows rather than post-`prepare`. This coexists with the legacy
    // `Symbol("name")` path (parser `extract_contract_data_name_writes`, deploy
    // second pass, `soroban_contracts.name`); full removal of that path is
    // deferred to task 0304.
    out.metadata_rows = build_metadata_rows(contract_metadata_writes);
    // A SAC first seen THIS ledger (its carrier flagged with `sac_contract_id` in
    // `assets`) isn't in the pre-fetched DB `asset_sac` map yet — it's written to
    // `asset_sac` during this same staging. Seed those current-ledger carriers so a
    // same-ledger contract-held balance re-keys onto the wrapped classic/native id
    // instead of orphaning on its surrogate. Guarded: the common ledger (no new SAC,
    // or no balances) skips the clone; the DB map wins on conflict (`or_insert`).
    let effective_sac_classic;
    let sac_map: &HashMap<i64, i64> = if !soroban_token_balances.is_empty()
        && assets.iter().any(|t| t.sac_contract_id.is_some())
    {
        let mut m = sac_classic.clone();
        for t in assets {
            if let Some(sac) = t.sac_contract_id.as_deref() {
                let issuer_id = t
                    .issuer_address
                    .as_deref()
                    .map(ids::account_id)
                    .unwrap_or(0);
                let classic = ids::asset_id(
                    t.asset_type as i16,
                    t.asset_code.as_deref().unwrap_or(""),
                    issuer_id,
                    0,
                );
                m.entry(ids::contract_id(sac)).or_insert(classic);
            }
        }
        effective_sac_classic = m;
        &effective_sac_classic
    } else {
        sac_classic
    };
    out.unified_balance_rows = build_balance_rows(soroban_token_balances, sac_map);

    // Task 0323 — un-deployed SACs are modelled as ASSETS, not contracts.
    // The `is_sac=true` skeleton `soroban_contracts` rows that task 0220 wrote
    // here are removed. `sac_overrides` (now the crypto-proven event emitters
    // from `detect_undeployed_sac_overrides`) instead (a) suppress the Pass-2 FK
    // stub below so no contract row is written, and (b) seed a SAC `assets` row
    // in the asset-emission pass. A real deploy still writes its contract row
    // from `contract_deployments` (site above).

    // ---- transactions + transaction_hash_index ----
    let mut tx_id_by_hash: HashMap<String, i64> = HashMap::with_capacity(transactions.len());
    for (idx, tx) in transactions.iter().enumerate() {
        let hash = decode_hash(&tx.hash, "tx.hash")?;
        let tx_id = ids::transaction_id(&hash);
        tx_id_by_hash.insert(tx.hash.clone(), tx_id);

        let inner_tx_hash = match tx.inner_tx_hash.as_deref() {
            Some(h) => Some(decode_hash(h, "inner_tx_hash")?),
            None => None,
        };
        let app_order =
            i16::try_from(idx + 1).map_err(|_| staging_err("application_order overflow (>i16)"))?;
        let op_count = op_count_by_tx.get(tx.hash.as_str()).copied().unwrap_or(0);

        out.transaction_rows.push(TransactionRow {
            id: tx_id,
            hash,
            ledger_sequence: ledger_sequence_i64,
            application_order: app_order,
            source_id: ids::account_id(&tx.source_account),
            fee_charged: tx.fee_charged,
            inner_tx_hash,
            successful: tx.successful,
            operation_count: op_count,
            has_soroban: *has_soroban.get(&tx.hash).unwrap_or(&false),
            parse_error: tx.parse_error,
        });

        out.hash_index_rows.push(TransactionHashIndexRow {
            hash,
            ledger_sequence: ledger_sequence_i64,
        });
    }

    // ---- transaction_participants ----
    for tx in transactions {
        let Some(set) = participants_per_tx.get(&tx.hash) else {
            continue;
        };
        let Some(&tx_id) = tx_id_by_hash.get(&tx.hash) else {
            continue;
        };
        for key in set {
            if !is_strkey_account(key) {
                continue;
            }
            out.participant_rows.push(TransactionParticipantRow {
                account_id: ids::account_id(key),
                ledger_sequence: ledger_sequence_i64,
                transaction_id: tx_id,
            });
        }
    }

    // ---- liquidity_pools (dedup by pool_id, latest watermark) ----
    let mut pool_indices: HashMap<[u8; 32], usize> = HashMap::new();
    for pool in liquidity_pools {
        let pool_id = decode_hash(&pool.pool_id, "pool_id")?;
        let (Some((a_type, a_code, a_issuer)), Some((b_type, b_code, b_issuer))) = (
            split_pool_asset(&pool.asset_a),
            split_pool_asset(&pool.asset_b),
        ) else {
            continue;
        };
        let last_updated_ledger = i64::from(pool.last_updated_ledger);
        let new_row = LiquidityPoolRow {
            pool_id,
            asset_a_type: a_type as i16,
            asset_a_code: a_code.unwrap_or_default(),
            asset_a_issuer_id: a_issuer.as_deref().map(ids::account_id).unwrap_or(0),
            asset_b_type: b_type as i16,
            asset_b_code: b_code.unwrap_or_default(),
            asset_b_issuer_id: b_issuer.as_deref().map(ids::account_id).unwrap_or(0),
            fee_bps: pool.fee_bps,
            last_updated_ledger,
        };
        match pool_indices.get(&pool_id).copied() {
            Some(idx) => {
                let existing = &mut out.pool_rows[idx];
                if last_updated_ledger >= existing.last_updated_ledger {
                    existing.last_updated_ledger = last_updated_ledger;
                }
            }
            None => {
                pool_indices.insert(pool_id, out.pool_rows.len());
                out.pool_rows.push(new_row);
            }
        }
    }

    // ---- liquidity_pool_snapshots ----
    // Per-(pool, ledger) asset-A trade volume from claim atoms (0261 extractor).
    // Live ingest now derives it directly (previously backfill-only); the 0266
    // worker reuses this same value via `prepare`, so live + backfill agree.
    let gross_volume_by_pool = gross_volume_a_by_pool(operations);
    for snap in pool_snapshots {
        let pool_id = decode_hash(&snap.pool_id, "snapshot.pool_id")?;
        let reserve_a = snap
            .reserves
            .get("a")
            .and_then(Value::as_i64)
            .map(i128::from)
            .unwrap_or(0);
        let reserve_b = snap
            .reserves
            .get("b")
            .and_then(Value::as_i64)
            .map(i128::from)
            .unwrap_or(0);
        out.snapshot_rows.push(LiquidityPoolSnapshotRow {
            pool_id,
            ledger_sequence: i64::from(snap.ledger_sequence),
            reserve_a,
            reserve_b,
            total_shares: decimal7_string_to_i128(&snap.total_shares)?,
            tvl: snap
                .tvl
                .as_deref()
                .map(decimal7_string_to_i128)
                .transpose()?,
            volume: snap
                .volume
                .as_deref()
                .map(decimal7_string_to_i128)
                .transpose()?,
            fee_revenue: snap
                .fee_revenue
                .as_deref()
                .map(decimal7_string_to_i128)
                .transpose()?,
            // Asset-A-side trade volume for this (pool, ledger) from claim atoms
            // (0261). `None` when the pool had no trade this ledger. USD volume/
            // fee_revenue remain read-time (ADR 0048); those columns stay NULL.
            gross_volume_a: gross_volume_by_pool.get(&pool_id).copied(),
        });
    }

    // ---- lp_positions (dedup by (pool_id, account_id)) ----
    use std::collections::hash_map::Entry;
    let mut lp_dedup: HashMap<([u8; 32], i64), LpPositionRow> = HashMap::new();
    for pos in lp_positions {
        let pool_id = decode_hash(&pos.pool_id, "lp_position.pool_id")?;
        let acct_id = ids::account_id(&pos.account_id);
        let last = i64::from(pos.last_updated_ledger);
        let first = pos.first_deposit_ledger.map(i64::from).unwrap_or(last);
        let new_row = LpPositionRow {
            pool_id,
            account_id: acct_id,
            shares: decimal7_string_to_i128(&pos.shares)?,
            first_deposit_ledger: first,
            last_updated_ledger: last,
        };
        match lp_dedup.entry((pool_id, acct_id)) {
            Entry::Occupied(mut occ) => {
                let existing = occ.get_mut();
                if new_row.last_updated_ledger >= existing.last_updated_ledger {
                    let preserved_first = existing
                        .first_deposit_ledger
                        .min(new_row.first_deposit_ledger);
                    *existing = new_row;
                    existing.first_deposit_ledger = preserved_first;
                } else {
                    existing.first_deposit_ledger = existing
                        .first_deposit_ledger
                        .min(new_row.first_deposit_ledger);
                }
            }
            Entry::Vacant(vac) => {
                vac.insert(new_row);
            }
        }
    }
    out.lp_position_rows.extend(lp_dedup.into_values());

    // ---- operations_appearances (identity fold per task 0163) ----
    #[derive(Eq, PartialEq, Hash)]
    struct OpKey {
        tx_hash_hex: String,
        op_type: i16,
        source_account: Option<String>,
        destination_account: Option<String>,
        contract_strkey: Option<String>,
        asset_code: String,
        asset_issuer_account: Option<String>,
        /// Sorted + deduped — canonical order makes the fold identity (and
        /// the emitted row) deterministic across re-parses (task 0261/0266).
        pool_ids: Vec<[u8; 32]>,
    }
    struct OpAgg {
        count: i64,
        min_apply_order: u32,
    }
    let mut op_agg: HashMap<OpKey, OpAgg> = HashMap::new();
    for (tx_hash, ops) in operations {
        if !tx_id_by_hash.contains_key(tx_hash) {
            continue;
        }
        for op in ops {
            let typed = OpTyped::from_details(op.op_type, &op.details);
            let mut pool_ids = Vec::with_capacity(typed.pool_ids_hex.len());
            for h in &typed.pool_ids_hex {
                pool_ids.push(decode_hash(h, "op.pool_ids")?);
            }
            pool_ids.sort_unstable();
            pool_ids.dedup();
            let key = OpKey {
                tx_hash_hex: tx_hash.clone(),
                op_type: op.op_type as i16,
                source_account: op.source_account.clone(),
                destination_account: typed.destination,
                contract_strkey: typed.contract_id,
                asset_code: typed.asset_code.unwrap_or_default(),
                asset_issuer_account: typed.asset_issuer,
                pool_ids,
            };
            op_agg
                .entry(key)
                .and_modify(|agg| {
                    agg.count += 1;
                    agg.min_apply_order = agg.min_apply_order.min(op.operation_index);
                })
                .or_insert(OpAgg {
                    count: 1,
                    min_apply_order: op.operation_index,
                });
        }
    }
    for (k, agg) in op_agg {
        let Some(&tx_id) = tx_id_by_hash.get(&k.tx_hash_hex) else {
            continue;
        };
        let app_order = i16::try_from(agg.min_apply_order)
            .map_err(|_| staging_err("operation_index >i16 — protocol violation"))?;
        out.op_rows.push(OperationAppearanceRow {
            transaction_id: tx_id,
            application_order: app_order,
            op_type: k.op_type,
            source_id: k.source_account.as_deref().map(ids::account_id),
            destination_id: k.destination_account.as_deref().map(ids::account_id),
            contract_id: k.contract_strkey.as_deref().map(ids::contract_id),
            asset_code: k.asset_code,
            asset_issuer_id: k.asset_issuer_account.as_deref().map(ids::account_id),
            pool_ids: k.pool_ids,
            amount: agg.count,
            ledger_sequence: ledger_sequence_i64,
        });
    }

    // ---- soroban_events (UNFOLDED per ADR 0044 §4a) ----
    let mut diagnostic_dropped: usize = 0;
    let mut contract_orphan_dropped: usize = 0;
    for (tx_hash, evs) in events {
        let Some(&tx_id) = tx_id_by_hash.get(tx_hash) else {
            continue;
        };
        for ev in evs {
            if is_diagnostic(ev.source) {
                diagnostic_dropped += 1;
                continue;
            }
            let Some(contract_strkey) = &ev.contract_id else {
                contract_orphan_dropped += 1;
                continue;
            };
            let event_index = i16::try_from(ev.event_index)
                .map_err(|_| staging_err("event_index overflow (>i16)"))?;
            let topics_xdr = serde_json::to_string(&ev.topics)
                .map_err(|e| staging_err(&format!("event topics serialize: {e}")))?;
            let data_xdr = serde_json::to_string(&ev.data)
                .map_err(|e| staging_err(&format!("event data serialize: {e}")))?;
            let signature = extract_event_signature(&ev.topics);
            out.event_rows.push(SorobanEventRow {
                contract_id: ids::contract_id(contract_strkey),
                transaction_id: tx_id,
                ledger_sequence: ledger_sequence_i64,
                event_index,
                event_type: ev.event_type as i16,
                signature,
                topics_xdr,
                data_xdr,
            });
        }
    }
    if diagnostic_dropped > 0 || contract_orphan_dropped > 0 {
        tracing::debug!(
            ledger_sequence = ledger.sequence,
            diagnostic_dropped,
            contract_orphan_dropped,
            staged = out.event_rows.len(),
            "CH soroban_events filtered"
        );
    }

    // ---- soroban_invocations_appearances (ADR 0034 fold) ----
    #[derive(Eq, PartialEq, Hash)]
    struct InvKey {
        contract_strkey: String,
        tx_hash_hex: String,
    }
    struct InvAgg {
        amount: i32,
        caller_account: Option<String>,
        caller_contract_strkey: Option<String>,
    }
    let mut inv_agg: HashMap<InvKey, InvAgg> = HashMap::new();
    for (tx_hash, invs) in invocations {
        if !tx_id_by_hash.contains_key(tx_hash) {
            continue;
        }
        for inv in invs {
            let Some(contract) = &inv.contract_id else {
                continue;
            };
            let (caller_account, caller_contract) = match inv.caller_account.as_deref() {
                Some(k) if is_strkey_account(k) => (Some(k.to_string()), None),
                Some(k) if k.starts_with('C') => (None, Some(k.to_string())),
                _ => (None, None),
            };
            let key = InvKey {
                contract_strkey: contract.clone(),
                tx_hash_hex: tx_hash.clone(),
            };
            inv_agg
                .entry(key)
                .and_modify(|agg| {
                    agg.amount = agg.amount.saturating_add(1);
                    if agg.caller_account.is_none() && agg.caller_contract_strkey.is_none() {
                        agg.caller_account = caller_account.clone();
                        agg.caller_contract_strkey = caller_contract.clone();
                    }
                })
                .or_insert(InvAgg {
                    amount: 1,
                    caller_account,
                    caller_contract_strkey: caller_contract,
                });
        }
    }
    for (k, agg) in inv_agg {
        let Some(&tx_id) = tx_id_by_hash.get(&k.tx_hash_hex) else {
            continue;
        };
        out.invocation_rows.push(SorobanInvocationAppearanceRow {
            contract_id: ids::contract_id(&k.contract_strkey),
            transaction_id: tx_id,
            ledger_sequence: ledger_sequence_i64,
            caller_id: agg.caller_account.as_deref().map(ids::account_id),
            caller_contract_id: agg.caller_contract_strkey.as_deref().map(ids::contract_id),
            amount: agg.amount,
        });
    }

    // ---- assets identity rows (dedup by 4-tuple) + asset_sac facet rows ----
    //
    // ADR 0051: the classic_credit / native `assets` row is the asset's identity;
    // the SAC handle is a FACET written to the `asset_sac` side table — NOT a
    // column on `assets`, which is a no-version RMT re-written whole every ledger
    // a trustline for the asset changes (`detect_classic_credit_assets`) and would
    // clobber a mutable non-key column to its default on the next re-emit (the
    // ~25% NULL prod bug). `push_asset` dedups identity by the 4-tuple; `push_sac`
    // collects one facet per key, `max`-merging `sac_deployed` (a deploy sticks
    // over a later un-deployed override) — mirrored by the `asset_sac`
    // AggregatingMergeTree so the fold is correct cross-ledger too, not just
    // within one staged batch.
    let mut asset_seen: HashSet<(i16, String, i64, i64)> = HashSet::new();
    let push_asset =
        |out: &mut StagedLedger, seen: &mut HashSet<(i16, String, i64, i64)>, row: AssetRow| {
            // Dedup by the identity 4-tuple. `row.id` is already the `ids::asset_id`
            // surrogate (computed in `AssetRow::staged`), so build sites can't diverge.
            let key = (
                row.asset_type,
                row.asset_code.clone(),
                row.issuer_id,
                row.contract_id,
            );
            if seen.insert(key) {
                out.asset_rows.push(row);
            }
        };
    // Facet accumulator: (asset_type, code, issuer_id, contract_id=0) → (surrogate, deployed).
    let mut sac_facets: HashMap<(i16, String, i64), (i64, u8)> = HashMap::new();
    let mut push_sac = |asset_type: i16,
                        asset_code: String,
                        issuer_id: i64,
                        sac_contract_id: i64,
                        deployed: u8| {
        let e = sac_facets
            .entry((asset_type, asset_code, issuer_id))
            .or_insert((0, 0));
        if sac_contract_id != 0 {
            e.0 = sac_contract_id;
        }
        e.1 = e.1.max(deployed);
    };
    for t in assets {
        let issuer_id = t
            .issuer_address
            .as_deref()
            .map(ids::account_id)
            .unwrap_or(0);
        let contract_id_int = t.contract_id.as_deref().map(ids::contract_id).unwrap_or(0);
        let row = AssetRow::staged(
            t.asset_type as i16,
            t.asset_code.clone().unwrap_or_default(),
            issuer_id,
            contract_id_int,
            t.name.clone(),
        );
        // SAC facet carried by `detect_assets`' SAC branch (the classic/native
        // carrier for a deploy). Emitted to `asset_sac`, never onto `assets`.
        if let Some(sac) = t.sac_contract_id.as_deref() {
            push_sac(
                t.asset_type as i16,
                row.asset_code.clone(),
                issuer_id,
                ids::contract_id(sac),
                t.sac_deployed as u8,
            );
        }
        push_asset(&mut out, &mut asset_seen, row);
    }

    // SAC deploys (ADR 0051): ensure the underlying classic_credit / native
    // identity row exists and record the SAC facet (`sac_deployed = 1`). Never a
    // separate `type=2` row. `detect_assets` already emits the same carrier from
    // these deployments; the dedup collapses the two, and this also covers callers
    // that pass deployments without running `detect_assets`.
    for dep in contract_deployments {
        let Some(sac) = &dep.sac_asset else {
            continue;
        };
        let (asset_type, asset_code, issuer_id) = match sac {
            SacAssetIdentity::Native => (domain::TokenAssetType::Native, String::new(), 0),
            SacAssetIdentity::Credit { code, issuer } => (
                domain::TokenAssetType::ClassicCredit,
                code.clone(),
                ids::account_id(issuer),
            ),
        };
        push_sac(
            asset_type as i16,
            asset_code.clone(),
            issuer_id,
            ids::contract_id(&dep.contract_id),
            1,
        );
        push_asset(
            &mut out,
            &mut asset_seen,
            AssetRow::staged(
                asset_type as i16,
                asset_code,
                issuer_id,
                0, // key reserved for soroban identity
                None,
            ),
        );
    }

    // Un-deployed-SAC facet (task 0323 AC#3 → ADR 0051). The crypto-proven event
    // emitters in `sac_overrides` (see `detect_undeployed_sac_overrides`) get NO
    // contract row (suppressed in Pass-2 below); fold the SAC handle onto the
    // underlying classic_credit / native row with `sac_deployed = false` so its
    // activity has a home while it stays a reserved-but-un-deployed address.
    // `ov.identity` carries the classic asset. A deployed SAC (e.g. USDC) that
    // also emits is harmless — `push_asset`'s merge keeps `sac_deployed = true`
    // from the real deploy row (OR-merge never downgrades it).
    for ov in sac_overrides {
        let (asset_type, asset_code, issuer_id) = match &ov.identity {
            SacAssetIdentity::Native => (domain::TokenAssetType::Native, String::new(), 0),
            SacAssetIdentity::Credit { code, issuer } => (
                domain::TokenAssetType::ClassicCredit,
                code.clone(),
                ids::account_id(issuer),
            ),
        };
        push_sac(
            asset_type as i16,
            asset_code.clone(),
            issuer_id,
            ids::contract_id(&ov.contract_id),
            0,
        );
        push_asset(
            &mut out,
            &mut asset_seen,
            AssetRow::staged(
                asset_type as i16,
                asset_code,
                issuer_id,
                0, // key reserved for soroban identity
                None,
            ),
        );
    }

    // Native XLM singleton (PG sqlx migration 20260428000000 analog).
    push_asset(
        &mut out,
        &mut asset_seen,
        AssetRow::staged(
            domain::TokenAssetType::Native as i16,
            String::new(),
            0,
            0,
            Some("Stellar Lumen".to_string()),
        ),
    );

    // ---- assets type-3 for WASM-classified Soroban fungibles (task 0283 G2) --
    //
    // Mirror of PG `insert_assets_from_reclassified_contracts`: a contract
    // whose verdict resolved to `Fungible` (same-ledger classification or the
    // writer's prior-ledger prefetch via the deploy override above) gets a
    // bespoke-Soroban (`asset_type = 3`) asset row carrying only the
    // `contract_id` — code/issuer are empty, aggregates are filled later by the
    // `asset-aggregates` batch. `push_asset` dedups against any row the parser
    // already emitted same-batch; SAC short-circuits `Fungible`, so these never
    // collide with a SAC-carrying classic/native row. Read from the staged
    // `contract_rows` so the corrected verdict (incl. the prior-ledger override)
    // is honoured.
    let fungible_contract_ids: Vec<i64> = out
        .contract_rows
        .iter()
        .filter(|r| !r.is_sac && r.contract_type == Some(ContractType::Fungible as i16))
        .map(|r| r.id)
        .collect();
    for contract_id in fungible_contract_ids {
        push_asset(
            &mut out,
            &mut asset_seen,
            AssetRow::staged(
                domain::TokenAssetType::Soroban as i16,
                String::new(),
                0,
                contract_id,
                None,
            ),
        );
    }

    // Materialise the accumulated SAC facets into `asset_sac` rows (contract_id
    // is 0 — the classic/native carrier's key). AggregatingMergeTree `max`-merges
    // these with any prior-ledger rows, so `sac_deployed` is monotonic.
    for ((asset_type, asset_code, issuer_id), (sac_contract_id, sac_deployed)) in sac_facets {
        out.asset_sac_rows.push(AssetSacRow {
            asset_type,
            asset_code,
            issuer_id,
            contract_id: 0,
            sac_contract_id,
            sac_deployed,
        });
    }

    // ---- NFT routing verdict map (task 0217 / 0220) -------------------
    //
    // Build a per-contract verdict map keyed by strkey. Sources, in
    // precedence order:
    //   1. Same-ledger `contract_rows` carrying a definitive
    //      `contract_type` (Token / Nft / Fungible). Either:
    //        - SAC deploy (`is_sac=true` → Token).
    //        - WASM-classified deploy (the override applied above).
    //   2. SAC overrides (also Token) — these were skipped from Pass-2
    //      stubs, so they're in `out.contract_rows` already.
    // Contracts with NO entry in EITHER source → treat as `Other`/uncached →
    // route to pending. The stage itself has no DB access; cross-ledger
    // verdicts arrive via `prior_contract_verdicts` (task 0283 live G9), the
    // writer's lookup of `soroban_contracts` for contracts emitting NFT
    // rows/events here but deployed earlier. This restores the PG
    // `ClassificationCache` semantic the CH cutover dropped — without it a
    // later transfer from an already-classified NFT would quarantine.
    let mut verdict_by_contract: HashMap<&str, ContractType> = HashMap::new();
    for row in &out.contract_rows {
        if let Some(ty_i16) = row.contract_type
            && let Ok(ty) = ContractType::try_from(ty_i16)
        {
            verdict_by_contract.insert(row.contract_id.as_str(), ty);
        }
    }

    // 3-way routing helper. Mirrors PG `resolve_nft_filter` bucketing.
    // This-ledger `contract_rows` take precedence; `prior_contract_verdicts`
    // (G9, cross-ledger) is the fallback for contracts not deployed here.
    enum NftRoute {
        Hot,
        Pending,
        Drop,
    }
    let route_for = |strkey: &str| -> NftRoute {
        let verdict = verdict_by_contract
            .get(strkey)
            .copied()
            .or_else(|| prior_contract_verdicts.get(strkey).copied());
        match verdict {
            Some(ContractType::Token) | Some(ContractType::Fungible) => NftRoute::Drop,
            Some(ContractType::Nft) => NftRoute::Hot,
            // `Other` and uncached (no entry in either source) both go to
            // quarantine — same semantic as PG-side `resolve_nft_filter`.
            _ => NftRoute::Pending,
        }
    };

    // ---- nfts / nfts_pending (dedup by (contract_id, token_id),
    //                            latest watermark) ----
    //
    // Each `(contract_id, token_id)` row lives in exactly one bucket
    // (hot OR pending) per partition — picked by the per-contract
    // verdict above. Dedup keys are per-bucket so a contract that
    // somehow appeared with mixed verdicts within the same ledger
    // (impossible today, defensive) would have separate slots.
    let mut nft_hot_indices: HashMap<(i64, String), usize> = HashMap::new();
    let mut nft_pending_indices: HashMap<(i64, String), usize> = HashMap::new();
    for nft in nfts {
        let route = route_for(nft.contract_id.as_str());
        if matches!(route, NftRoute::Drop) {
            continue;
        }
        let contract_id_int = ids::contract_id(&nft.contract_id);
        let watermark = i64::from(nft.last_seen_ledger);
        let key = (contract_id_int, nft.token_id.clone());
        let owner_id = nft.owner_account.as_deref().map(ids::account_id);
        let minted = nft.minted_at_ledger.map(i64::from);

        match route {
            NftRoute::Hot => match nft_hot_indices.get(&key).copied() {
                Some(idx) => {
                    let existing = &mut out.nft_rows[idx];
                    if watermark >= existing.current_owner_ledger {
                        existing.current_owner_id = owner_id;
                        existing.current_owner_ledger = watermark;
                    }
                    existing.minted_at_ledger = match (existing.minted_at_ledger, minted) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    existing.collection_name = existing
                        .collection_name
                        .clone()
                        .or_else(|| nft.collection_name.clone());
                    existing.name = existing.name.clone().or_else(|| nft.name.clone());
                    existing.media_url =
                        existing.media_url.clone().or_else(|| nft.media_url.clone());
                }
                None => {
                    nft_hot_indices.insert(key, out.nft_rows.len());
                    out.nft_rows.push(NftRow {
                        contract_id: contract_id_int,
                        token_id: nft.token_id.clone(),
                        collection_name: nft.collection_name.clone(),
                        name: nft.name.clone(),
                        media_url: nft.media_url.clone(),
                        minted_at_ledger: minted,
                        current_owner_id: owner_id,
                        current_owner_ledger: watermark,
                    });
                }
            },
            NftRoute::Pending => match nft_pending_indices.get(&key).copied() {
                Some(idx) => {
                    let existing = &mut out.nft_pending_rows[idx];
                    if watermark >= existing.current_owner_ledger {
                        existing.current_owner_id = owner_id;
                        existing.current_owner_ledger = watermark;
                    }
                    existing.minted_at_ledger = match (existing.minted_at_ledger, minted) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                    existing.collection_name = existing
                        .collection_name
                        .clone()
                        .or_else(|| nft.collection_name.clone());
                    existing.name = existing.name.clone().or_else(|| nft.name.clone());
                    existing.media_url =
                        existing.media_url.clone().or_else(|| nft.media_url.clone());
                }
                None => {
                    nft_pending_indices.insert(key, out.nft_pending_rows.len());
                    out.nft_pending_rows.push(NftPendingRow {
                        contract_id: contract_id_int,
                        token_id: nft.token_id.clone(),
                        collection_name: nft.collection_name.clone(),
                        name: nft.name.clone(),
                        media_url: nft.media_url.clone(),
                        minted_at_ledger: minted,
                        current_owner_id: owner_id,
                        current_owner_ledger: watermark,
                    });
                }
            },
            NftRoute::Drop => unreachable!("filtered above"),
        }
    }

    // ---- nft_ownership / nft_ownership_pending ----
    for ev in nft_events {
        let route = route_for(ev.contract_id.as_str());
        if matches!(route, NftRoute::Drop) {
            continue;
        }
        let Some(&tx_id) = tx_id_by_hash.get(&ev.transaction_hash) else {
            continue;
        };
        let event_order =
            i16::try_from(ev.event_order).map_err(|_| staging_err("nft event_order overflow"))?;
        let contract_id = ids::contract_id(&ev.contract_id);
        let ledger_sequence = i64::from(ev.ledger_sequence);
        let owner_id = ev.owner_account.as_deref().map(ids::account_id);
        let event_type = ev.event_type as i16;

        match route {
            NftRoute::Hot => out.nft_ownership_rows.push(NftOwnershipRow {
                contract_id,
                token_id: ev.token_id.clone(),
                ledger_sequence,
                event_order,
                transaction_id: tx_id,
                owner_id,
                event_type,
            }),
            NftRoute::Pending => out.nft_ownership_pending_rows.push(NftOwnershipPendingRow {
                contract_id,
                token_id: ev.token_id.clone(),
                ledger_sequence,
                event_order,
                transaction_id: tx_id,
                owner_id,
                event_type,
            }),
            NftRoute::Drop => unreachable!("filtered above"),
        }
    }

    // ---- unified `balances` — classic + native per-account balances (lore-0331
    // Option A single-write). `account_balances_current` is no longer written (only
    // its table remains, for the pending classic→`balances` migration + rollback).
    // Dedup straight on (holder_id, asset_id): the project `asset_id` already folds
    // the Horizon alphanum4/12 split into one `classic-credit` key. amount is the
    // same scaled-i128 (decimals=7 at read); accounts resolve via `accounts`.
    let mut balance_dedup: HashMap<(i64, i64), BalanceRow> = HashMap::new();
    for st in account_states {
        let watermark = i64::from(st.last_seen_ledger);
        let account_id_int = ids::account_id(&st.account_id);
        for b in st.balances.as_array().into_iter().flatten() {
            let Some(asset_type) = b
                .get("asset_type")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<AssetType>().ok())
            else {
                continue;
            };
            let balance_str = b.get("balance").and_then(Value::as_str).unwrap_or("0");
            let amount = decimal7_string_to_i128(balance_str)?;
            let asset_id = if asset_type == AssetType::Native {
                ids::asset_id(0, "", 0, 0)
            } else {
                let code = b.get("asset_code").and_then(Value::as_str).unwrap_or("");
                let issuer = b.get("issuer").and_then(Value::as_str).unwrap_or("");
                if code.is_empty() || issuer.is_empty() {
                    continue;
                }
                ids::asset_id(1, code, ids::account_id(issuer), 0)
            };
            upsert_balance(
                &mut balance_dedup,
                account_id_int,
                asset_id,
                amount,
                watermark,
            );
        }

        for rm in &st.removed_trustlines {
            let code = rm.get("asset_code").and_then(Value::as_str).unwrap_or("");
            let issuer = rm.get("issuer").and_then(Value::as_str).unwrap_or("");
            if code.is_empty() || issuer.is_empty() {
                continue;
            }
            let asset_id = ids::asset_id(1, code, ids::account_id(issuer), 0);
            upsert_balance(&mut balance_dedup, account_id_int, asset_id, 0, watermark);
        }
    }
    out.unified_balance_rows.extend(balance_dedup.into_values());

    // ---- soroban_contracts Pass 2 stub-rowing ----
    {
        let mut emitted: HashSet<&str> = HashSet::new();
        for cid in &contract_seen {
            emitted.insert(cid.as_str());
        }
        // Task 0323 — suppress the Pass-2 FK stub for SAC-override contracts.
        // `sac_overrides` are crypto-proven un-deployed SACs (modelled as
        // ASSETS, not contracts) plus deployed SACs that emit this ledger
        // (already carrying a real deploy row). Neither should get an
        // `is_sac=false` stub: un-deployed SACs get an `assets` row instead,
        // and for a deployed SAC a stub at `wasm_uploaded_at_ledger=0` could
        // clobber its real deploy row on the equal-version RMT merge.
        for ov in sac_overrides {
            emitted.insert(ov.contract_id.as_str());
        }

        let mut referenced: HashSet<String> = HashSet::new();
        for (_, ops) in operations {
            for op in ops {
                if let Some(c) = OpTyped::from_details(op.op_type, &op.details).contract_id {
                    referenced.insert(c);
                }
            }
        }
        for (_, evs) in events {
            for ev in evs {
                if is_diagnostic(ev.source) {
                    continue;
                }
                if let Some(c) = &ev.contract_id {
                    referenced.insert(c.clone());
                }
            }
        }
        for (_, invs) in invocations {
            for inv in invs {
                if let Some(c) = &inv.contract_id {
                    referenced.insert(c.clone());
                }
                if let Some(caller) = &inv.caller_account
                    && caller.starts_with('C')
                {
                    referenced.insert(caller.clone());
                }
            }
        }
        for a in assets {
            if let Some(c) = &a.contract_id {
                referenced.insert(c.clone());
            }
        }
        for n in nfts {
            referenced.insert(n.contract_id.clone());
        }
        for ev in nft_events {
            referenced.insert(ev.contract_id.clone());
        }

        for cid in &referenced {
            if emitted.contains(cid.as_str()) {
                continue;
            }
            out.contract_rows.push(SorobanContractRow {
                id: ids::contract_id(cid),
                contract_id: cid.clone(),
                wasm_hash: None,
                wasm_uploaded_at_ledger: 0,
                deployer_id: None,
                deployed_at_ledger: None,
                contract_type: None,
                is_sac: false,
                name: None,
            });
        }
    }

    // ---- Task 0320 live path: WASM-upgrade row rewrites ----
    // Appended last (after deploy / SAC-override / skeleton rows) so a contract
    // upgraded this ledger gets a full row overriding `wasm_hash` +
    // `wasm_uploaded_at_ledger` (RMT version = upgrade ledger, wins the merge)
    // while carrying its identity forward. `prior_contract_rows` is empty for
    // every caller except the live indexer, so this is a no-op elsewhere. A
    // contract that both deploys and upgrades in one ledger is impossible (the
    // WASM upload must precede the upgrade in an earlier tx).
    out.contract_rows.extend(build_wasm_upgrade_rows(
        events,
        prior_contract_rows,
        ledger_sequence_i64,
    ));

    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn decode_hash(hex_str: &str, field: &'static str) -> Result<[u8; 32], SchemaError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| staging_err(&format!("hex decode {field}: {e} (value={hex_str})")))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        staging_err(&format!(
            "hex length {field}: expected 32 bytes, got {} (value={hex_str})",
            bytes.len()
        ))
    })
}

fn staging_err(msg: &str) -> SchemaError {
    SchemaError::Staging(msg.to_string())
}

/// Upsert a `BalanceRow` into the per-`(holder_id, asset_id)` dedup map, keeping
/// the newest `last_updated_ledger` (RMT version semantics resolved at stage time).
fn upsert_balance(
    map: &mut HashMap<(i64, i64), BalanceRow>,
    holder_id: i64,
    asset_id: i64,
    amount: i128,
    last_updated_ledger: i64,
) {
    use std::collections::hash_map::Entry;
    let row = BalanceRow {
        holder_id,
        asset_id,
        amount,
        last_updated_ledger,
    };
    match map.entry((holder_id, asset_id)) {
        Entry::Occupied(mut occ) => {
            if row.last_updated_ledger >= occ.get().last_updated_ledger {
                *occ.get_mut() = row;
            }
        }
        Entry::Vacant(vac) => {
            vac.insert(row);
        }
    }
}

fn decimal7_string_to_i128(s: &str) -> Result<i128, SchemaError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1i128, r),
        None => (1i128, s.strip_prefix('+').unwrap_or(s)),
    };
    let unsigned: i128 = if let Some((whole, frac)) = rest.split_once('.') {
        let whole_n: i128 = if whole.is_empty() {
            0
        } else {
            whole
                .parse()
                .map_err(|e| staging_err(&format!("decimal parse '{s}': {e}")))?
        };
        let frac_trim = &frac[..frac.len().min(7)];
        let frac_n: i128 = if frac_trim.is_empty() {
            0
        } else {
            frac_trim
                .parse()
                .map_err(|e| staging_err(&format!("decimal frac parse '{s}': {e}")))?
        };
        let scale = 10i128.pow((7 - frac_trim.len()) as u32);
        whole_n
            .checked_mul(10_000_000)
            .and_then(|w| w.checked_add(frac_n * scale))
            .ok_or_else(|| staging_err(&format!("decimal overflow '{s}'")))?
    } else {
        rest.parse()
            .map_err(|e| staging_err(&format!("integer parse '{s}': {e}")))?
    };
    Ok(sign * unsigned)
}

fn is_strkey_account(s: &str) -> bool {
    s.len() <= 56 && s.starts_with('G')
}

fn is_diagnostic(src: EventSource) -> bool {
    matches!(src, EventSource::Diagnostic)
}

fn extract_event_signature(topics: &Value) -> Option<String> {
    let first = topics.as_array()?.first()?.as_object()?;
    if first.get("type").and_then(Value::as_str)? != "sym" {
        return None;
    }
    first
        .get("value")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn tx_has_soroban_map(operations: &[(String, Vec<ExtractedOperation>)]) -> HashMap<String, bool> {
    operations
        .iter()
        .map(|(tx_hash, ops)| {
            let has = ops.iter().any(|op| {
                matches!(
                    op.op_type,
                    OperationType::InvokeHostFunction
                        | OperationType::ExtendFootprintTtl
                        | OperationType::RestoreFootprint
                )
            });
            (tx_hash.clone(), has)
        })
        .collect()
}

struct OpTyped {
    destination: Option<String>,
    contract_id: Option<String>,
    asset_code: Option<String>,
    asset_issuer: Option<String>,
    /// Liquidity pools touched by the op. Single-element for LP
    /// deposit/withdraw; the full crossed-pool list (from result claim
    /// atoms) for path payments; empty otherwise. Task 0261 / 0268.
    pool_ids_hex: Vec<String>,
}

impl OpTyped {
    fn from_details(op_type: OperationType, details: &Value) -> Self {
        let mut out = Self {
            destination: None,
            contract_id: None,
            asset_code: None,
            asset_issuer: None,
            pool_ids_hex: Vec::new(),
        };
        match op_type {
            OperationType::CreateAccount => {
                out.destination = str_field(details, "destination");
            }
            OperationType::Payment => {
                out.destination = str_field(details, "destination");
                if let Some(asset) = details.get("asset") {
                    let (c, i) = split_asset_ref(asset);
                    out.asset_code = c;
                    out.asset_issuer = i;
                }
            }
            OperationType::PathPaymentStrictReceive | OperationType::PathPaymentStrictSend => {
                out.destination = str_field(details, "destination");
                if let Some(asset) = details.get("destAsset") {
                    let (c, i) = split_asset_ref(asset);
                    out.asset_code = c;
                    out.asset_issuer = i;
                }
            }
            OperationType::AccountMerge => {
                out.destination = str_field(details, "destination");
            }
            OperationType::Clawback => {
                out.destination = str_field(details, "from");
                if let Some(asset) = details.get("asset") {
                    let (c, i) = split_asset_ref(asset);
                    out.asset_code = c;
                    out.asset_issuer = i;
                }
            }
            OperationType::LiquidityPoolDeposit | OperationType::LiquidityPoolWithdraw => {
                out.pool_ids_hex = str_field(details, "liquidityPoolId").into_iter().collect();
            }
            OperationType::InvokeHostFunction => {
                out.contract_id = str_field(details, "contractId");
            }
            OperationType::ChangeTrust => {
                if let Some(asset) = details.get("asset") {
                    let (c, i) = split_asset_ref(asset);
                    out.asset_code = c;
                    out.asset_issuer = i;
                }
            }
            OperationType::SetTrustLineFlags => {
                out.destination = str_field(details, "trustor");
                if let Some(asset) = details.get("asset") {
                    let (c, i) = split_asset_ref(asset);
                    out.asset_code = c;
                    out.asset_issuer = i;
                }
            }
            OperationType::AllowTrust => {
                out.destination = str_field(details, "trustor");
                if let Some(asset) = details.get("asset")
                    && let Some(code) = asset.as_str()
                {
                    out.asset_code = Some(code.to_string());
                }
            }
            OperationType::BeginSponsoringFutureReserves => {
                out.destination = str_field(details, "sponsoredId");
            }
            _ => {}
        }
        // `poolIds` (path payments + offers crossing an LP — task 0261) is
        // present on any op whose result carried claim atoms, regardless of op
        // type. LP deposit/withdraw already set `pool_ids_hex` from
        // `liquidityPoolId` above, so the guard skips them.
        if out.pool_ids_hex.is_empty()
            && let Some(ids) = details.get("poolIds").and_then(Value::as_array)
        {
            out.pool_ids_hex = ids
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        out
    }
}

fn str_field(obj: &Value, field: &str) -> Option<String> {
    obj.get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn split_asset_ref(asset: &Value) -> (Option<String>, Option<String>) {
    let Some(s) = asset.as_str() else {
        return (None, None);
    };
    if s == "native" {
        return (None, None);
    }
    match s.split_once(':') {
        Some((code, issuer)) if !code.is_empty() && !issuer.is_empty() => {
            (Some(code.to_string()), Some(issuer.to_string()))
        }
        _ => (None, None),
    }
}

fn asset_issuer(asset: &Value) -> Option<String> {
    asset
        .as_object()?
        .get("issuer")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn split_pool_asset(asset: &Value) -> Option<(AssetType, Option<String>, Option<String>)> {
    if let Some(s) = asset.as_str()
        && s == "native"
    {
        return Some((AssetType::Native, None, None));
    }
    let obj = asset.as_object()?;
    let ty = obj.get("type").and_then(Value::as_str)?.parse().ok()?;
    let code = obj
        .get("code")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let issuer = obj
        .get("issuer")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((ty, code, issuer))
}

fn op_participant_str_keys(op_type: OperationType, details: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |v: Option<String>| {
        if let Some(s) = v
            && is_strkey_account(&s)
        {
            out.push(s);
        }
    };
    use OperationType as Op;
    match op_type {
        Op::CreateAccount
        | Op::Payment
        | Op::PathPaymentStrictReceive
        | Op::PathPaymentStrictSend
        | Op::AccountMerge => {
            push(str_field(details, "destination"));
        }
        Op::Clawback => {
            push(str_field(details, "from"));
        }
        Op::AllowTrust | Op::SetTrustLineFlags => {
            push(str_field(details, "trustor"));
        }
        Op::BeginSponsoringFutureReserves => {
            push(str_field(details, "sponsoredId"));
        }
        _ => {}
    }
    for field in ["asset", "destAsset", "sendAsset"] {
        if let Some(asset) = details.get(field) {
            let (_, issuer) = split_asset_ref(asset);
            push(issuer);
        }
    }
    out
}

#[derive(Debug, Default)]
struct AccountOverride {
    sequence_number: i64,
    sequence_set: bool,
    first_seen_ledger: Option<i64>,
    home_domain: Option<String>,
    last_seen_ledger: Option<i64>,
}

impl AccountOverride {
    fn from_first(state: &ExtractedAccountState) -> Self {
        Self {
            sequence_number: state.sequence_number,
            sequence_set: state.sequence_number >= 0,
            first_seen_ledger: state.first_seen_ledger.map(i64::from),
            home_domain: state.home_domain.clone(),
            last_seen_ledger: Some(i64::from(state.last_seen_ledger)),
        }
    }
}

#[derive(Debug)]
struct AccountOverridePublic {
    last_seen_ledger: Option<i64>,
    first_seen_ledger: Option<i64>,
    sequence_number: i64,
    home_domain: Option<String>,
}

fn merge_account_state_overrides(
    states: &[ExtractedAccountState],
) -> HashMap<String, AccountOverridePublic> {
    use std::collections::hash_map::Entry;
    let mut overrides: HashMap<String, AccountOverride> = HashMap::new();
    for st in states {
        match overrides.entry(st.account_id.clone()) {
            Entry::Occupied(mut occ) => {
                let existing = occ.get_mut();
                if st.sequence_number >= 0 {
                    existing.sequence_number = st.sequence_number;
                    existing.sequence_set = true;
                }
                let incoming = st.first_seen_ledger.map(i64::from);
                existing.first_seen_ledger = match (existing.first_seen_ledger, incoming) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, b) => b,
                };
                if let Some(hd) = st.home_domain.clone() {
                    existing.home_domain = Some(hd);
                }
                let last = i64::from(st.last_seen_ledger);
                existing.last_seen_ledger = Some(match existing.last_seen_ledger {
                    Some(prev) => prev.max(last),
                    None => last,
                });
            }
            Entry::Vacant(vac) => {
                vac.insert(AccountOverride::from_first(st));
            }
        }
    }
    overrides
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                AccountOverridePublic {
                    last_seen_ledger: v.last_seen_ledger,
                    first_seen_ledger: v.first_seen_ledger,
                    sequence_number: if v.sequence_set { v.sequence_number } else { 0 },
                    home_domain: v.home_domain,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod balance_tests {
    use super::*;
    use xdr_parser::ExtractedSorobanBalance;

    #[test]
    fn build_balance_rows_maps_holder_and_asset_surrogates() {
        let extracted = vec![ExtractedSorobanBalance {
            contract_id: "CTOKEN1".into(),
            holder: "GHOLDER1".into(),
            balance: 800_009_446_178_i128,
            ledger: 100,
        }];
        // No SAC map → type-3 keying: asset_id == the token's contract surrogate.
        let rows = build_balance_rows(&extracted, &HashMap::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].holder_id, ids::address_id("GHOLDER1"));
        assert_eq!(rows[0].asset_id, ids::contract_id("CTOKEN1"));
        assert_eq!(rows[0].amount, 800_009_446_178_i128);
        assert_eq!(rows[0].last_updated_ledger, 100);
    }

    #[test]
    fn build_balance_rows_rekeys_sac_via_map_but_not_type3() {
        // A SAC contract-held balance + a type-3 token balance. The map re-keys
        // only the SAC (its surrogate → classic id); the type-3 token is absent
        // and keeps its own surrogate.
        let classic_usdc = ids::asset_id(1, "USDC", 42, 0);
        let sac_classic = HashMap::from([(ids::contract_id("CSAC"), classic_usdc)]);
        let rows = build_balance_rows(
            &[
                ExtractedSorobanBalance {
                    contract_id: "CSAC".into(),
                    holder: "CPOOL".into(),
                    balance: 100,
                    ledger: 10,
                },
                ExtractedSorobanBalance {
                    contract_id: "CTOKEN3".into(),
                    holder: "GHOLDER".into(),
                    balance: 200,
                    ledger: 10,
                },
            ],
            &sac_classic,
        );

        assert_eq!(
            rows[0].asset_id, classic_usdc,
            "SAC contract-held → classic id"
        );
        assert_eq!(
            rows[1].asset_id,
            ids::contract_id("CTOKEN3"),
            "type-3 unchanged"
        );
    }
}
