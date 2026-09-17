//! Derived state extraction from raw ledger entry changes.
//!
//! Processes `ExtractedLedgerEntryChange` records to produce higher-level
//! entities: contract deployments, account states, liquidity pools,
//! assets, and NFTs. This is the final parsing stage before DB persistence.

use std::collections::HashMap;

use serde_json::Value;
use tracing::{instrument, warn};

use crate::classification::{ContractClassification, classify_contract_from_wasm_spec};
use crate::types::{
    ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment, ExtractedContractInterface,
    ExtractedContractMetadata, ExtractedLedgerEntryChange, ExtractedLiquidityPool,
    ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft, ExtractedNftEvent,
    ExtractedSorobanBalance, NftEvent, SacAssetIdentity,
};
use domain::{AssetFamily, ContractType, NftEventType};

// ---------------------------------------------------------------------------
// Step 1 + Step 7: Contract Deployment + SAC Detection
// ---------------------------------------------------------------------------

/// Extract contract deployments from ledger entry changes.
///
/// Identifies new contract instances by looking for `contract_data` entries
/// with the contract instance key. Detects SACs from the executable type.
///
/// `sac_identities` maps `contract_id` (the deterministic preimage hash
/// per stellar-core, see `crate::sac::derive_sac_contract_id`) to the
/// underlying classic asset for every SAC found in the current batch's
/// transaction envelopes (top-level `CreateContract` ops AND
/// `CreateContractHostFn` auth entries — task 0160). For SAC
/// deployments without a matching identity (e.g. replay from
/// mid-ledger without the original deploy tx) the deployment still
/// lands here with `sac_asset: None`; `detect_assets` then skips the
/// asset row with a `tracing::warn` rather than fabricate one.
///
/// `deployer_by_contract` maps `contract_id` to the per-op effective
/// source for every `CreateContract*` reachable from the batch's
/// envelopes — top-level op effective source (op.source_account override
/// OR tx source) and auth-tree `CreateContractHostFn` signer
/// (`SorobanAuthorizationEntry.credentials`). When the map carries an
/// entry for the deployed `contract_id`, that StrKey wins; otherwise
/// `tx_source_account` is used as fallback. The fallback preserves
/// behaviour for the ~88 % of mainnet deploys where the op inherits the
/// tx source (no per-op override and no auth indirection). Built by
/// `crate::extract_op_source_per_contract` at the indexer call site.
/// Task 0255 Phase 1.
pub fn extract_contract_deployments(
    changes: &[ExtractedLedgerEntryChange],
    tx_source_account: &str,
    sac_identities: &HashMap<String, SacAssetIdentity>,
    deployer_by_contract: &HashMap<String, String>,
) -> Vec<ExtractedContractDeployment> {
    let mut deployments = Vec::new();

    for change in changes {
        if change.entry_type != "contract_data" || change.change_type != "created" {
            continue;
        }
        let Some(ref data) = change.data else {
            continue;
        };
        if !is_contract_instance_key(&change.key) {
            continue;
        }

        let contract_id = change
            .key
            .get("contract")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if contract_id.is_empty() {
            continue;
        }

        let is_sac = is_sac_from_data(data);
        let wasm_hash = extract_wasm_hash(data);
        // ADR 0031: synthetic 2-variant classification. SACs wrap a classic
        // asset and are always assets; everything else is `Other` until the
        // explorer learns to recognise a richer taxonomy.
        let contract_type = if is_sac {
            ContractType::Token
        } else {
            ContractType::Other
        };

        // Task 0160: SAC identity is keyed by the deterministic preimage
        // hash (== contract_id). Lookup is O(1) and correlation-free —
        // works across multi-SAC tx, factory deploys (auth entries), and
        // batch boundaries.
        let sac_asset = if is_sac {
            sac_identities.get(&contract_id).cloned()
        } else {
            None
        };

        let deployer_account = deployer_by_contract
            .get(&contract_id)
            .cloned()
            .or_else(|| Some(tx_source_account.to_string()));

        deployments.push(ExtractedContractDeployment {
            contract_id,
            wasm_hash,
            executable_ref: crate::executable_ref::external_ref_from_instance(data),
            deployer_account,
            deployed_at_ledger: change.ledger_sequence,
            contract_type,
            is_sac,
            sac_asset,
        });
    }

    deployments
}

/// Extract token-metadata writes from contract-instance `created` / `updated`
/// changes that carry a `Symbol("METADATA")` struct in instance storage.
///
/// Reads the typed `change.token_metadata` (populated in `ledger_entry_changes`,
/// chain-verified location — task 0297) rather than re-decoding. Emits one
/// [`ExtractedContractMetadata`] per qualifying change, for the
/// `soroban_contract_metadata` side table (task 0297).
///
/// - `created` + `updated` + `restored` carry the current value and are kept;
///   `state` (pre-image) and `removed` are ignored. `restored` matters because
///   an instance restored from archival is the first time live ingestion may
///   see a contract's METADATA — dropping it would leave a cold-start hole.
/// - **SACs are skipped at extraction**: `entry_token_metadata` already returns
///   `None` for SAC instances (their name/symbol/decimals derive from the asset
///   identity), so a SAC change simply has no `token_metadata` to emit here.
pub fn extract_contract_metadata_writes(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedContractMetadata> {
    let mut out = Vec::new();
    for change in changes {
        // Cheap structural guards first, so we never clone metadata for a
        // change we then drop.
        if change.entry_type != "contract_data" {
            continue;
        }
        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored"
        ) {
            continue;
        }
        if !is_contract_instance_key(&change.key) {
            continue;
        }
        let Some(contract_id) = extract_contract_id_from_key(&change.key) else {
            continue;
        };
        // `None` for SACs (skipped at extraction) and for instances without a
        // METADATA struct — both correctly drop out here.
        let Some(metadata) = change.token_metadata.clone() else {
            continue;
        };
        out.push(ExtractedContractMetadata {
            contract_id,
            metadata,
            ledger: change.ledger_sequence,
        });
    }
    out
}

