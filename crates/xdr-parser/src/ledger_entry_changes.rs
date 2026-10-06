//! LedgerEntryChanges extraction from transaction metadata.
//!
//! Extracts all ledger entry mutations (created, updated, removed, state)
//! from `TransactionMeta` V3/V4. Each `LedgerEntryChange` is converted into
//! an `ExtractedLedgerEntryChange` with typed key fields and full entry data.
//!
//! Supported entry types: account, trustline, offer, data, claimable_balance,
//! liquidity_pool, contract_data, contract_code, config_setting, ttl.

use serde_json::{Value, json};
use stellar_xdr::*;

use crate::scval::scval_to_typed_json;
use crate::token_metadata::{
    TokenMetadata, extract_token_metadata, has_metadata_key, is_stellar_asset_instance,
};
use crate::types::ExtractedLedgerEntryChange;

/// Extract all ledger entry changes from a transaction's metadata.
///
/// Iterates `tx_changes_before`, per-operation changes, and `tx_changes_after`
/// in order. Returns one `ExtractedLedgerEntryChange` per `LedgerEntryChange`.
/// Non-V3/V4 meta produces an empty vec.
pub fn extract_ledger_entry_changes(
    tx_meta: &TransactionMeta,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
) -> Vec<ExtractedLedgerEntryChange> {
    let mut results = Vec::new();
    let mut index: u32 = 0;

    match tx_meta {
        TransactionMeta::V3(v3) => {
            extract_from_changes(
                &v3.tx_changes_before,
                None,
                transaction_hash,
                ledger_sequence,
                created_at,
                &mut index,
                &mut results,
            );
            for (op_idx, op_meta) in v3.operations.iter().enumerate() {
                let op_index =
                    Some(u32::try_from(op_idx).expect("operation index does not fit into u32"));
                extract_from_changes(
                    &op_meta.changes,
                    op_index,
                    transaction_hash,
                    ledger_sequence,
                    created_at,
                    &mut index,
                    &mut results,
                );
            }
            extract_from_changes(
                &v3.tx_changes_after,
                None,
                transaction_hash,
                ledger_sequence,
                created_at,
                &mut index,
                &mut results,
            );
        }
        TransactionMeta::V4(v4) => {
            extract_from_changes(
                &v4.tx_changes_before,
                None,
                transaction_hash,
                ledger_sequence,
                created_at,
                &mut index,
                &mut results,
            );
            for (op_idx, op_meta) in v4.operations.iter().enumerate() {
                let op_index =
                    Some(u32::try_from(op_idx).expect("operation index does not fit into u32"));
                extract_from_changes(
                    &op_meta.changes,
                    op_index,
                    transaction_hash,
                    ledger_sequence,
                    created_at,
                    &mut index,
                    &mut results,
                );
            }
            extract_from_changes(
                &v4.tx_changes_after,
                None,
                transaction_hash,
                ledger_sequence,
                created_at,
                &mut index,
                &mut results,
            );
        }
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => {}
    }

    results
}

/// Process a single `LedgerEntryChanges` collection.
fn extract_from_changes(
    changes: &LedgerEntryChanges,
    operation_index: Option<u32>,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    index: &mut u32,
    results: &mut Vec<ExtractedLedgerEntryChange>,
) {
    for change in changes.iter() {
        if let Some(extracted) = extract_single_change(
            change,
            operation_index,
            transaction_hash,
            ledger_sequence,
            created_at,
            *index,
        ) {
            results.push(extracted);
        }
        *index += 1;
    }
}

/// A ledger entry read outside any transaction — a checkpoint bucket record — as
/// an `updated` change stamped with the entry's own `lastModifiedLedgerSeq`:
/// the entry's value as of that ledger. Lets `snapshot-seed` build rows through
/// the same extractor and builders as live ingest instead of a second decoder.
///
/// `updated`, not `state`: a `state` change is the image at the start of an
/// operation and the extractors never take a value from it. Not `created`
/// either, which would claim the pool was created at its last modification.
/// There is no transaction, so the hash is empty and `created_at` is 0: use it
/// only for rows that store neither.
pub fn entry_as_current_change(entry: &LedgerEntry) -> Option<ExtractedLedgerEntryChange> {
    extract_single_change(
        &LedgerEntryChange::Updated(entry.clone()),
        None,
        "",
        entry.last_modified_ledger_seq,
        0,
        0,
    )
}

