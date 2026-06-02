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
//! Parser-emitted `removed_trustlines` are translated to
//! `AccountBalanceRow` with `balance = 0` and current ledger as
//! `last_updated_ledger`. `ReplacingMergeTree(last_updated_ledger)`
//! keeps the zero-balance row (newest version wins). Read-time
//! convention: `WHERE balance > 0` to recover "active trustlines"
//! semantics.

use std::collections::{HashMap, HashSet};

use domain::{AssetType, ContractType, OperationType};
use serde_json::Value;
use xdr_parser::SacOverride;
use xdr_parser::types::{
    EventSource, ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment,
    ExtractedContractInterface, ExtractedEvent, ExtractedInvocation, ExtractedLedger,
    ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft,
    ExtractedNftEvent, ExtractedOperation, ExtractedTransaction, SacAssetIdentity,
};

use super::ids;
use super::rows::*;
use crate::SchemaError;

#[derive(Debug, Default)]
pub struct StagedLedger {
    pub ledger_sequence: i64,

    pub ledger_rows: Vec<LedgerRow>,
    pub account_rows: Vec<AccountRow>,
    pub wasm_rows: Vec<WasmInterfaceMetadataRow>,
    pub contract_rows: Vec<SorobanContractRow>,
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
    pub balance_rows: Vec<AccountBalanceRow>,
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
    contract_name_writes: &[(String, String)],
) -> Result<StagedLedger, SchemaError> {
    prepare_with_sac_overrides(
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
        contract_name_writes,
        &[],
    )
}

