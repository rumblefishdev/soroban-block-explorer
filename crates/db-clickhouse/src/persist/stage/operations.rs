//! Operation-derived rows staged per ledger: `operations_appearances` (the
//! identity fold), the op-derived half of `operation_asset_appearances`,
//! `operation_pools` and `lp_operation_amounts` — every table filled by the
//! one walk over each transaction's operations.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{HashMap, HashSet};

use xdr_parser::asset_appearances::AssetRef;
use xdr_parser::types::ExtractedOperation;

use super::{OpTyped, StagedLedger, decode_hash, pool_fill_amounts, staging_err};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::{
    LpOperationAmountRow, OperationAppearanceRow, OperationAssetAppearanceRow, OperationPoolRow,
};

pub(super) fn operation_rows(
    out: &mut StagedLedger,
    operations: &[(String, Vec<ExtractedOperation>)],
    tx_id_by_hash: &HashMap<String, i64>,
    app_order_by_hash: &HashMap<String, i16>,
    ledger_sequence_i64: i64,
) -> Result<(), SchemaError> {
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
        // Per-tx dedup for the asset fan-out (PR #6): N ops touching the same
        // asset in one tx would otherwise write N identical (asset, tx) rows. The
        // RMT sort key collapses them eventually, but deduping at write cuts the
        // backfilled volume up front. Scoped per tx — one entry per tx_hash here.
        let mut seen_tx_asset_ids: HashSet<i64> = HashSet::new();
        // Same per-tx dedup for the pool fan-out (task 0365): N ops crossing the
        // same pool in one tx → one (pool, tx) row.
        let mut seen_tx_pool_ids: HashSet<[u8; 32]> = HashSet::new();
        for op in ops {
            // ---- operation_asset_appearances (task 0359, pure presence) ----
            // Asset-dimension twin of transaction_participants: one row per
            // (asset the op touches, tx). Native is a FIRST-CLASS surrogate (never
            // the empty-string sentinel); classic credit hashes
            // code:issuer_surrogate — both via `ids::asset_id`.
            if !op.asset_appearances.is_empty() {
                let application_order = app_order_by_hash[tx_hash];
                for asset in &op.asset_appearances {
                    let asset_id = match asset {
                        AssetRef::Native => ids::NATIVE_ASSET_ID,
                        AssetRef::Credit { code, issuer } => ids::credit_asset_id(code, issuer),
                    };
                    if seen_tx_asset_ids.insert(asset_id) {
                        out.op_asset_rows.push(OperationAssetAppearanceRow {
                            asset_id,
                            ledger_sequence: ledger_sequence_i64,
                            application_order,
                        });
                    }
                }
            }

            let typed = OpTyped::from_details(op.op_type, &op.details);
            let mut pool_ids = Vec::with_capacity(typed.pool_ids_hex.len());
            for h in &typed.pool_ids_hex {
                pool_ids.push(decode_hash(h, "op.pool_ids")?);
            }
            pool_ids.sort_unstable();
            pool_ids.dedup();

            // ---- operation_pools (task 0365, pure presence) ----
            // Pool-dimension twin of the asset fan-out above: one row per (pool
            // the op crossed, tx). `pool_ids` is already the sorted+deduped
            // crossing list; dedup per-tx so N ops crossing the same pool in one
            // tx write one (pool, tx) row (the RMT collapses any residual). Sourced
            // from `oa.pool_ids` — no XDR-only data, so a plain CH re-key can
            // backfill it (task 0365 Path B).
            if !pool_ids.is_empty() {
                let tx_id = tx_id_by_hash[tx_hash];
                for pool_id in &pool_ids {
                    if seen_tx_pool_ids.insert(*pool_id) {
                        out.op_pool_rows.push(OperationPoolRow {
                            pool_id: *pool_id,
                            ledger_sequence: ledger_sequence_i64,
                            transaction_id: tx_id,
                        });
                    }
                }
            }

            // ---- lp_operation_amounts (task 0279) ----
            // The value twin of the block above: `gross_volume_a_by_pool` walks
            // the same trade atoms and sums them into one number per pool; here
            // the per-(op, pool, asset) attribution is KEPT instead of
            // discarded, and deposits/withdrawals — which have no atoms — come
            // from the op's own reserve delta.
            {
                let tx_id = tx_id_by_hash[tx_hash];
                // Fail the ledger rather than clamp, matching the
                // `transactions.application_order` conversion above: this
                // column is part of the ORDER BY, so two operations squeezed
                // onto one saturated value would share a key and the RMT would
                // drop a fill silently — the loss the per-op summing exists to
                // prevent. Unreachable while Stellar caps ops per tx at 100.
                let order = i16::try_from(op.operation_index)
                    .map_err(|_| staging_err("lp_operation_amounts application_order (>i16)"))?;
                for (pool_id, asset_id, amount) in pool_fill_amounts(&op.details) {
                    out.lp_amount_rows.push(LpOperationAmountRow {
                        pool_id,
                        ledger_sequence: ledger_sequence_i64,
                        transaction_id: tx_id,
                        application_order: order,
                        asset_id,
                        amount,
                    });
                }
            }

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
    Ok(())
}