/// Convert a single `LedgerEntryChange` into an `ExtractedLedgerEntryChange`.
fn extract_single_change(
    change: &LedgerEntryChange,
    operation_index: Option<u32>,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    change_index: u32,
) -> Option<ExtractedLedgerEntryChange> {
    let (change_type, entry_type, key, data, token_metadata) = match change {
        LedgerEntryChange::Created(entry) => {
            let (et, k, d) = extract_entry_info(entry);
            ("created", et, k, Some(d), entry_token_metadata(entry))
        }
        LedgerEntryChange::Updated(entry) => {
            let (et, k, d) = extract_entry_info(entry);
            ("updated", et, k, Some(d), entry_token_metadata(entry))
        }
        LedgerEntryChange::Removed(ledger_key) => {
            let (et, k) = extract_key_info(ledger_key);
            ("removed", et, k, None, None)
        }
        LedgerEntryChange::State(entry) => {
            let (et, k, d) = extract_entry_info(entry);
            ("state", et, k, Some(d), entry_token_metadata(entry))
        }
        LedgerEntryChange::Restored(entry) => {
            let (et, k, d) = extract_entry_info(entry);
            ("restored", et, k, Some(d), entry_token_metadata(entry))
        }
    };

    Some(ExtractedLedgerEntryChange {
        transaction_hash: transaction_hash.to_string(),
        change_type: change_type.to_string(),
        entry_type: entry_type.to_string(),
        key,
        data,
        change_index,
        operation_index,
        ledger_sequence,
        created_at,
        token_metadata,
    })
}

/// Pull token metadata (`name`/`symbol`/`decimals`) from a contract-instance
/// entry's stored value. Returns `None` for any non-`ContractData` entry, a
/// value that is not a contract instance carrying a `METADATA` struct, OR a SAC
/// instance.
///
/// SACs are skipped here on purpose: a SAC *does* carry METADATA on-chain, but
/// its name (`CODE:ISSUER`) / symbol (= asset code) / decimals (= 7) derive from
/// the asset identity, so we never store a `soroban_contract_metadata` row for
/// it. Returning `None` is the single signal the producer
/// (`state::extract_contract_metadata_writes`) keys off — no separate `is_sac`
/// flag is threaded on the change. See `crate::token_metadata`.
fn entry_token_metadata(entry: &LedgerEntry) -> Option<TokenMetadata> {
    let LedgerEntryData::ContractData(cd) = &entry.data else {
        return None;
    };
    // SAC: carries METADATA on-chain, but name/symbol/decimals derive from the
    // asset identity — skip silently (an expected non-store, not a miss).
    if is_stellar_asset_instance(&cd.val) {
        return None;
    }
    match extract_token_metadata(&cd.val) {
        Some(md) => Some(md),
        // A METADATA key is present but yielded nothing usable (non-Map struct,
        // or name/symbol/decimals in a shape we don't decode). Surface the
        // contract instead of silently dropping it — a non-standard token we'd
        // otherwise never know we missed ("monitored UNKNOWN"). Broaden the
        // decoder only once a real shape shows up here.
        None if has_metadata_key(&cd.val) => {
            tracing::warn!(
                contract = %cd.contract,
                "contract instance carries Symbol(\"METADATA\") but no usable \
                 name/symbol/decimals decoded — non-standard shape, skipped"
            );
            None
        }
        None => None,
    }
}

// ---------------------------------------------------------------------------
// Entry data extraction (for created/updated/state)
// ---------------------------------------------------------------------------

/// Extract entry type, key fields, and full data from a `LedgerEntry`.
fn extract_entry_info(entry: &LedgerEntry) -> (&'static str, Value, Value) {
    match &entry.data {
        LedgerEntryData::Account(a) => ("account", account_key(a), account_data(a)),
        LedgerEntryData::Trustline(t) => ("trustline", trustline_key(t), trustline_data(t)),
        LedgerEntryData::Offer(o) => ("offer", offer_key(o), offer_data(o)),
        LedgerEntryData::Data(d) => ("data", data_entry_key(d), data_entry_data(d)),
        LedgerEntryData::ClaimableBalance(cb) => (
            "claimable_balance",
            claimable_balance_key(cb),
            claimable_balance_data(cb),
        ),
        LedgerEntryData::LiquidityPool(lp) => (
            "liquidity_pool",
            liquidity_pool_key(lp),
            liquidity_pool_data(lp),
        ),
        LedgerEntryData::ContractData(cd) => (
            "contract_data",
            contract_data_key(cd),
            contract_data_data(cd),
        ),
        LedgerEntryData::ContractCode(cc) => (
            "contract_code",
            contract_code_key(cc),
            contract_code_data(cc),
        ),
        // Config setting payload intentionally excluded — protocol-internal, not exposed by explorer.
        // Each variant has a different inner type; serializing all is high effort, low value.
        LedgerEntryData::ConfigSetting(cs) => ("config_setting", config_setting_key(cs), json!({})),
        LedgerEntryData::Ttl(t) => ("ttl", ttl_key(t), ttl_data(t)),
    }
}

