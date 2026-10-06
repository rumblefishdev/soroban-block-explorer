//! The `account_entry_state` row staged from one observed `AccountEntry`:
//! signers, thresholds and flags, whole-set per account (lore-0463).
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use serde_json::Value;
use xdr_parser::types::ExtractedAccountState;

use crate::persist::rows::AccountEntryStateRow;

/// The account's signing configuration as one row, versioned at `watermark`.
/// `None` when the thresholds hex is malformed — the row is skipped, never
/// fabricated.
pub(super) fn entry_state_row(
    st: &ExtractedAccountState,
    th_hex: &str,
    account_id_int: i64,
    watermark: i64,
) -> Option<AccountEntryStateRow> {
    match parse_thresholds(th_hex) {
        Some([master_weight, threshold_low, threshold_med, threshold_high]) => {
            let signers = st.signers.as_deref().unwrap_or_default();
            let mut signer_keys = Vec::with_capacity(signers.len());
            let mut signer_weights = Vec::with_capacity(signers.len());
            let mut signer_types = Vec::with_capacity(signers.len());
            for sg in signers {
                let key = sg.get("key").and_then(Value::as_str).unwrap_or("");
                let weight = sg.get("weight").and_then(Value::as_u64).unwrap_or(0) as u32;
                let typ = sg.get("type").and_then(Value::as_str).unwrap_or("unknown");
                // A keyless signer would silently shrink the set — a
                // 3-of-5 stored as 3-of-4, which reads as a real
                // threshold rather than as missing data. Cannot happen
                // from `account_data` (the key is always emitted), so
                // reaching this means the producer changed shape.
                if key.is_empty() {
                    tracing::warn!(
                        account = %st.account_id,
                        "signer entry carries no key — DROPPED from the set; \
                         the stored signer count is now lower than the chain's"
                    );
                    continue;
                }
                // The protocol constrains non-master weights to 1-255
                // (SetOptions deletes at 0). Store what the chain
                // carried; out-of-range is an anomaly worth a trace,
                // never a silent clamp.
                if weight == 0 || weight > 255 {
                    tracing::warn!(
                        account = %st.account_id,
                        weight,
                        "signer weight outside protocol range 1-255 — stored as carried"
                    );
                }
                signer_keys.push(key.to_string());
                signer_weights.push(weight);
                signer_types.push(typ.to_string());
            }
            Some(AccountEntryStateRow {
                account_id: account_id_int,
                signer_keys,
                signer_weights,
                signer_types,
                master_weight,
                threshold_low,
                threshold_med,
                threshold_high,
                flags: st.flags.unwrap_or(0),
                last_updated_ledger: watermark,
            })
        }
        None => {
            tracing::warn!(
                account = %st.account_id,
                thresholds = %th_hex,
                "unparseable thresholds hex — signers row skipped, not fabricated"
            );
            None
        }
    }
}

/// 4-byte `Thresholds` hex → [master_weight, low, med, high]. `None` on any
/// malformation — the caller skips the row rather than fabricating zeros.
fn parse_thresholds(hex_str: &str) -> Option<[u8; 4]> {
    let bytes = hex::decode(hex_str).ok()?;
    <[u8; 4]>::try_from(bytes.as_slice()).ok()
}
