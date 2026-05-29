//! Request and response DTOs for the transactions endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// `filter[...]` query parameters for `GET /v1/transactions`.
///
/// `limit` and `cursor` are read by a sibling `Pagination<TsIdCursor>`
/// extractor and documented via the handler's `#[utoipa::path(params(...))]`
/// attribute.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// Filter by source account StrKey (G…).
    #[serde(rename = "filter[source_account]")]
    pub filter_source_account: Option<String>,
    /// Filter by contract StrKey (C…) — matches root op, nested call, or event emission.
    #[serde(rename = "filter[contract_id]")]
    pub filter_contract_id: Option<String>,
    /// Filter by operation type (e.g. `INVOKE_HOST_FUNCTION`).
    #[serde(rename = "filter[operation_type]")]
    pub filter_operation_type: Option<String>,
}

/// Opaque pagination payload for `GET /v1/transactions` (encoded via
/// [`common::cursor`](crate::common::cursor)).
///
/// The PG and CH read paths key their list scans differently, so this
/// payload is a superset that serves both. Field meanings by datasource:
///
/// | field      | PG (`created_at, id` keyset) | CH (`ledger_sequence, id` keyset) |
/// |------------|------------------------------|-----------------------------------|
/// | `ts`       | `transactions.created_at`    | parent ledger `closed_at`         |
/// | `id`       | `transactions.id` (BIGSERIAL)| parent `ledger_sequence`          |
/// | `tiebreak` | absent                       | `transactions.id` hash surrogate  |
///
/// CH orders by `(ledger_sequence, id)` and partition-prunes on
/// `intDiv(ledger_sequence, 500000)` (canonical SQL 02), so it needs the
/// `ledger_sequence` partition key (`id`) plus the within-ledger tie-break
/// (`tiebreak`). PG's `(created_at, id)` keyset needs no third field, so
/// `tiebreak` is omitted from the wire on that path. `tiebreak` carries a
/// serde default so cursors minted before this field existed still decode
/// (ADR 0008: the payload may evolve as long as old cursors decode or fail
/// cleanly — the wire format stays opaque to clients either way).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxListCursor {
    pub ts: DateTime<Utc>,
    pub id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiebreak: Option<i64>,
}

/// Slim transaction row returned in the list endpoint.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionListItem {
    /// Transaction hash (64-char lowercase hex).
    pub hash: String,
    pub ledger_sequence: i64,
    /// 1-based position of this transaction within its ledger.
    pub application_order: i16,
    /// `null` for Variant A `parse_error` transactions whose envelope
    /// could not be decoded (lore-0209). Always populated for ordinary
    /// (successful or failed-but-decoded) transactions.
    pub source_account: Option<String>,
    /// Fee charged in stroops.
    pub fee_charged: i64,
    /// Inner-transaction hash (64-char hex) for fee-bump envelopes, `null` otherwise.
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    /// `true` when the transaction touched at least one Soroban contract
    /// (root invocation, nested call, or event emission).
    pub has_soroban: bool,
    /// All distinct operation type names in the transaction
    /// (e.g. `["INVOKE_HOST_FUNCTION", "PAYMENT"]`).
    pub operation_types: Vec<String>,
    /// All C-StrKeys touched anywhere in the transaction.
    pub contract_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// `memo_type` / `memo` are NOT exposed on the list item by design — list
// endpoints stay DB-only. Memo lives on the transaction detail endpoint
// (`GET /v1/transactions/{hash}`) inside the E3 `heavy` block, which
// already pays for the archive XDR fetch for the full transaction view.
// Adding memo here would require an archive fetch per ledger touched by
// the page, which is wasteful for the list use case and inconsistent
// with the DB-only contract advertised by canonical SQL 02.

