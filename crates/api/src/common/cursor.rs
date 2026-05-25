//! Opaque cursor encoding shared by every paginated list endpoint.
//!
//! The wire format is `base64url(JSON(<envelope>))` with no padding. The
//! envelope is a `{ "dir": <Direction>, "p": <payload> }` object: `dir`
//! discriminates between the forward (`Next`) and backward (`Prev`) walks,
//! and `p` carries a resource-specific payload struct. Most resources use
//! [`TsIdCursor`] as the payload, which encodes `(created_at, id)` and
//! matches the natural DESC ordering + `id` tie-break of the partitioned
//! fact tables.
//!
//! The envelope is **strict**: missing `dir`, missing `p`, or unknown
//! `dir` variants fail decode with [`CursorError::InvalidPayload`]. There
//! is no legacy fallback — every minted cursor must carry the discriminant
//! explicitly. This forecloses the silent-promotion path where a
//! malformed cursor was mistakenly treated as a forward walk.
//!
//! The wrapper is **nested** rather than `#[serde(flatten)]`'d on
//! purpose: flatten buffers through `serde_json::Value`, breaks under
//! `#[serde(deny_unknown_fields)]` on the payload, and silently collides
//! if a future payload type introduces a `dir` field of its own. Nesting
//! is ~5 bytes larger on the wire per cursor and avoids all three traps.
//!
//! Cursor opacity is a public-contract requirement from ADR 0008: clients
//! must never construct a cursor by hand or rely on its internal shape.
//! That means the payload type can change between releases without a
//! breaking API change, as long as the previous format fails decode cleanly
//! and produces an `INVALID_CURSOR` error.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

/// Error returned by [`decode`] when a client-supplied cursor string
/// cannot be parsed.
///
/// Both variants map to HTTP 400 + `ErrorEnvelope { code: "invalid_cursor" }`
/// at the extractor boundary. The distinction is kept internal so tests and
/// logs can assert on the specific failure mode without exposing it over
/// the wire.
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("cursor is not valid base64url")]
    InvalidBase64,
    #[error("cursor payload does not match expected schema")]
    InvalidPayload,
}

/// Walk direction encoded inside the cursor envelope.
///
/// `Next` means "fetch rows ordered DESC strictly after the cursor
/// position" — the forward behaviour. `Prev` means "fetch rows ordered
/// ASC strictly before the cursor position, then reverse for
/// presentation" — the backward walk that powers `prev_cursor`.
///
/// Clients never inspect this; it round-trips opaquely as part of the
/// base64-JSON cursor string. There is no `Default` impl on purpose: the
/// envelope requires an explicit `dir` to decode, so a missing
/// discriminant is an error and not a silent forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Next,
    Prev,
}

/// On-the-wire cursor envelope: an explicit [`Direction`] discriminant
/// plus a resource-specific payload `P` under the `p` key.
///
/// Nested (not `#[serde(flatten)]`) so the wrapper:
///
/// - composes with arbitrary `P` types regardless of `deny_unknown_fields`
///   or other serde attributes,
/// - cannot silently collide with a future payload that happens to have a
///   `dir` field of its own,
/// - decodes without the `Value` buffering hop that `flatten` requires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorEnvelope<P> {
    dir: Direction,
    p: P,
}

/// Encode a typed payload + direction as a base64url-JSON cursor string.
///
/// Serialisation is infallible for every payload type whose `Serialize`
/// impl is total (all `#[derive(Serialize)]` structs fit this). A panic
/// here would indicate a broken manual `Serialize` impl, not user input,
/// so we surface it as a panic rather than an error.
pub fn encode<P: Serialize>(payload: &P, dir: Direction) -> String {
    // Borrowing wrapper kept local to this function — `CursorEnvelope<P>`
    // is the decode-side shape (owned `p: P`), so reusing it here would
    // either require cloning the payload or rely on `P` being inferred
    // as `&T`. An explicit local struct keeps the encode shape obvious
    // and zero-copy.
    #[derive(Serialize)]
    struct Wrap<'a, T: Serialize> {
        dir: Direction,
        p: &'a T,
    }
    // to_vec, not to_string — JSON output is already valid UTF-8, so
    // round-tripping through String just adds a redundant validation +
    // allocation. Encoder takes &[u8] anyway.
    let json = serde_json::to_vec(&Wrap { dir, p: payload })
        .expect("cursor payload serialization is infallible");
    URL_SAFE_NO_PAD.encode(&json)
}

/// Decode a base64url-JSON cursor string into `(Direction, payload)`.
///
/// Returns [`CursorError::InvalidBase64`] for malformed base64 and
/// [`CursorError::InvalidPayload`] for any envelope-shape failure:
/// missing `dir`, missing `p`, unknown `dir` variant, payload schema
/// mismatch, truncated JSON, etc. There is no legacy-cursor fallback.
pub fn decode<P: DeserializeOwned>(s: &str) -> Result<(Direction, P), CursorError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| CursorError::InvalidBase64)?;
    let envelope: CursorEnvelope<P> =
        serde_json::from_slice(&bytes).map_err(|_| CursorError::InvalidPayload)?;
    Ok((envelope.dir, envelope.p))
}

/// Default cursor payload for resources ordered by `(created_at DESC, id DESC)`.
///
/// Every partitioned fact table in the explorer (transactions, operations,
/// ledgers, events, …) orders its list endpoints this way, so this payload
/// covers the common case. Resources with a different natural ordering
/// define their own payload struct and reuse [`encode`] / [`decode`] directly.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TsIdCursor {
    pub ts: DateTime<Utc>,
    pub id: i64,
}