/// Same as [`prepare`] but also re-emits SAC-override `ContractRow`s
/// for every `(contract_id, identity)` pair in `sac_overrides` (task
/// 0220 Part 2). Each override row carries `is_sac=true,
/// contract_type=Token, wasm_uploaded_at_ledger=0` so RMT collapses by
/// `ORDER BY (contract_id)` keeping the SAC-flagged version over the
/// `is_sac=false` Pass-2 stub that would otherwise be emitted for the
/// same contract.
///
/// Production callers that have a `ParseOutput.sac_overrides` slice
/// (PG-side bridge for task 0218 + the CH backfill path) call this
/// directly; legacy callers via [`prepare`] get a no-op override list
/// and behave exactly as before — the override mechanism stays opt-in
/// at the call site, so the addition is fully backward-compatible.
#[allow(clippy::too_many_arguments)]
pub fn prepare_with_sac_overrides(
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
    contract_name_writes: &[(String, String)],
    sac_overrides: &[SacOverride],
) -> Result<StagedLedger, SchemaError> {
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

        let metadata = serde_json::json!({
            "functions": iface.functions,
            "wasm_byte_len": iface.wasm_byte_len,
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
        // wasm was classified in the same ledger and carries a
        // definitive `Nft` / `Fungible` verdict, override the parser
        // default (`Other` for non-SAC) before the row reaches CH.
        // SAC deploys stay `Token` (is_sac short-circuits WASM
        // classification — SACs have no WASM).
        let mut contract_type = dep.contract_type;
        if !dep.is_sac
            && let Some(hash) = wasm_hash
            && let Some(&classified) = wasm_classification.get(&hash)
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

    for (cid, name) in contract_name_writes {
        out.contract_rows.push(SorobanContractRow {
            id: ids::contract_id(cid),
            contract_id: cid.clone(),
            wasm_hash: None,
            wasm_uploaded_at_ledger: ledger_sequence_i64,
            deployer_id: None,
            deployed_at_ledger: None,
            contract_type: None,
            is_sac: false,
            name: Some(name.clone()),
        });
    }

    // Task 0220 — SAC override re-insert. For every observed classic
    // asset (Native / ClassicCredit), the parser already derived the
    // SAC contract_id (see `xdr_parser::derive_sac_overrides_from_assets`).
    // Re-insert a corrected `SorobanContractRow` with
    // `is_sac=true, contract_type=Token, wasm_uploaded_at_ledger=0` so
    // RMT collapses by `ORDER BY (contract_id)` keeping the override
    // version when the original deploy lived outside our backfill
    // window and persisted as `is_sac=false`. Skips contracts already
    // emitted from `contract_deployments` (no point in writing twice
    // in the same partition).
    {
        let mut override_seen: HashSet<&str> = HashSet::new();
        for cid in &contract_seen {
            override_seen.insert(cid.as_str());
        }
        for ov in sac_overrides {
            if !override_seen.insert(ov.contract_id.as_str()) {
                continue;
            }
            out.contract_rows.push(SorobanContractRow {
                id: ids::contract_id(&ov.contract_id),
                contract_id: ov.contract_id.clone(),
                wasm_hash: None,
                // RMT version = 0 sentinel: every real deploy (which
                // carries `wasm_uploaded_at_ledger = deployed_at_ledger
                // >= window_start`) wins over this override, so a
                // future in-window deploy of the same contract won't
                // be downgraded back to a stub. But the override
                // _does_ win over the existing `is_sac=false` skeleton
                // RMT-merged from referenced-only Pass-2 emits which
                // also carry `wasm_uploaded_at_ledger = 0` — the new
                // row dedupes by `(contract_id)` ORDER BY and the
                // freshly-written `is_sac=true` is the one kept.
                wasm_uploaded_at_ledger: 0,
                deployer_id: None,
                deployed_at_ledger: None,
                contract_type: Some(ContractType::Token as i16),
                is_sac: true,
                name: None,
            });
        }
    }

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
        pool_id: Option<[u8; 32]>,
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
            let pool_id = match &typed.pool_id_hex {
                Some(h) => Some(decode_hash(h, "op.pool_id")?),
                None => None,
            };
            let key = OpKey {
                tx_hash_hex: tx_hash.clone(),
                op_type: op.op_type as i16,
                source_account: op.source_account.clone(),
                destination_account: typed.destination,
                contract_strkey: typed.contract_id,
                asset_code: typed.asset_code.unwrap_or_default(),
                asset_issuer_account: typed.asset_issuer,
                pool_id,
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
            pool_id: k.pool_id,
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

    // ---- assets (dedup by 4-tuple identity) ----
    let mut asset_seen: HashSet<(i16, String, i64, i64)> = HashSet::new();
    let push_asset =
        |out: &mut StagedLedger, seen: &mut HashSet<(i16, String, i64, i64)>, row: AssetRow| {
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
    for t in assets {
        let issuer_id = t
            .issuer_address
            .as_deref()
            .map(ids::account_id)
            .unwrap_or(0);
        let contract_id_int = t.contract_id.as_deref().map(ids::contract_id).unwrap_or(0);
        let row = AssetRow {
            asset_type: t.asset_type as i16,
            asset_code: t.asset_code.clone().unwrap_or_default(),
            issuer_id,
            contract_id: contract_id_int,
            name: t.name.clone(),
            total_supply: t
                .total_supply
                .as_deref()
                .map(decimal7_string_to_i128)
                .transpose()?,
            holder_count: t.holder_count,
            icon_url: None,
        };
        push_asset(&mut out, &mut asset_seen, row);
    }

    for dep in contract_deployments {
        let Some(sac) = &dep.sac_asset else {
            continue;
        };
        let (asset_code, issuer_id) = match sac {
            SacAssetIdentity::Native => (String::new(), 0),
            SacAssetIdentity::Credit { code, issuer } => (code.clone(), ids::account_id(issuer)),
        };
        push_asset(
            &mut out,
            &mut asset_seen,
            AssetRow {
                asset_type: domain::TokenAssetType::Sac as i16,
                asset_code,
                issuer_id,
                contract_id: ids::contract_id(&dep.contract_id),
                name: None,
                total_supply: None,
                holder_count: None,
                icon_url: None,
            },
        );
    }

    // Native XLM singleton (PG sqlx migration 20260428000000 analog).
    push_asset(
        &mut out,
        &mut asset_seen,
        AssetRow {
            asset_type: domain::TokenAssetType::Native as i16,
            asset_code: String::new(),
            issuer_id: 0,
            contract_id: 0,
            name: Some("Stellar Lumen".to_string()),
            total_supply: None,
            holder_count: None,
            icon_url: None,
        },
    );

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
    // Contracts with NO entry → treat as `Other`/uncached → route to
    // pending. CH has no DB access in the stage, so prior-ledger
    // classifications are not visible here; this is the same semantic
    // PG would produce for a worker with an empty `ClassificationCache`
    // — pending now, drained / promoted later via the runbook.
    let mut verdict_by_contract: HashMap<&str, ContractType> = HashMap::new();
    for row in &out.contract_rows {
        if let Some(ty_i16) = row.contract_type
            && let Ok(ty) = ContractType::try_from(ty_i16)
        {
            verdict_by_contract.insert(row.contract_id.as_str(), ty);
        }
    }

    // 3-way routing helper. Mirrors PG `resolve_nft_filter` bucketing.
    enum NftRoute {
        Hot,
        Pending,
        Drop,
    }
    let route_for = |strkey: &str| -> NftRoute {
        match verdict_by_contract.get(strkey).copied() {
            Some(ContractType::Token) | Some(ContractType::Fungible) => NftRoute::Drop,
            Some(ContractType::Nft) => NftRoute::Hot,
            // `Other` (cached or just-fetched) and uncached (no entry)
            // both go to quarantine — same semantic as PG-side
            // `resolve_nft_filter`.
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

    // ---- account_balances_current ----
    let mut balance_dedup: HashMap<(i64, i16, String, i64), AccountBalanceRow> = HashMap::new();
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
            let balance = decimal7_string_to_i128(balance_str)?;
            let (asset_code, issuer_id) = if asset_type == AssetType::Native {
                (String::new(), 0)
            } else {
                let code = b
                    .get("asset_code")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let issuer = b
                    .get("issuer")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if code.is_empty() || issuer.is_empty() {
                    continue;
                }
                (code, ids::account_id(&issuer))
            };
            let key = (
                account_id_int,
                asset_type as i16,
                asset_code.clone(),
                issuer_id,
            );
            let row = AccountBalanceRow {
                account_id: account_id_int,
                asset_type: asset_type as i16,
                asset_code,
                issuer_id,
                balance,
                last_updated_ledger: watermark,
            };
            match balance_dedup.entry(key) {
                Entry::Occupied(mut occ) => {
                    let existing = occ.get_mut();
                    if row.last_updated_ledger >= existing.last_updated_ledger {
                        *existing = row;
                    }
                }
                Entry::Vacant(vac) => {
                    vac.insert(row);
                }
            }
        }

        for rm in &st.removed_trustlines {
            let code = rm
                .get("asset_code")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let issuer = rm
                .get("issuer")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if code.is_empty() || issuer.is_empty() {
                continue;
            }
            let asset_type = if code.len() <= 4 {
                AssetType::CreditAlphanum4
            } else {
                AssetType::CreditAlphanum12
            };
            let issuer_id = ids::account_id(&issuer);
            let key = (account_id_int, asset_type as i16, code.clone(), issuer_id);
            let row = AccountBalanceRow {
                account_id: account_id_int,
                asset_type: asset_type as i16,
                asset_code: code,
                issuer_id,
                balance: 0,
                last_updated_ledger: watermark,
            };
            match balance_dedup.entry(key) {
                Entry::Occupied(mut occ) => {
                    let existing = occ.get_mut();
                    if row.last_updated_ledger >= existing.last_updated_ledger {
                        *existing = row;
                    }
                }
                Entry::Vacant(vac) => {
                    vac.insert(row);
                }
            }
        }
    }
    out.balance_rows.extend(balance_dedup.into_values());

    // ---- soroban_contracts Pass 2 stub-rowing ----
    {
        let mut emitted: HashSet<&str> = HashSet::new();
        for cid in &contract_seen {
            emitted.insert(cid.as_str());
        }
        for (cid, _) in contract_name_writes {
            emitted.insert(cid.as_str());
        }
        // Task 0220 — exclude SAC-override contracts. The override row
        // (emitted earlier with `is_sac=true`) and a Pass-2 stub
        // (`is_sac=false`) would both carry `wasm_uploaded_at_ledger=0`;
        // CH RMT tie-breaks nondeterministically on equal version, so
        // emitting both could clobber the override on merge. Suppress
        // the stub here — the override carries enough fields
        // (`contract_id`, `id`) to satisfy the FK-by-id read path.
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
    pool_id_hex: Option<String>,
}

impl OpTyped {
    fn from_details(op_type: OperationType, details: &Value) -> Self {
        let mut out = Self {
            destination: None,
            contract_id: None,
            asset_code: None,
            asset_issuer: None,
            pool_id_hex: None,
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
                out.pool_id_hex = str_field(details, "liquidityPoolId");
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
