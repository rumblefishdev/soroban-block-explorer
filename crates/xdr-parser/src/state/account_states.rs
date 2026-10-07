//! Step 2 of state extraction: per-account state (native balance, sequence,
//! home domain, signers, thresholds, flags) and trustline balances, from
//! `account` and `trustline` ledger entry changes.

use super::*;

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
        num_sponsoring: Option<u32>,
        num_sponsored: Option<u32>,
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
        entry.num_sponsoring = data
            .get("num_sponsoring")
            .and_then(Value::as_u64)
            .map(|n| n as u32);
        entry.num_sponsored = data
            .get("num_sponsored")
            .and_then(Value::as_u64)
            .map(|n| n as u32);
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
                num_sponsoring: accum.num_sponsoring,
                num_sponsored: accum.num_sponsored,
                home_domain: accum.home_domain,
                created_at: accum.created_at,
            }
        })
        .collect()
}
