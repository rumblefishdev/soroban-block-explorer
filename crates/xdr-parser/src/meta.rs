//! Central `TransactionMeta` version accessor (task 0359, adoption #1).
//!
//! This module is the **only** place in the parser that matches on
//! `TransactionMeta` variants. Every function here is exhaustive with **no `_`
//! wildcard**, so a new protocol meta version (e.g. `V5` for Protocol 24) fails
//! to compile HERE — and is never silently absorbed into an empty result
//! anywhere else. Before this module, six independently-written call sites each
//! ended in a `_ => empty` arm that would have quietly dropped every
//! event / change / return value of a `V5` transaction while the ledger still
//! committed (the "quiet chain" failure Horizon's version-stamped rebuild
//! exists to prevent).
//!
//! `V0 | V1 | V2` are the pre-Soroban legacy shapes. This is a Soroban-era
//! indexer (backfill floor = ledger 50,457,424, Protocol 20 go-live), so those
//! variants never occur in the fed range; they are handled as an explicit
//! empty/legacy arm, and [`is_soroban_meta`] lets the tx extractor flag any
//! stray legacy meta as `parse_error` rather than persist a silent empty record.
//!
//! ## Adding a new meta version (Protocol 24+)
//!
//! The compiler will point at every function below. For each, decide whether the
//! new version carries the projected data (implement the arm) or not (extend the
//! legacy arm) — never add a `_ =>` wildcard, and never stub an empty return
//! without also excluding the version from [`is_soroban_meta`].

use stellar_xdr::{
    ContractEvent, DiagnosticEvent, LedgerEntryChange, LedgerEntryChanges, ScVal, TransactionMeta,
};

use crate::types::EventSource;

/// A ledger entry change together with the operation it belongs to.
/// `op_index = None` marks a transaction-level change (`tx_changes_before` /
/// `tx_changes_after`); `Some(i)` marks a change of operation `i` (0-based).
pub struct LocatedChange<'a> {
    pub change: &'a LedgerEntryChange,
    pub op_index: Option<u32>,
}

/// A contract event together with the meta container it was found in — preserves
/// the exact ordering `event::extract_events` numbers `event_index` from.
pub struct EventRef<'a> {
    pub event: &'a ContractEvent,
    pub source: EventSource,
}

/// Whether this meta version carries the Soroban data this indexer extracts
/// (`V3` / `V4`). `false` for the pre-Soroban legacy shapes (`V0`/`V1`/`V2`),
/// which never occur in the Soroban-era range — the tx extractor treats a stray
/// one as `parse_error` rather than a silent empty record.
pub fn is_soroban_meta(meta: &TransactionMeta) -> bool {
    match meta {
        TransactionMeta::V3(_) | TransactionMeta::V4(_) => true,
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => false,
    }
}

/// Every ledger entry change in canonical order: `tx_changes_before`, then each
/// operation's changes (tagged with its 0-based op index), then
/// `tx_changes_after`. Empty for legacy meta.
pub fn located_ledger_changes(meta: &TransactionMeta) -> Vec<LocatedChange<'_>> {
    match meta {
        TransactionMeta::V3(v3) => collect_located(
            &v3.tx_changes_before,
            v3.operations.iter().map(|o| &o.changes),
            &v3.tx_changes_after,
        ),
        TransactionMeta::V4(v4) => collect_located(
            &v4.tx_changes_before,
            v4.operations.iter().map(|o| &o.changes),
            &v4.tx_changes_after,
        ),
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => Vec::new(),
    }
}

fn collect_located<'a>(
    before: &'a LedgerEntryChanges,
    op_changes: impl Iterator<Item = &'a LedgerEntryChanges>,
    after: &'a LedgerEntryChanges,
) -> Vec<LocatedChange<'a>> {
    let mut out = Vec::new();
    for change in before.iter() {
        out.push(LocatedChange {
            change,
            op_index: None,
        });
    }
    for (i, changes) in op_changes.enumerate() {
        let op_index = Some(u32::try_from(i).expect("operation index does not fit into u32"));
        for change in changes.iter() {
            out.push(LocatedChange { change, op_index });
        }
    }
    for change in after.iter() {
        out.push(LocatedChange {
            change,
            op_index: None,
        });
    }
    out
}

/// The ledger entry changes of ONE operation (by 0-based index). Empty for
/// legacy meta or an index past the meta's operations (a failed tx carries no
/// per-op changes).
pub fn op_changes(meta: &TransactionMeta, op_idx: usize) -> &[LedgerEntryChange] {
    match meta {
        TransactionMeta::V3(v3) => v3
            .operations
            .get(op_idx)
            .map(|o| o.changes.as_slice())
            .unwrap_or(&[]),
        TransactionMeta::V4(v4) => v4
            .operations
            .get(op_idx)
            .map(|o| o.changes.as_slice())
            .unwrap_or(&[]),
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => &[],
    }
}