// ---------------------------------------------------------------------------
// Key extraction from LedgerKey (for removed changes)
// ---------------------------------------------------------------------------

/// Extract entry type and key fields from a `LedgerKey`.
fn extract_key_info(key: &LedgerKey) -> (&'static str, Value) {
    match key {
        LedgerKey::Account(k) => ("account", json!({ "account_id": k.account_id.to_string() })),
        LedgerKey::Trustline(k) => (
            "trustline",
            json!({
                "account_id": k.account_id.to_string(),
                "asset": format_trustline_asset_key(&k.asset),
            }),
        ),
        LedgerKey::Offer(k) => (
            "offer",
            json!({
                "seller_id": k.seller_id.to_string(),
                "offer_id": k.offer_id,
            }),
        ),
        LedgerKey::Data(k) => (
            "data",
            json!({
                "account_id": k.account_id.to_string(),
                "data_name": String::from_utf8_lossy(k.data_name.as_slice()).to_string(),
            }),
        ),
        LedgerKey::ClaimableBalance(k) => (
            "claimable_balance",
            json!({ "balance_id": format_claimable_balance_id(&k.balance_id) }),
        ),
        LedgerKey::LiquidityPool(k) => (
            "liquidity_pool",
            json!({ "pool_id": hex::encode(k.liquidity_pool_id.0.clone()) }),
        ),
        LedgerKey::ContractData(k) => (
            "contract_data",
            json!({
                "contract": k.contract.to_string(),
                "key": scval_to_typed_json(&k.key),
                "durability": format_durability(&k.durability),
            }),
        ),
        LedgerKey::ContractCode(k) => ("contract_code", json!({ "hash": hex::encode(k.hash.0) })),
        LedgerKey::ConfigSetting(k) => (
            "config_setting",
            json!({ "config_setting_id": format!("{:?}", k.config_setting_id) }),
        ),
        LedgerKey::Ttl(k) => ("ttl", json!({ "key_hash": hex::encode(k.key_hash.0) })),
    }
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

fn account_key(a: &AccountEntry) -> Value {
    json!({ "account_id": a.account_id.to_string() })
}

/// XDR arm → the wire word for a signer key kind. Total over `SignerKey` on
/// purpose — a new arm must fail compilation here, never pass through blank.
///
/// `pub` because the checkpoint-snapshot seed writes into the SAME
/// `account_entry_state.signer_types` column as this writer. A second copy of these
/// four words would split the stored vocabulary by whichever path last touched
/// the account — silently. Sharing the function makes that a compile-time
/// guarantee instead of a promise in a comment.
pub fn signer_type_name(key: &SignerKey) -> &'static str {
    match key {
        SignerKey::Ed25519(_) => "ed25519",
        SignerKey::PreAuthTx(_) => "preauth_tx",
        SignerKey::HashX(_) => "hash_x",
        SignerKey::Ed25519SignedPayload(_) => "ed25519_signed_payload",
    }
}

