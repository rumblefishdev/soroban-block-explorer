//! `liquidity_pools` and `liquidity_pool_snapshots` rows for CLASSIC pools, from
//! the parser's pool changes (`xdr_parser::state::extract_liquidity_pools`).
//!
//! Two callers: ledger staging, and `snapshot-seed`, which inserts pools the
//! ingest floor never saw (task 0210). One builder, so a pool the seed inserts is
//! row-for-row what live ingest would have written for the same entry.

use std::collections::HashMap;

use domain::AssetType;
use serde_json::Value;
use xdr_parser::types::{ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot};

use super::ids;
use super::rows::{LiquidityPoolRow, LiquidityPoolSnapshotRow};
use super::stage::{decimal7_string_to_i128, decode_hash};
use crate::SchemaError;

/// One row per pool, carrying the newest `last_updated_ledger` seen.
pub fn build_pool_rows(
    liquidity_pools: &[ExtractedLiquidityPool],
) -> Result<Vec<LiquidityPoolRow>, SchemaError> {
    let mut rows: Vec<LiquidityPoolRow> = Vec::new();
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
        let asset_a_code = a_code.unwrap_or_default();
        let asset_a_issuer_id = a_issuer.as_deref().map(ids::account_id).unwrap_or(0);
        let asset_b_code = b_code.unwrap_or_default();
        let asset_b_issuer_id = b_issuer.as_deref().map(ids::account_id).unwrap_or(0);
        let new_row = LiquidityPoolRow {
            pool_id,
            // Legs migration step 2 (task 0374 committed follow-through):
            // classic rows fill `legs` too, so the pair columns can retire.
            // Classic legs are ASSET surrogates (`pool_leg_asset_id` — the
            // same key `lp_operation_amounts` joins on), NOT contract
            // surrogates like a soroban row's; `pool_kind` says which space.
            legs: vec![
                ids::pool_leg_asset_id(a_type as i16, &asset_a_code, asset_a_issuer_id),
                ids::pool_leg_asset_id(b_type as i16, &asset_b_code, asset_b_issuer_id),
            ],
            asset_a_type: a_type as i16,
            asset_a_code,
            asset_a_issuer_id,
            asset_b_type: b_type as i16,
            asset_b_code,
            asset_b_issuer_id,
            fee_bps: pool.fee_bps,
            last_updated_ledger,
            pool_kind: 0,
            deployment_id: 0,
            pool_type_raw: String::new(),
        };
        match pool_indices.get(&pool_id).copied() {
            Some(idx) => {
                let existing = &mut rows[idx];
                if last_updated_ledger >= existing.last_updated_ledger {
                    existing.last_updated_ledger = last_updated_ledger;
                }
            }
            None => {
                pool_indices.insert(pool_id, rows.len());
                rows.push(new_row);
            }
        }
    }
    Ok(rows)
}

/// One row per snapshot; `gross_volume_by_pool` is this ledger's asset-A trade
/// volume per pool (`stage::gross_volume_a_by_pool`), empty when there is none.
pub fn build_snapshot_rows(
    pool_snapshots: &[ExtractedLiquidityPoolSnapshot],
    gross_volume_by_pool: &HashMap<[u8; 32], i128>,
) -> Result<Vec<LiquidityPoolSnapshotRow>, SchemaError> {
    let mut rows = Vec::with_capacity(pool_snapshots.len());
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
        rows.push(LiquidityPoolSnapshotRow {
            pool_id,
            ledger_sequence: i64::from(snap.ledger_sequence),
            reserve_a,
            reserve_b,
            total_shares: decimal7_string_to_i128(&snap.total_shares)?,
            // Asset-A-side trade volume for this (pool, ledger) from claim
            // atoms (0261). `None` when the pool had no trade this ledger.
            // USD tvl/volume/fee_revenue have NO columns here any more: they
            // were written as NULL since 0199 (compute-at-read, ADR 0053) and
            // read by nothing — dropped from the write path in 0374's
            // distillation; prod drops them with ALTER … DROP COLUMN.
            gross_volume_a: gross_volume_by_pool.get(&pool_id).copied(),
        });
    }
    Ok(rows)
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