/// Diagnostic events — V3 `soroban_meta.diagnostic_events` / V4 top-level
/// `diagnostic_events`. Empty for legacy meta.
pub fn diagnostic_events(meta: &TransactionMeta) -> Vec<&DiagnosticEvent> {
    match meta {
        TransactionMeta::V3(v3) => v3
            .soroban_meta
            .as_ref()
            .map(|m| m.diagnostic_events.iter().collect())
            .unwrap_or_default(),
        TransactionMeta::V4(v4) => v4.diagnostic_events.iter().collect(),
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => Vec::new(),
    }
}

/// The Soroban root return value, if any. V3's `return_value` is a bare `ScVal`;
/// V4's is `Option<ScVal>`. `None` for legacy meta.
pub fn return_value(meta: &TransactionMeta) -> Option<ScVal> {
    match meta {
        TransactionMeta::V3(v3) => v3.soroban_meta.as_ref().map(|m| m.return_value.clone()),
        TransactionMeta::V4(v4) => v4
            .soroban_meta
            .as_ref()
            .and_then(|m| m.return_value.clone()),
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => None,
    }
}

/// The contract-event stream in the exact order `event::extract_events` assigns
/// `event_index` from:
/// - V3: `soroban_meta.events` (tx-level), then `soroban_meta.diagnostic_events`.
/// - V4 (CAP-67): `v4.events` (tx-level), then each op's `events` (per-op), then
///   `v4.diagnostic_events`.
///
/// Empty for legacy meta.
pub fn contract_events(meta: &TransactionMeta) -> Vec<EventRef<'_>> {
    let mut out = Vec::new();
    match meta {
        TransactionMeta::V3(v3) => {
            let Some(sm) = v3.soroban_meta.as_ref() else {
                return out;
            };
            for event in sm.events.iter() {
                out.push(EventRef {
                    event,
                    source: EventSource::TxLevel,
                });
            }
            for diag in sm.diagnostic_events.iter() {
                out.push(EventRef {
                    event: &diag.event,
                    source: EventSource::Diagnostic,
                });
            }
        }
        TransactionMeta::V4(v4) => {
            for tx_event in v4.events.iter() {
                out.push(EventRef {
                    event: &tx_event.event,
                    source: EventSource::TxLevel,
                });
            }
            for op_meta in v4.operations.iter() {
                for event in op_meta.events.iter() {
                    out.push(EventRef {
                        event,
                        source: EventSource::PerOp,
                    });
                }
            }
            for diag in v4.diagnostic_events.iter() {
                out.push(EventRef {
                    event: &diag.event,
                    source: EventSource::Diagnostic,
                });
            }
        }
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        AccountId, ExtensionPoint, LedgerKey, LedgerKeyAccount, OperationMeta, PublicKey,
        TransactionMetaV3, Uint256, VecM,
    };

    fn acct(b: u8) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([b; 32])))
    }

    /// A cheap `Removed` change (no full entry needed) tagged by a byte.
    fn removed(b: u8) -> LedgerEntryChange {
        LedgerEntryChange::Removed(LedgerKey::Account(LedgerKeyAccount {
            account_id: acct(b),
        }))
    }

    fn changes(cs: Vec<LedgerEntryChange>) -> LedgerEntryChanges {
        cs.try_into().unwrap()
    }

    fn v3(
        before: Vec<LedgerEntryChange>,
        ops: Vec<Vec<LedgerEntryChange>>,
        after: Vec<LedgerEntryChange>,
    ) -> TransactionMeta {
        let operations: Vec<OperationMeta> = ops
            .into_iter()
            .map(|c| OperationMeta {
                changes: changes(c),
            })
            .collect();
        TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: changes(before),
            operations: operations.try_into().unwrap(),
            tx_changes_after: changes(after),
            soroban_meta: None,
        })
    }

    fn legacy() -> TransactionMeta {
        TransactionMeta::V0(VecM::default())
    }

    #[test]
    fn is_soroban_meta_true_for_v3_false_for_legacy() {
        assert!(is_soroban_meta(&v3(vec![], vec![], vec![])));
        assert!(!is_soroban_meta(&legacy()));
    }

    #[test]
    fn located_changes_tag_op_index_and_preserve_order() {
        // before · op0 · after → the op-tagging + before/after ordering contract.
        let meta = v3(
            vec![removed(0xB1)],
            vec![vec![removed(0xC1)]],
            vec![removed(0xA1)],
        );
        let idx: Vec<Option<u32>> = located_ledger_changes(&meta)
            .iter()
            .map(|lc| lc.op_index)
            .collect();
        assert_eq!(idx, vec![None, Some(0), None]);
    }

    #[test]
    fn located_changes_empty_for_legacy() {
        assert!(located_ledger_changes(&legacy()).is_empty());
    }

    #[test]
    fn op_changes_slice_and_out_of_range() {
        let meta = v3(vec![], vec![vec![removed(0xC1), removed(0xC2)]], vec![]);
        assert_eq!(op_changes(&meta, 0).len(), 2);
        assert!(op_changes(&meta, 9).is_empty());
        assert!(op_changes(&legacy(), 0).is_empty());
    }

    #[test]
    fn soroban_projections_empty_for_legacy() {
        let legacy = legacy();
        assert!(return_value(&legacy).is_none());
        assert!(diagnostic_events(&legacy).is_empty());
        assert!(contract_events(&legacy).is_empty());
    }
}