impl TsIdCursor {
    pub fn new(ts: DateTime<Utc>, id: i64) -> Self {
        Self { ts, id }
    }
}

/// SQL fragments for a direction-aware cursor walk: the comparison
/// operator and the `ORDER BY` direction.
///
/// `Next` walks DESC strictly less than the cursor anchor; `Prev` walks
/// ASC strictly greater than the cursor anchor (the caller reverses the
/// resulting rows for presentation via `common::pagination::finalize_page`).
/// Both fragments are `'static` SQL literals, safe to interpolate into
/// `format!()` query strings — no client input flows through them.
pub fn direction_sql(direction: Direction) -> (&'static str, &'static str) {
    match direction {
        Direction::Next => ("<", "DESC"),
        Direction::Prev => (">", "ASC"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn ts_id_round_trip_next() {
        let ts = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let original = TsIdCursor::new(ts, 42_000_i64);
        let encoded = encode(&original, Direction::Next);
        let (dir, decoded): (Direction, TsIdCursor) = decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Next);
        assert_eq!(decoded.ts, original.ts);
        assert_eq!(decoded.id, original.id);
    }

    #[test]
    fn ts_id_round_trip_prev() {
        let ts = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let original = TsIdCursor::new(ts, 42_000_i64);
        let encoded = encode(&original, Direction::Prev);
        let (dir, decoded): (Direction, TsIdCursor) = decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Prev);
        assert_eq!(decoded.ts, original.ts);
        assert_eq!(decoded.id, original.id);
    }

    #[test]
    fn legacy_cursor_without_dir_is_rejected() {
        // Pre-Direction wire format (bare payload, no envelope, no `dir`)
        // is no longer accepted: there is no legacy fallback. Such a
        // cursor must fail decode with InvalidPayload so the client
        // surfaces an `invalid_cursor` 400 rather than silently being
        // promoted to a forward walk.
        let legacy_json = br#"{"ts":"2026-04-24T12:00:00Z","id":42}"#;
        let legacy_b64 = URL_SAFE_NO_PAD.encode(legacy_json);
        let err = decode::<TsIdCursor>(&legacy_b64).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }

    #[test]
    fn envelope_without_payload_is_rejected() {
        let bad = URL_SAFE_NO_PAD.encode(br#"{"dir":"next"}"#);
        let err = decode::<TsIdCursor>(&bad).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }

    #[test]
    fn envelope_with_extra_top_level_field_is_rejected() {
        // `deny_unknown_fields` on the envelope catches forged cursors
        // and forward-compat experiments minted by a newer deployment.
        let bad = URL_SAFE_NO_PAD.encode(
            br#"{"dir":"next","p":{"ts":"2026-04-24T12:00:00Z","id":42},"extra":1}"#,
        );
        let err = decode::<TsIdCursor>(&bad).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }

    #[test]
    fn invalid_base64_returns_invalid_base64() {
        let err = decode::<TsIdCursor>("not!!base64").unwrap_err();
        assert!(matches!(err, CursorError::InvalidBase64));
    }

    #[test]
    fn wrong_schema_returns_invalid_payload() {
        let bad = URL_SAFE_NO_PAD.encode(b"{}");
        let err = decode::<TsIdCursor>(&bad).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }

    #[test]
    fn truncated_json_returns_invalid_payload() {
        let bad = URL_SAFE_NO_PAD.encode(br#"{"dir":"next","p":{"ts":"2026-04-24T12:00:00Z""#);
        let err = decode::<TsIdCursor>(&bad).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct StrictPayload {
        seq: u64,
    }

    #[test]
    fn payload_with_deny_unknown_fields_round_trips() {
        // Regression for the flatten footgun: under `#[serde(flatten)]`
        // a payload with `deny_unknown_fields` would have rejected the
        // envelope's `dir` key. The nested envelope sidesteps that.
        let original = StrictPayload { seq: 12_345 };
        let encoded = encode(&original, Direction::Next);
        let (dir, decoded): (Direction, StrictPayload) = decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Next);
        assert_eq!(decoded, original);
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct CollidingPayload {
        // Real-world flatten footgun: payload happens to declare a `dir`
        // field that would shadow the envelope's discriminant. Nesting
        // keeps the two namespaces independent.
        dir: String,
        v: i64,
    }

    #[test]
    fn payload_field_named_dir_does_not_collide() {
        let original = CollidingPayload {
            dir: "north".into(),
            v: 7,
        };
        let encoded = encode(&original, Direction::Prev);
        let (dir, decoded): (Direction, CollidingPayload) = decode(&encoded).unwrap();
        assert_eq!(dir, Direction::Prev);
        assert_eq!(decoded, original);
    }

    #[test]
    fn unknown_dir_value_returns_invalid_payload() {
        // Defensive: a cursor minted by a future deployment with a new
        // Direction variant must fail decode on the current code rather
        // than silently mapping to Next. Tests `#[serde(rename_all =
        // "snake_case")]` rejection of unknown variants.
        let bad_json =
            br#"{"dir":"sideways","p":{"ts":"2026-04-24T12:00:00Z","id":42}}"#;
        let bad_b64 = URL_SAFE_NO_PAD.encode(bad_json);
        let err = decode::<TsIdCursor>(&bad_b64).unwrap_err();
        assert!(matches!(err, CursorError::InvalidPayload));
    }
}