/// DB-sourced light slice for the transaction detail endpoint.
///
/// Composed with `E3HeavyFields` via `merge_e3_response` (task 0150). All
/// XDR-sourced fields (memo, result_code, signatures, events, operation
/// details, envelope_xdr/result_xdr, operation_tree) live in `heavy`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionDetailLight {
    /// Transaction hash (64-char lowercase hex).
    pub hash: String,
    pub ledger_sequence: i64,
    /// 1-based position of this transaction within its ledger.
    pub application_order: i16,
    /// `null` for Variant A `parse_error` transactions whose envelope
    /// could not be decoded (lore-0209).
    pub source_account: Option<String>,
    /// Fee charged in stroops.
    pub fee_charged: i64,
    /// Inner-transaction hash (64-char hex) for fee-bump envelopes, `null` otherwise.
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub created_at: DateTime<Utc>,
    /// `true` when the XDR parser encountered an error for this transaction.
    pub parse_error: bool,
    pub operations: Vec<OperationItem>,
    /// Accounts touched by this transaction. Populated only when
    /// `heavy_fields_status = "unavailable"`; otherwise `[]` and consumers
    /// should rely on the heavy block.
    pub participants: Vec<String>,
    /// Soroban event appearance index rows. Same fallback semantics as
    /// `participants`. Full topics + data live in `heavy.contract_events`.
    pub soroban_events: Vec<EventAppearanceItem>,
    /// Soroban invocation appearance index rows. Same fallback semantics
    /// as `participants`. Full call hierarchy lives in `heavy.operation_tree`.
    pub soroban_invocations: Vec<InvocationAppearanceItem>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EventAppearanceItem {
    pub contract_id: String,
    pub ledger_sequence: i64,
    pub amount: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvocationAppearanceItem {
    pub contract_id: String,
    /// Root caller G-StrKey. Per ADR 0034 nested-call hierarchy is XDR-only.
    pub caller_account: Option<String>,
    pub ledger_sequence: i64,
    pub amount: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OperationItem {
    /// Global BIGSERIAL `operations_appearances.id`. Internal ordering
    /// artefact only; not a within-tx index. Use `application_order`
    /// for apply-order display and to join against
    /// `XdrOperationDto.application_order` from the heavy overlay.
    pub appearance_id: i64,
    /// Operation type tag in canonical SCREAMING_SNAKE_CASE
    /// (e.g. `"INVOKE_HOST_FUNCTION"`).
    pub type_name: String,
    /// Raw `OperationType` SMALLINT (ADR 0031).
    #[serde(rename = "type")]
    pub op_type: i16,
    pub source_account: Option<String>,
    pub destination_account: Option<String>,
    pub contract_id: Option<String>,
    /// Asset code (≤12 chars) for classic asset operations.
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// Liquidity pool ID as SEP-23 strkey (`L...`, 56 chars). Encoded
    /// from the DB hex form at the response boundary so cross-entity
    /// link targets match the `/v1/liquidity-pools/:id` route shape.
    pub pool_id: Option<String>,
    /// 1-based per-tx apply position carrying on-chain operation order
    /// (task 0192). For folded appearance rows (multiple identical-identity
    /// envelope ops collapsed into one row, see task 0163) this is the
    /// MIN of the folded ops' indices — the position of the row's first
    /// occurrence in `tx.operations[]`. `None` for pre-task-0192 rows
    /// where the column was not yet populated; clients fall back to
    /// `appearance_id` order in that case.
    pub application_order: Option<i16>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::cursor::{self, Direction};
    use chrono::TimeZone;

    #[test]
    fn ch_cursor_round_trips_with_tiebreak() {
        // CH path: id = ledger_sequence, tiebreak = transactions.id hash
        // surrogate (may be negative — cityhash64 lower-bits as i64).
        let ts = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let c = TxListCursor {
            ts,
            id: 50_000,
            tiebreak: Some(-123),
        };
        let encoded = cursor::encode(&c, Direction::Next);
        let (dir, decoded): (Direction, TxListCursor) = cursor::decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Next);
        assert_eq!(decoded.id, 50_000);
        assert_eq!(decoded.tiebreak, Some(-123));
    }

    #[test]
    fn pg_cursor_omits_tiebreak_on_the_wire() {
        // `skip_serializing_if` keeps the PG cursor wire-identical to the
        // pre-0243 `(ts, id)` shape — no `tiebreak` key emitted.
        let ts = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let c = TxListCursor {
            ts,
            id: 42,
            tiebreak: None,
        };
        let json = serde_json::to_value(&c).unwrap();
        assert!(
            json.get("tiebreak").is_none(),
            "tiebreak must be omitted on the PG path: {json}"
        );
        assert_eq!(json["id"], 42);
    }

    #[test]
    fn legacy_ts_id_cursor_decodes_with_no_tiebreak() {
        // A cursor minted before `tiebreak` existed (`{ts, id}` only) must
        // still decode — serde default → None — so in-flight PG cursors
        // survive the 0243 deploy (ADR 0008 cursor opacity / evolvability).
        #[derive(serde::Serialize)]
        struct Legacy {
            ts: DateTime<Utc>,
            id: i64,
        }
        let ts = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let encoded = cursor::encode(&Legacy { ts, id: 7 }, Direction::Prev);
        let (dir, decoded): (Direction, TxListCursor) = cursor::decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Prev);
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.tiebreak, None);
    }
}
