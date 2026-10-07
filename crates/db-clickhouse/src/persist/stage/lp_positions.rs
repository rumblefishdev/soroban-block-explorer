//! `lp_positions` rows staged per ledger: one row per (pool, account), the
//! pool-share trustline's latest state.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::HashMap;

use xdr_parser::types::ExtractedLpPosition;

use super::{decimal7_string_to_i128, decode_hash};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::LpPositionRow;

pub(super) fn lp_position_rows(
    lp_positions: &[ExtractedLpPosition],
) -> Result<Vec<LpPositionRow>, SchemaError> {
    use std::collections::hash_map::Entry;
    let mut lp_dedup: HashMap<([u8; 32], i64), LpPositionRow> = HashMap::new();
    for pos in lp_positions {
        let pool_id = decode_hash(&pos.pool_id, "lp_position.pool_id")?;
        let acct_id = ids::account_id(&pos.account_id);
        let last = i64::from(pos.last_updated_ledger);
        let new_row = LpPositionRow {
            pool_id,
            account_id: acct_id,
            shares: decimal7_string_to_i128(&pos.shares)?,
            last_updated_ledger: last,
            // The pool-share trustline was removed — the participant left the
            // pool, as opposed to withdrawing to zero and staying. ADR 0055.
            closed_at_ledger: if pos.closed { last } else { 0 },
        };
        match lp_dedup.entry((pool_id, acct_id)) {
            Entry::Occupied(mut occ) => {
                let existing = occ.get_mut();
                if new_row.last_updated_ledger >= existing.last_updated_ledger {
                    *existing = new_row;
                }
            }
            Entry::Vacant(vac) => {
                vac.insert(new_row);
            }
        }
    }
    Ok(lp_dedup.into_values().collect())
}