/// Pull the `contract` StrKey from a ContractData ledger key. Used by
/// `extract_contract_metadata_writes` to dispatch instance-storage METADATA
/// writes to the right contract row.
fn extract_contract_id_from_key(key: &Value) -> Option<String> {
    key.get("contract")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Extract per-holder Soroban token balances from `ContractData`
/// `Balance(Address)` ledger-entry changes (task 0331).
///
/// Reads ledger STATE (the current stored balance), not an event-fold —
/// correct-by-construction for vault / rebasing / non-SEP-41-event tokens where
/// a fold under-counts (README DECISION 2026-06-29). Persisted into the unified
/// `balances` table (task 0331 Option C — the per-type `soroban_token_balances`
/// table was dropped on the pivot).
///
/// Recognises the standard `Vec[Symbol("Balance"), Address]` key with EITHER
/// value shape: a bare `i128` (a type-3 Soroban token balance) OR the SAC
/// `BalanceValue` struct (a contract-held classic/native asset, held via the
/// asset's SAC — task 0331). This extractor emits every balance keyed by the
/// STORING contract; the type distinction is resolved downstream in
/// `build_balance_rows`, which keeps type-3 on its own surrogate but re-keys a
/// SAC-held balance onto the wrapped classic/native asset_id via the `asset_sac`
/// map (ADR 0051 — task 0339 retired the standalone type-2 SAC asset, so a SAC
/// balance now folds onto its type-0/1 row). Any other value shape is skipped,
/// never silently mis-summed.
pub fn extract_soroban_token_balances(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedSorobanBalance> {
    let mut out = Vec::new();
    for change in changes {
        if change.entry_type != "contract_data" {
            continue;
        }
        let Some(holder) = balance_key_holder(&change.key) else {
            continue;
        };
        let Some(contract_id) = extract_contract_id_from_key(&change.key) else {
            continue;
        };
        // `closed` carries what the 0 cannot: the ENTRY is gone, as opposed to a
        // holder who spent down to zero but still has one. ADR 0055.
        let closed = change.change_type == "removed";
        let balance = match change.change_type.as_str() {
            // Holder fully spent / entry archived → 0, so the RMT supersedes the
            // stale positive balance (mirrors trustline-removal → 0).
            "removed" => 0,
            // `created` / `updated` / `restored` carry the current value.
            // `state` (pre-image) is ignored — it shares the change's ledger, so
            // emitting it would let the RMT clobber the real value with the old.
            "created" | "updated" | "restored" => {
                let Some(data) = change.data.as_ref() else {
                    continue;
                };
                // Bare `i128` → type-3 token balance. SAC `BalanceValue` struct →
                // contract-held classic/native balance; take `.amount`. (Its
                // `authorized`/`clawback` flags are decodable but not propagated
                // yet — the frozen-balance policy is open, task 0331.) Any other
                // shape is skipped.
                if let Some(b) = decode_scval_i128(data) {
                    b
                } else if let Some(sac) = decode_sac_balance_value(data) {
                    sac.amount
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        out.push(ExtractedSorobanBalance {
            contract_id,
            holder,
            balance,
            ledger: change.ledger_sequence,
            closed,
        });
    }
    out
}

/// `Some(holder_strkey)` when `key.key` is the standard token balance key
/// `Vec[Symbol("Balance"), Address(holder)]`; `None` otherwise. The holder is
/// a `G…` account or `C…` contract — both are valid `ScAddress` holders.
fn balance_key_holder(key: &Value) -> Option<String> {
    // Token / SAC balances are PERSISTENT contract-data entries. Reject temporary
    // (or missing-durability) entries even when the inner shape matches, so a
    // foreign `Balance(Address)`-shaped temp entry is never summed as a balance.
    if key.get("durability")?.as_str()? != "persistent" {
        return None;
    }
    let inner = key.get("key")?;
    if inner.get("type")?.as_str()? != "vec" {
        return None;
    }
    let elems = inner.get("value")?.as_array()?;
    if elems.len() != 2 {
        return None;
    }
    let tag = &elems[0];
    if tag.get("type")?.as_str()? != "sym" || tag.get("value")?.as_str()? != "Balance" {
        return None;
    }
    let holder = &elems[1];
    if holder.get("type")?.as_str()? != "address" {
        return None;
    }
    Some(holder.get("value")?.as_str()?.to_string())
}

/// Decode `data.val` as a bare `i128` (the standard token balance value shape).
fn decode_scval_i128(data: &Value) -> Option<i128> {
    let val = data.get("val")?;
    if val.get("type")?.as_str()? != "i128" {
        return None;
    }
    val.get("value")?.as_str()?.parse::<i128>().ok()
}

/// The SAC `BalanceValue` struct — how a CONTRACT holds a classic/native asset.
///
/// A contract has no trustline; it holds a classic (type-1) or native (type-0) asset
/// as a `Balance(Address)` `ContractData` entry **inside that asset's SAC**, and the
/// value is this struct — NOT the bare `i128` a bespoke Soroban token (type-3) uses.
/// `scval_to_typed_json` serializes it as a `map` of symbol→value entries. Task 0331
/// (contract-held 0/1). `authorized`/`clawback` are carried so a later step can decide
/// whether a deauthorized/frozen balance counts toward supply/holders (open policy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SacBalanceValue {
    pub amount: i128,
    pub authorized: bool,
    pub clawback: bool,
}

/// Decode `data.val` as a SAC `BalanceValue` struct. `None` for any other shape —
/// including the bare-`i128` type-3 balance and a partial/foreign map — so the two
/// value shapes never cross-decode. Does NOT assign an asset: mapping the SAC contract
/// back to its classic/native asset (type-0/1) is the caller's job (task 0331 Problem B).
pub fn decode_sac_balance_value(data: &Value) -> Option<SacBalanceValue> {
    let val = data.get("val")?;
    if val.get("type")?.as_str()? != "map" {
        return None;
    }
    let mut amount = None;
    let mut authorized = None;
    let mut clawback = None;
    for entry in val.get("value")?.as_array()? {
        let key = entry.get("key")?;
        if key.get("type")?.as_str()? != "sym" {
            return None;
        }
        let field = entry.get("value")?;
        match key.get("value")?.as_str()? {
            "amount" => {
                if field.get("type")?.as_str()? != "i128" {
                    return None;
                }
                amount = Some(field.get("value")?.as_str()?.parse::<i128>().ok()?);
            }
            "authorized" => authorized = field.get("value")?.as_bool(),
            "clawback" => clawback = field.get("value")?.as_bool(),
            // Strict: an unknown symbol key means this is NOT the SAC
            // `BalanceValue` struct → reject, never partial-decode a foreign map.
            _ => return None,
        }
    }
    Some(SacBalanceValue {
        amount: amount?,
        authorized: authorized?,
        clawback: clawback?,
    })
}

fn is_contract_instance_key(key: &Value) -> bool {
    let key_field = key.get("key");
    match key_field {
        Some(k) => k
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t == "ledger_key_contract_instance"),
        None => false,
    }
}

fn is_sac_from_data(data: &Value) -> bool {
    data.get("val")
        .and_then(|v| v.get("value"))
        .and_then(|v| v.get("executable"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .is_some_and(|t| t == "stellar_asset")
}

fn extract_wasm_hash(data: &Value) -> Option<String> {
    data.get("val")
        .and_then(|v| v.get("value"))
        .and_then(|v| v.get("executable"))
        .and_then(|v| v.get("hash"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Step 2: Account State Extraction
// ---------------------------------------------------------------------------

/// Convert raw stroops (i64) to Stellar-standard decimal string with 7 decimal places.
/// Example: 10_000_000 → "1.0000000", 1234 → "0.0001234"
fn format_stroops(stroops: i64) -> String {
    let whole = stroops / 10_000_000;
    let frac = (stroops % 10_000_000).unsigned_abs();
    format!("{whole}.{frac:07}")
}

/// Extract account states from ledger entry changes.
///
/// Processes both `account` and `trustline` entry types. Account entries provide
/// native XLM balance, sequence number, and home domain. Trustline entries provide
/// non-native asset balances (credit_alphanum4, credit_alphanum12).
///
/// Within a single transaction's changes, entries are merged by `account_id` so that
/// the output contains at most one `ExtractedAccountState` per account.
///
/// Trustline-only changes (no account entry in the same tx) produce an entry with
/// `sequence_number = -1` (sentinel), signalling the SQL layer to preserve the
/// existing value.
pub fn extract_account_states(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedAccountState> {
    use std::collections::HashMap;

    #[derive(Default)]
    struct AccountAccum {
        native_balance: Option<i64>,
        sequence_number: Option<i64>,
        home_domain: Option<String>,
        is_creation: bool,
        ledger_sequence: u32,
        created_at: i64,
        trustline_balances: Vec<Value>,
        removed_trustlines: Vec<Value>,
        /// Set by a `removed` account entry, cleared by any later
        /// created/updated/restored one — merge-then-recreate must not leave
        /// the account marked closed. ADR 0055.
        account_removed: bool,
        /// Some = an AccountEntry was observed (full-set semantics — an empty
        /// vec is a real "no signers" state). None = trustline-only accum;
        /// no signers row may be emitted. lore-0463.
        signers: Option<Vec<Value>>,
        thresholds: Option<String>,
        flags: Option<u32>,
    }

    let mut map: HashMap<String, AccountAccum> = HashMap::new();

    // Pass 1: account entries
    for change in changes {
        if change.entry_type != "account" {
            continue;
        }

        // AccountMerge tombstone (task 0295): a `removed` account entry is the
        // only way an account is deleted on Stellar. Emit native balance=0 at
        // the merge ledger so the stale balance row is superseded (the balances
        // table is RMT keyed on the higher ledger). account_id comes from the
        // change key — removed entries carry no data. Identity columns are not
        // set here; the separate RMT whole-row clobber is tracked in lore-0316.
        if change.change_type == "removed" {
            let account_id = change
                .key
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if account_id.is_empty() {
                continue;
            }
            let entry = map.entry(account_id).or_insert_with(|| AccountAccum {
                ledger_sequence: change.ledger_sequence,
                created_at: change.created_at,
                ..Default::default()
            });
            entry.native_balance = Some(0);
            entry.account_removed = true;
            entry.ledger_sequence = change.ledger_sequence;
            entry.created_at = change.created_at;
            continue;
        }

        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored"
        ) {
            continue;
        }
        let Some(ref data) = change.data else {
            continue;
        };

        let account_id = data
            .get("account_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if account_id.is_empty() {
            continue;
        }

        let balance = data.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
        let seq = data.get("seq_num").and_then(|v| v.as_i64()).unwrap_or(0);
        let hd = data
            .get("home_domain")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let is_creation = matches!(change.change_type.as_str(), "created" | "restored");

        let entry = map.entry(account_id).or_insert_with(|| AccountAccum {
            ledger_sequence: change.ledger_sequence,
            created_at: change.created_at,
            ..Default::default()
        });
        entry.native_balance = Some(balance);
        entry.sequence_number = Some(seq);
        if hd.is_some() {
            entry.home_domain = hd;
        }
        // A live entry supersedes any removal seen earlier in this change set —
        // merge-then-recreate within one ledger must not stay marked closed.
        entry.account_removed = false;
        // Full-set semantics: the entry carries the COMPLETE signer list, so a
        // missing/empty array is a real "no signers" state, not absence of
        // data. Master is not in this list (thresholds byte 0). lore-0463.
        entry.signers = Some(
            data.get("signers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        entry.thresholds = data
            .get("thresholds")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.flags = data.get("flags").and_then(Value::as_u64).map(|f| f as u32);
        entry.is_creation = entry.is_creation || is_creation;
        entry.ledger_sequence = change.ledger_sequence;
        entry.created_at = change.created_at;
    }

    // Pass 2: trustline entries
    for change in changes {
        if change.entry_type != "trustline" {
            continue;
        }

        match change.change_type.as_str() {
            "created" | "updated" | "restored" => {
                let Some(ref data) = change.data else {
                    continue;
                };
                let account_id = data
                    .get("account_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if account_id.is_empty() {
                    continue;
                }

                let balance = data.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
                let asset = data.get("asset");

                let trustline_entry = match asset {
                    Some(Value::Object(obj)) => {
                        let asset_type = obj
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        // pool_share trustlines are LP positions, not asset
                        // balances — handled by the sibling producer
                        // `extract_lp_positions` (task 0162). Skipping here
                        // is intentional, not a data drop.
                        if asset_type == "pool_share" {
                            continue;
                        }
                        let code = obj.get("code").and_then(|v| v.as_str()).unwrap_or("");
                        let issuer = obj.get("issuer").and_then(|v| v.as_str()).unwrap_or("");
                        serde_json::json!({
                            "asset_type": asset_type,
                            "asset_code": code,
                            "issuer": issuer,
                            "balance": format_stroops(balance),
                        })
                    }
                    // Native trustlines shouldn't exist; skip
                    _ => continue,
                };

                let entry = map.entry(account_id).or_insert_with(|| AccountAccum {
                    ledger_sequence: change.ledger_sequence,
                    created_at: change.created_at,
                    ..Default::default()
                });

                // Dedup: remove existing entry for same asset, then add new
                let new_code = trustline_entry.get("asset_code").cloned();
                let new_issuer = trustline_entry.get("issuer").cloned();
                entry.trustline_balances.retain(|tb| {
                    tb.get("asset_code") != new_code.as_ref()
                        || tb.get("issuer") != new_issuer.as_ref()
                });
                // Cancel any prior removal for the same asset (remove-then-recreate in same tx)
                entry.removed_trustlines.retain(|rt| {
                    rt.get("asset_code") != new_code.as_ref()
                        || rt.get("issuer") != new_issuer.as_ref()
                });
                entry.trustline_balances.push(trustline_entry);

                if change.ledger_sequence >= entry.ledger_sequence {
                    entry.ledger_sequence = change.ledger_sequence;
                    entry.created_at = change.created_at;
                }
            }
            "removed" => {
                // Trustline removed — extract account_id and asset from the key
                let account_id = change
                    .key
                    .get("account_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if account_id.is_empty() {
                    continue;
                }

                let asset = change.key.get("asset");
                let removal_key = match asset {
                    Some(Value::Object(obj)) => {
                        let asset_type = obj
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        // pool_share removal is handled by `extract_lp_positions`
                        // (task 0162) which emits a zero-shares row from the
                        // change.key; skipping here keeps account-state focus.
                        if asset_type == "pool_share" {
                            continue;
                        }
                        let code = obj.get("code").and_then(|v| v.as_str()).unwrap_or("");
                        let issuer = obj.get("issuer").and_then(|v| v.as_str()).unwrap_or("");
                        serde_json::json!({
                            "asset_type": asset_type,
                            "asset_code": code,
                            "issuer": issuer,
                        })
                    }
                    _ => continue,
                };

                let entry = map.entry(account_id).or_insert_with(|| AccountAccum {
                    ledger_sequence: change.ledger_sequence,
                    created_at: change.created_at,
                    ..Default::default()
                });

                // Also remove from trustline_balances if it was added in same tx
                let rm_code = removal_key.get("asset_code");
                let rm_issuer = removal_key.get("issuer");
                entry
                    .trustline_balances
                    .retain(|tb| tb.get("asset_code") != rm_code || tb.get("issuer") != rm_issuer);
                entry.removed_trustlines.push(removal_key);

                if change.ledger_sequence >= entry.ledger_sequence {
                    entry.ledger_sequence = change.ledger_sequence;
                    entry.created_at = change.created_at;
                }
            }
            _ => continue,
        }
    }

    // Build results
    map.into_iter()
        .map(|(account_id, accum)| {
            let mut balances_arr: Vec<Value> = Vec::new();
            if let Some(native) = accum.native_balance {
                balances_arr.push(
                    serde_json::json!({"asset_type": "native", "balance": format_stroops(native)}),
                );
            }
            balances_arr.extend(accum.trustline_balances);

            ExtractedAccountState {
                account_id,
                first_seen_ledger: if accum.is_creation {
                    Some(accum.ledger_sequence)
                } else {
                    None
                },
                last_seen_ledger: accum.ledger_sequence,
                sequence_number: accum.sequence_number.unwrap_or(-1),
                balances: Value::Array(balances_arr),
                removed_trustlines: accum.removed_trustlines,
                account_removed: accum.account_removed,
                signers: accum.signers,
                thresholds: accum.thresholds,
                flags: accum.flags,
                home_domain: accum.home_domain,
                created_at: accum.created_at,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Step 3 + Step 4: Liquidity Pool State + Snapshots
// ---------------------------------------------------------------------------

/// Extract liquidity pool states and snapshots from ledger entry changes.
///
/// What each change means, per stellar-core (`LedgerTxn::getChanges`: a
/// `STATE` is always followed by the `UPDATED` or `REMOVED` of the same key,
/// and `CREATED` stands alone) and confirmed on 9,865 mainnet pool changes
/// (task 0210, 2026-09-17: 0 lone `state`):
///
/// | change | snapshot | pool row |
/// |---|---|---|
/// | `created` / `restored` | its reserves | its params, creation ledger |
/// | `updated` | its reserves | its params |
/// | `state` | none | none — it is the image at the START of the operation, never a value |
/// | `removed` | reserves and shares 0 | params of the `state` before it |
///
/// A `removed` has no data, and the `state` before it is not the pool just
/// before removal: when an issuer revokes the last holder's authorization,
/// core redeems the shares, zeroes the reserves and erases the pool inside one
/// operation, and the meta carries only the start-of-operation `state` (full
/// reserves) and the `removed`. Storing that `state` left 1,381 erased pools
/// with their old reserves. A pool that does not exist holds nothing, so its
/// snapshot at that ledger is zero. The pool row still comes from the `state`
/// params (fixed for a pool id), because for a pool created before our history
/// the removal can be its only appearance.
///
/// A `state` with no following `updated` / `removed` cannot happen; one is
/// logged and ignored.
pub fn extract_liquidity_pools(
    changes: &[ExtractedLedgerEntryChange],
) -> (
    Vec<ExtractedLiquidityPool>,
    Vec<ExtractedLiquidityPoolSnapshot>,
) {
    let mut pools = Vec::new();
    let mut snapshots = Vec::new();
    // The `state` image the next change of the same pool pairs with.
    let mut before: HashMap<String, &Value> = HashMap::new();

    for change in changes {
        if change.entry_type != "liquidity_pool" {
            continue;
        }
        match change.change_type.as_str() {
            "state" => {
                if let Some(data) = &change.data
                    && let Some(pool_id) = data.get("pool_id").and_then(Value::as_str)
                {
                    before.insert(pool_id.to_string(), data);
                }
            }
            "removed" => {
                let Some(pool_id) = change.key.get("pool_id").and_then(Value::as_str) else {
                    continue;
                };
                match before.remove(pool_id) {
                    Some(data) => pools.push(pool_row(pool_id, data, change, false)),
                    None => warn!(
                        pool_id,
                        ledger = change.ledger_sequence,
                        "liquidity pool removed without a preceding state change; \
                         writing its zero snapshot, no pool row"
                    ),
                }
                snapshots.push(ExtractedLiquidityPoolSnapshot {
                    pool_id: pool_id.to_string(),
                    ledger_sequence: change.ledger_sequence,
                    created_at: change.created_at,
                    reserves: serde_json::json!({ "a": 0, "b": 0 }),
                    total_shares: "0".to_string(),
                });
            }
            "created" | "updated" | "restored" => {
                let Some(data) = &change.data else { continue };
                let Some(pool_id) = data.get("pool_id").and_then(Value::as_str) else {
                    continue;
                };
                before.remove(pool_id);
                let is_creation = change.change_type != "updated";
                let pool = pool_row(pool_id, data, change, is_creation);
                snapshots.push(ExtractedLiquidityPoolSnapshot {
                    pool_id: pool.pool_id.clone(),
                    ledger_sequence: change.ledger_sequence,
                    created_at: change.created_at,
                    reserves: pool.reserves.clone(),
                    total_shares: pool.total_shares.clone(),
                });
                pools.push(pool);
            }
            _ => {}
        }
    }
    for pool_id in before.keys() {
        warn!(
            pool_id = pool_id.as_str(),
            "liquidity pool state change with no following updated or removed; ignored"
        );
    }

    (pools, snapshots)
}

/// The pool row one change carries: params from `data`, reserves from `data`
/// unless the pool was removed (then 0), versioned on the change's ledger.
fn pool_row(
    pool_id: &str,
    data: &Value,
    change: &ExtractedLedgerEntryChange,
    is_creation: bool,
) -> ExtractedLiquidityPool {
    let removed = change.change_type == "removed";
    let params = data.get("params").cloned().unwrap_or(serde_json::json!({}));
    let reserve = |field| {
        if removed {
            0
        } else {
            data.get(field).and_then(Value::as_i64).unwrap_or(0)
        }
    };
    let total_shares = if removed {
        0
    } else {
        data.get("total_pool_shares")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    ExtractedLiquidityPool {
        pool_id: pool_id.to_string(),
        asset_a: params.get("asset_a").cloned().unwrap_or(Value::Null),
        asset_b: params.get("asset_b").cloned().unwrap_or(Value::Null),
        fee_bps: params.get("fee").and_then(Value::as_i64).unwrap_or(0) as i32,
        reserves: serde_json::json!({ "a": reserve("reserve_a"), "b": reserve("reserve_b") }),
        total_shares: total_shares.to_string(),
        created_at_ledger: is_creation.then_some(change.ledger_sequence),
        last_updated_ledger: change.ledger_sequence,
        created_at: change.created_at,
    }
}

/// Collapse pool snapshots to exactly one per `(pool_id, ledger_sequence)`: the
/// LAST one in ledger apply order, i.e. the end-of-ledger (final) reserves.
///
/// Producers push a snapshot for every value-carrying pool change in apply
/// order (transaction order, then operation order), so the last snapshot for a
/// `(pool, ledger)` is the pool's committed state at ledger close — the final
/// `updated` after-image, or the zero snapshot of a `removed`. Deduping here
/// makes the stored snapshot a deterministic function of the ledger
/// (re-ingesting the same ledger yields the same row) instead of leaving "one
/// row per (pool, ledger)" to CH's version-less `ReplacingMergeTree`, which
/// would otherwise keep an arbitrary intra-ledger image. See lore-0356.
///
/// Call once per ledger, after aggregating every transaction's snapshots
/// (`crate::fold::keep_last_by_key` carries the shared mechanism and the
/// belt-and-braces rationale for the `ledger_sequence` key component).
pub fn dedup_final_pool_snapshots(
    snapshots: Vec<ExtractedLiquidityPoolSnapshot>,
) -> Vec<ExtractedLiquidityPoolSnapshot> {
    crate::fold::keep_last_by_key(snapshots, |s| (s.pool_id.clone(), s.ledger_sequence))
}

// ---------------------------------------------------------------------------
// Step 4b: Liquidity-pool participant positions (task 0162)
// ---------------------------------------------------------------------------

/// Extract LP participant positions from `pool_share` trustline changes.
///
/// `extract_account_states` skips `pool_share` trustlines on purpose —
/// they are not classic asset balances and do not belong in the per-account
/// trustline_balances JSON. They DO encode `(account, pool_id, share balance)`
/// triples that the `lp_positions` table is shaped for, so this sibling fn
/// produces them as `ExtractedLpPosition` records on the same `changes`
/// slice. Two passes over `changes` is intentional: keeps each producer fn
/// single-purpose and matches the existing one-fn-per-output-type idiom in
/// this module.
///
/// Change-type semantics:
///
/// - `created` → emit with `first_deposit_ledger = Some(ledger_sequence)`;
///   staging layer COALESCEs to keep the original on subsequent updates.
/// - `updated` / `restored` → emit with `first_deposit_ledger = None`.
/// - `removed` → emit with `shares = "0.0000000"` and
///   `first_deposit_ledger = None`. Persist layer (task 0126) decides
///   whether zero-share rows are pruned or kept as historical
///   participant records — this fn just reports the data.
///
/// `state` change_type is observation-only (no balance change) and is
/// skipped here, matching the trustline path in `extract_account_states`.
pub fn extract_lp_positions(changes: &[ExtractedLedgerEntryChange]) -> Vec<ExtractedLpPosition> {
    let mut positions = Vec::new();

    for change in changes {
        if change.entry_type != "trustline" {
            continue;
        }

        // The pool-share trustline is gone (participant left) versus withdrawn
        // to zero but still open — both write `shares = 0`. ADR 0055.
        let closed = change.change_type == "removed";
        let (asset_holder, account_id, shares, first_deposit) = match change.change_type.as_str() {
            "created" | "updated" | "restored" => {
                let Some(ref data) = change.data else {
                    continue;
                };
                let Some(account_id) = data.get("account_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(asset) = data.get("asset") else {
                    continue;
                };
                let balance = data.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
                let first_deposit = if change.change_type == "created" {
                    Some(change.ledger_sequence)
                } else {
                    None
                };
                (
                    asset.clone(),
                    account_id.to_string(),
                    format_stroops(balance),
                    first_deposit,
                )
            }
            "removed" => {
                let Some(account_id) = change.key.get("account_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(asset) = change.key.get("asset") else {
                    continue;
                };
                (
                    asset.clone(),
                    account_id.to_string(),
                    format_stroops(0),
                    None,
                )
            }
            _ => continue,
        };

        let Some(asset_obj) = asset_holder.as_object() else {
            continue;
        };
        if asset_obj.get("type").and_then(|v| v.as_str()) != Some("pool_share") {
            continue;
        }
        let Some(pool_id) = asset_obj.get("pool_id").and_then(|v| v.as_str()) else {
            continue;
        };

        positions.push(ExtractedLpPosition {
            pool_id: pool_id.to_string(),
            account_id,
            shares,
            first_deposit_ledger: first_deposit,
            last_updated_ledger: change.ledger_sequence,
            closed,
        });
    }

    positions
}

// ---------------------------------------------------------------------------
// Step 5: Asset Detection
// ---------------------------------------------------------------------------

/// Detect assets from contract deployments.
///
/// Two paths produce an [`ExtractedAsset`]:
///
/// 1. **SAC deployments** — folded onto the underlying asset as a FACET
///    (ADR 0051): a `classic_credit` (type 1) or `native` (type 0) row with the
///    SAC handle in `sac_contract_id` (the deploy's derived `C…`) + `sac_deployed
///    = true`; the key `contract_id` stays unset (reserved for soroban identity).
///    Identity comes from `deployment.sac_asset` (resolved from
///    `ContractIdPreimage::FromAsset` via `crate::sac::extract_sac_identities`
///    in the indexer). Two shapes:
///    - `Credit { code, issuer }` → the classic_credit row for that pair.
///    - `Native` → the native (type 0) row (NULL code/issuer).
///    - `None` (SAC deployment whose creating preimage is not in this
///      batch) is logged as a warn and skipped — better to lose one row
///      than fabricate identity.
/// 2. **WASM-based deployments classifying as
///    [`ContractClassification::Fungible`]** — [`AssetFamily::Soroban`]
///    row; identity is `contract_id` only. Classification uses
///    [`classify_contract_from_wasm_spec`] against the deployment's WASM
///    interface function list.
///
/// NFT-classified contracts (SEP-0050 surface: `owner_of`, `token_uri`, …)
/// do **not** produce an assets row — they live in the `nfts` table via
/// the NFT pipeline (task 0118). `Other`-classified contracts also produce
/// no row: a later WASM upload may promote them, in which case the
/// reclassification write step backfills the missing assets row
/// (`write::insert_assets_from_reclassified_contracts`, task 0120).
///
/// Soroban rows carry identity only. On-chain name/symbol extraction from
/// ContractData storage entries is tracked as follow-up task 0156; a separate
/// scheduled-Lambda enrichment path for SEP-1 metadata lives under task 0124.
/// Supply/holders are not a parser concern at all — they are aggregated from
/// `balances` into `balance_aggregates` (0293/0331).
pub fn detect_assets(
    deployments: &[ExtractedContractDeployment],
    interfaces: &[ExtractedContractInterface],
) -> Vec<ExtractedAsset> {
    // Pre-index interfaces by wasm_hash so the inner loop is O(1) per
    // deployment. Classification itself is O(|functions|) but amortised
    // across all deployments sharing that wasm_hash (shared-library
    // contracts are common on Stellar), so cache the verdict too.
    use std::collections::HashMap;
    let mut verdict_by_hash: HashMap<&str, ContractClassification> =
        HashMap::with_capacity(interfaces.len());
    for iface in interfaces {
        verdict_by_hash
            .entry(iface.wasm_hash.as_str())
            .or_insert_with(|| classify_contract_from_wasm_spec(&iface.functions));
    }

    let mut assets = Vec::new();
    for deployment in deployments {
        if deployment.is_sac {
            // ADR 0051: a SAC is a FACET of its underlying classic_credit /
            // native asset, not a separate `asset_type`. Emit the underlying
            // asset row and record the SAC handle (`deployment.contract_id` is
            // the SAC's derived `C…` StrKey) + deployed=true in the facet
            // columns. Identity from the typed enum produced by the parser:
            //   Native             → the native (type=0) row, no code/issuer.
            //   Credit{code,issuer}→ the classic_credit (type=1) row.
            //   None               → preimage not in this batch; skip with
            //                        a warn rather than fabricate identity.
            let (asset_type, asset_code, issuer_address) = match &deployment.sac_asset {
                Some(SacAssetIdentity::Native) => (AssetFamily::Native, None, None),
                Some(SacAssetIdentity::Credit { code, issuer }) => (
                    AssetFamily::ClassicCredit,
                    Some(code.clone()),
                    Some(issuer.clone()),
                ),
                None => {
                    warn!(
                        contract_id = %deployment.contract_id,
                        "SAC deployment without resolved asset identity; skipping assets row"
                    );
                    continue;
                }
            };
            assets.push(ExtractedAsset {
                asset_type,
                asset_code,
                issuer_address,
                // Key `contract_id` stays reserved for soroban identity — the
                // SAC handle lives in the facet column, keeping this classic /
                // native row on its stable identity key (ORDER BY value 0).
                contract_id: None,
                sac_contract_id: Some(deployment.contract_id.clone()),
                sac_deployed: true,
            });
            continue;
        }

        // Non-SAC: classify by WASM spec. Requires a matching interface in
        // this batch — if absent (e.g. contract deployed in a prior ledger
        // whose WASM only arrives later), skip here; the late-WASM bridge
        // in the persist layer picks it up once classification succeeds.
        let Some(wasm_hash) = deployment.wasm_hash.as_deref() else {
            continue;
        };
        if verdict_by_hash.get(wasm_hash) == Some(&ContractClassification::Fungible) {
            assets.push(ExtractedAsset {
                asset_type: AssetFamily::Soroban,
                asset_code: None,
                issuer_address: None,
                contract_id: Some(deployment.contract_id.clone()),
                // Bespoke Soroban token — no classic backing, so no SAC facet.
                sac_contract_id: None,
                sac_deployed: false,
            });
        }
    }

    assets
}

/// Detect **classic-credit** assets from observed trustline
/// `LedgerEntryChange`s (task 0219 — Karol's pre-audit Bug #1).
///
/// `detect_assets` above only emits SAC + Soroban-fungible rows from
/// observed contract deployments. Classic credits (USDC, AQUA, EURC,
/// every asset issued by a G-account) need their own producer because
/// no deployment-shaped observation carries their identity — the
/// authoritative carrier is the `trustline` `LedgerEntryChange`'s
/// `data.asset` field, which holds `{type, code, issuer}` for every
/// `credit_alphanum4` / `credit_alphanum12` asset.
///
/// Flow:
///
/// 1. Walk `changes` looking for `entry_type == "trustline"`.
/// 2. Read the asset payload from `data.asset` (live changes) or
///    fall back to `key.asset` (removed changes — `data` is `None`,
///    but the change's key still carries `{type, code, issuer}` per
///    `format_trustline_asset_key` on the ingest side). The fallback
///    matters for partial-window backfills whose first observation
///    of a `(code, issuer)` pair is a trustline removal.
/// 3. Skip `asset.type == "pool_share"` — those are LP positions,
///    handled by `extract_lp_positions`.
/// 4. Extract `(code, issuer)`; emit one `ExtractedAsset { asset_type:
///    ClassicCredit, asset_code: code, issuer_address: issuer }` per
///    distinct pair (dedup within this call).
///
/// The row carries identity only. `name` for classic credits lands via
/// Lambda 2's `sep1_assets` enrichment path (task 0195 §2a) — runtime SEP-1
/// stellar.toml fetch keyed on the `(code, issuer)` pair. Supply/holders are
/// aggregated from `balances` into `balance_aggregates` (0293/0331), never
/// written back onto this row.
///
/// The function is pure (no I/O, no DB) and idempotent on replay.
/// Downstream dedup in `Staged::prepare`
/// (`crates/indexer/src/handler/persist/staging.rs`, local
/// `asset_rows` accumulator keyed by the per-`asset_type` fingerprint)
/// already collapses same `(code, issuer)` from multiple sources to
/// one row before the `upsert_assets_classic_like` INSERT fires.
pub fn detect_classic_credit_assets(changes: &[ExtractedLedgerEntryChange]) -> Vec<ExtractedAsset> {
    use std::collections::HashSet;
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut assets: Vec<ExtractedAsset> = Vec::new();

    for change in changes {
        if change.entry_type != "trustline" {
            continue;
        }

        // Live changes (created / updated / restored / state) carry the
        // asset on `data.asset`. Removed changes have `data: None`, but
        // the change's `key.asset` still carries `{type, code, issuer}`
        // — so a partial-window backfill whose first observation is a
        // trustline removal still emits the asset row.
        let asset_source = change
            .data
            .as_ref()
            .and_then(|d| d.get("asset"))
            .or_else(|| change.key.get("asset"));
        let Some(asset) = asset_source.and_then(Value::as_object) else {
            continue;
        };
        let asset_type = asset.get("type").and_then(Value::as_str).unwrap_or("");
        // `pool_share` trustlines are LP positions, not asset balances —
        // handled by `extract_lp_positions`. Skipping here is intentional,
        // not a data drop.
        if asset_type == "pool_share" {
            continue;
        }
        let code = asset.get("code").and_then(Value::as_str).unwrap_or("");
        let issuer = asset.get("issuer").and_then(Value::as_str).unwrap_or("");
        if code.is_empty() || issuer.is_empty() {
            // Malformed trustline (shouldn't happen on mainnet); skip
            // rather than fabricate a partial-identity row.
            continue;
        }

        let key = (code.to_string(), issuer.to_string());
        if !seen.insert(key) {
            continue;
        }
        assets.push(ExtractedAsset {
            asset_type: AssetFamily::ClassicCredit,
            asset_code: Some(code.to_string()),
            issuer_address: Some(issuer.to_string()),
            contract_id: None,
            // A trustline observation carries no SAC signal; if this asset has
            // a SAC, the deploy/override path folds it onto this same row.
            sac_contract_id: None,
            sac_deployed: false,
        });
    }

    assets
}

/// Native XLM singleton bootstrap (task 0219 — Karol's pre-audit Bug #1).
///
/// Returns a single `ExtractedAsset { asset_type: Native }` row. The
/// indexer emits this once per ledger; the persist path
/// (`upsert_assets_native`) inserts via `WHERE NOT EXISTS` against
/// `uidx_assets_native`, so every call after the first is a no-op.
pub fn native_asset_singleton() -> ExtractedAsset {
    ExtractedAsset {
        asset_type: AssetFamily::Native,
        asset_code: None,
        issuer_address: None,
        contract_id: None,
        // XLM's SAC facet is folded on by the deploy/override path when seen.
        sac_contract_id: None,
        sac_deployed: false,
    }
}

// ---------------------------------------------------------------------------
// Step 6: NFT Detection
// ---------------------------------------------------------------------------

/// Detect NFTs from NFT events (produced by task 0026's `detect_nft_events`).
///
/// Converts `NftEvent` records into `ExtractedNft` entities for DB persistence.
pub fn detect_nfts(nft_events: &[NftEvent]) -> Vec<ExtractedNft> {
    let mut nfts = Vec::new();

    for event in nft_events {
        let token_id = token_id_to_string(&event.token_id);
        if token_id.is_empty() {
            continue;
        }

        let (owner_account, minted_at_ledger) = match event.event_kind.as_str() {
            "mint" => (event.to.clone(), Some(event.ledger_sequence)),
            "transfer" => (event.to.clone(), None),
            "burn" => (None, None),
            _ => continue,
        };

        nfts.push(ExtractedNft {
            contract_id: event.contract_id.clone(),
            token_id,
            collection_name: None,
            owner_account,
            name: None,
            media_url: None,
            minted_at_ledger,
            last_seen_ledger: event.ledger_sequence,
            created_at: event.created_at,
        });
    }

    nfts
}

/// Convert an NftEvent token_id JSON value to a string key for the DB.
fn token_id_to_string(token_id: &Value) -> String {
    if let Some(v) = token_id.get("value") {
        if v.is_null() {
            return String::new();
        }
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        if let Some(n) = v.as_i64() {
            return n.to_string();
        }
        if let Some(n) = v.as_u64() {
            return n.to_string();
        }
        return v.to_string();
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Step 6b: NFT Ownership Event Extraction (task 0202)
// ---------------------------------------------------------------------------

/// Transform raw parser `NftEvent` records into schema-shaped
/// `ExtractedNftEvent` rows for `nft_ownership`.
///
/// The parser (`detect_nft_events`) emits events with a JSON-typed
/// `token_id`, string `event_kind` ("mint"/"transfer"/"burn"), and split
/// `from`/`to` fields. The persistence layer expects a stringified
/// `token_id`, the `NftEventType` enum, and a unified `owner_account`
/// field (`Some(to)` for mint/transfer, `None` for burn).
///
/// Additionally, this fn computes `event_order` — a per-`(contract, token,
/// ledger)` monotonic ordinal (SMALLINT) required by the schema PK
/// `(nft_id, created_at, ledger_sequence, event_order)` and by the
/// LEAD-window pagination in `17_get_nfts_transfers.sql`.
///
/// Events with empty `token_id` are skipped (matches `detect_nfts`
/// behaviour). Events with `event_kind` not in {"mint","transfer","burn"}
/// are skipped — the parser already restricts emission to these three
/// kinds, so the guard is defensive.
///
/// Pathological-input guard: `event_order` is persisted as SMALLINT so
/// the schema bound is `i16::MAX = 32_767`. Once a single
/// `(contract, token, ledger)` triple has already produced that many
/// rows, further events for the same triple are skipped with a warn
/// instead of overflowing the staging `try_into::<i16>()` and failing
/// the whole ledger. No real NFT contract reaches this bound; the cap
/// exists to keep ingestion robust against a malicious / buggy
/// contract emitting tens of thousands of events for one NFT in a
/// single ledger.
#[instrument(skip(events), fields(event_count = events.len()))]
pub fn extract_nft_ownership_events(events: &[NftEvent]) -> Vec<ExtractedNftEvent> {
    /// SMALLINT max — `nft_ownership.event_order` is stored as i16 in PG.
    const MAX_EVENT_ORDER: u16 = i16::MAX as u16;

    let mut order_counter: HashMap<(String, String, u32), u16> = HashMap::new();
    let mut out: Vec<ExtractedNftEvent> = Vec::with_capacity(events.len());

    for event in events {
        let token_id = token_id_to_string(&event.token_id);
        if token_id.is_empty() {
            continue;
        }

        let event_type = match event.event_kind.parse::<NftEventType>() {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    event_kind = %event.event_kind,
                    error = %e,
                    "unknown NFT event_kind — skipping (parser should not emit this)"
                );
                continue;
            }
        };

        let owner_account = match event_type {
            NftEventType::Mint | NftEventType::Transfer => event.to.clone(),
            NftEventType::Burn => None,
        };

        let key = (
            event.contract_id.clone(),
            token_id.clone(),
            event.ledger_sequence,
        );
        let counter = order_counter.entry(key).or_insert(0);
        if *counter > MAX_EVENT_ORDER {
            warn!(
                contract_id = %event.contract_id,
                token_id = %token_id,
                ledger_sequence = event.ledger_sequence,
                max = MAX_EVENT_ORDER,
                "event_order would exceed SMALLINT range; skipping further events for triple"
            );
            continue;
        }
        let event_order = *counter;
        *counter = counter.saturating_add(1);

        out.push(ExtractedNftEvent {
            transaction_hash: event.transaction_hash.clone(),
            contract_id: event.contract_id.clone(),
            token_id,
            event_type,
            owner_account,
            event_order,
            ledger_sequence: event.ledger_sequence,
            created_at: event.created_at,
        });
    }

    out
}

#[cfg(test)]
mod tests;