fn account_data(a: &AccountEntry) -> Value {
    json!({
        "account_id": a.account_id.to_string(),
        "balance": a.balance,
        "seq_num": i64::from(a.seq_num.clone()),
        "num_sub_entries": a.num_sub_entries,
        "home_domain": String::from_utf8_lossy(a.home_domain.as_slice()).to_string(),
        "thresholds": hex::encode(a.thresholds.0),
        "flags": a.flags,
        // Raw XDR truth: the master key is NOT in this list — its weight is
        // thresholds byte 0. Horizon SYNTHESIZES a master entry into its
        // signers array; comparing against Horizon therefore reads off-by-one
        // by design. Signer keys render as strkeys via stellar-xdr's own
        // Display (G/T/X/P namespaces). lore-0463.
        "signers": a.signers.iter().map(|s| json!({
            "key": s.key.to_string(),
            "weight": s.weight,
            "type": signer_type_name(&s.key),
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Trustline
// ---------------------------------------------------------------------------

fn trustline_key(t: &TrustLineEntry) -> Value {
    json!({
        "account_id": t.account_id.to_string(),
        "asset": format_trustline_asset(&t.asset),
    })
}

fn trustline_data(t: &TrustLineEntry) -> Value {
    json!({
        "account_id": t.account_id.to_string(),
        "asset": format_trustline_asset(&t.asset),
        "balance": t.balance,
        "limit": t.limit,
        "flags": t.flags,
    })
}

fn format_trustline_asset(asset: &TrustLineAsset) -> Value {
    match asset {
        TrustLineAsset::Native => json!("native"),
        TrustLineAsset::CreditAlphanum4(a) => json!({
            "type": "credit_alphanum4",
            "code": crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            "issuer": a.issuer.to_string(),
        }),
        TrustLineAsset::CreditAlphanum12(a) => json!({
            "type": "credit_alphanum12",
            "code": crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            "issuer": a.issuer.to_string(),
        }),
        TrustLineAsset::PoolShare(pool_id) => {
            json!({ "type": "pool_share", "pool_id": hex::encode(pool_id.0.clone()) })
        }
    }
}

fn format_trustline_asset_key(asset: &TrustLineAsset) -> Value {
    format_trustline_asset(asset)
}

// ---------------------------------------------------------------------------
// Offer
// ---------------------------------------------------------------------------

fn offer_key(o: &OfferEntry) -> Value {
    json!({
        "seller_id": o.seller_id.to_string(),
        "offer_id": o.offer_id,
    })
}

fn offer_data(o: &OfferEntry) -> Value {
    json!({
        "seller_id": o.seller_id.to_string(),
        "offer_id": o.offer_id,
        "selling": format_asset(&o.selling),
        "buying": format_asset(&o.buying),
        "amount": o.amount,
        "price": { "n": o.price.n, "d": o.price.d },
        "flags": o.flags,
    })
}

fn format_asset(asset: &Asset) -> Value {
    match asset {
        Asset::Native => json!("native"),
        Asset::CreditAlphanum4(a) => json!({
            "type": "credit_alphanum4",
            "code": crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            "issuer": a.issuer.to_string(),
        }),
        Asset::CreditAlphanum12(a) => json!({
            "type": "credit_alphanum12",
            "code": crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            "issuer": a.issuer.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Data entry
// ---------------------------------------------------------------------------

fn data_entry_key(d: &DataEntry) -> Value {
    json!({
        "account_id": d.account_id.to_string(),
        "data_name": String::from_utf8_lossy(d.data_name.as_slice()).to_string(),
    })
}

fn data_entry_data(d: &DataEntry) -> Value {
    json!({
        "account_id": d.account_id.to_string(),
        "data_name": String::from_utf8_lossy(d.data_name.as_slice()).to_string(),
        "data_value": hex::encode(d.data_value.as_slice()),
    })
}

// ---------------------------------------------------------------------------
// Claimable balance
// ---------------------------------------------------------------------------

fn claimable_balance_key(cb: &ClaimableBalanceEntry) -> Value {
    json!({ "balance_id": format_claimable_balance_id(&cb.balance_id) })
}

fn claimable_balance_data(cb: &ClaimableBalanceEntry) -> Value {
    json!({
        "balance_id": format_claimable_balance_id(&cb.balance_id),
        "asset": format_asset(&cb.asset),
        "amount": cb.amount,
        "claimants": cb.claimants.iter().map(format_claimant).collect::<Vec<_>>(),
    })
}

fn format_claimable_balance_id(id: &ClaimableBalanceId) -> String {
    match id {
        ClaimableBalanceId::ClaimableBalanceIdTypeV0(hash) => hex::encode(hash.0),
    }
}

fn format_claimant(c: &Claimant) -> Value {
    match c {
        Claimant::ClaimantTypeV0(v0) => json!({
            "destination": v0.destination.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Liquidity pool
// ---------------------------------------------------------------------------

fn liquidity_pool_key(lp: &LiquidityPoolEntry) -> Value {
    json!({ "pool_id": hex::encode(lp.liquidity_pool_id.0.clone()) })
}

fn liquidity_pool_data(lp: &LiquidityPoolEntry) -> Value {
    match &lp.body {
        LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) => json!({
            "pool_id": hex::encode(lp.liquidity_pool_id.0.clone()),
            "type": "constant_product",
            "params": {
                "asset_a": format_asset(&cp.params.asset_a),
                "asset_b": format_asset(&cp.params.asset_b),
                "fee": cp.params.fee,
            },
            "reserve_a": cp.reserve_a,
            "reserve_b": cp.reserve_b,
            "total_pool_shares": cp.total_pool_shares,
            "pool_shares_trust_line_count": cp.pool_shares_trust_line_count,
        }),
    }
}

// ---------------------------------------------------------------------------
// Contract data
// ---------------------------------------------------------------------------

fn contract_data_key(cd: &ContractDataEntry) -> Value {
    json!({
        "contract": cd.contract.to_string(),
        "key": scval_to_typed_json(&cd.key),
        "durability": format_durability(&cd.durability),
    })
}

fn contract_data_data(cd: &ContractDataEntry) -> Value {
    json!({
        "contract": cd.contract.to_string(),
        "key": scval_to_typed_json(&cd.key),
        "durability": format_durability(&cd.durability),
        "val": scval_to_typed_json(&cd.val),
    })
}

fn format_durability(d: &ContractDataDurability) -> &'static str {
    match d {
        ContractDataDurability::Temporary => "temporary",
        ContractDataDurability::Persistent => "persistent",
    }
}

// ---------------------------------------------------------------------------
// Contract code
// ---------------------------------------------------------------------------

fn contract_code_key(cc: &ContractCodeEntry) -> Value {
    json!({ "hash": hex::encode(cc.hash.0) })
}

fn contract_code_data(cc: &ContractCodeEntry) -> Value {
    json!({
        "hash": hex::encode(cc.hash.0),
        "code_byte_len": cc.code.as_slice().len(),
    })
}

// ---------------------------------------------------------------------------
// Config setting
// ---------------------------------------------------------------------------

fn config_setting_key(cs: &ConfigSettingEntry) -> Value {
    json!({ "config_setting_id": config_setting_id_name(cs.discriminant()) })
}

/// Stored identifier for a config-setting variant. Matching on
/// `ConfigSettingId` (a unit enum) rather than `ConfigSettingEntry` lets the
/// coverage test below enumerate `ConfigSettingEntry::VARIANTS` without
/// constructing entry payloads.
fn config_setting_id_name(id: ConfigSettingId) -> &'static str {
    match id {
        ConfigSettingId::ContractMaxSizeBytes => "contract_max_size_bytes",
        ConfigSettingId::ContractComputeV0 => "contract_compute_v0",
        ConfigSettingId::ContractLedgerCostV0 => "contract_ledger_cost_v0",
        ConfigSettingId::ContractHistoricalDataV0 => "contract_historical_data_v0",
        ConfigSettingId::ContractEventsV0 => "contract_events_v0",
        ConfigSettingId::ContractBandwidthV0 => "contract_bandwidth_v0",
        ConfigSettingId::ContractCostParamsCpuInstructions => "contract_cost_params_cpu",
        ConfigSettingId::ContractCostParamsMemoryBytes => "contract_cost_params_memory",
        ConfigSettingId::ContractDataKeySizeBytes => "contract_data_key_size_bytes",
        ConfigSettingId::ContractDataEntrySizeBytes => "contract_data_entry_size_bytes",
        ConfigSettingId::StateArchival => "state_archival",
        ConfigSettingId::ContractExecutionLanes => "contract_execution_lanes",
        ConfigSettingId::EvictionIterator => "eviction_iterator",
        ConfigSettingId::LiveSorobanStateSizeWindow => "live_soroban_state_size_window",
        ConfigSettingId::ContractParallelComputeV0 => "contract_parallel_compute_v0",
        ConfigSettingId::ContractLedgerCostExtV0 => "contract_ledger_cost_ext_v0",
        ConfigSettingId::ScpTiming => "scp_timing",
        ConfigSettingId::FrozenLedgerKeys => "frozen_ledger_keys",
        ConfigSettingId::FrozenLedgerKeysDelta => "frozen_ledger_keys_delta",
        ConfigSettingId::FreezeBypassTxs => "freeze_bypass_txs",
        ConfigSettingId::FreezeBypassTxsDelta => "freeze_bypass_txs_delta",
    }
}

// ---------------------------------------------------------------------------
// TTL
// ---------------------------------------------------------------------------

fn ttl_key(t: &TtlEntry) -> Value {
    json!({ "key_hash": hex::encode(t.key_hash.0) })
}

fn ttl_data(t: &TtlEntry) -> Value {
    json!({
        "key_hash": hex::encode(t.key_hash.0),
        "live_until_ledger_seq": t.live_until_ledger_seq,
    })
}

#[cfg(test)]
mod tests;
