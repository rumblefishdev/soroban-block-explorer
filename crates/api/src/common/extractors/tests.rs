use super::*;
use crate::common::cursor::TsIdCursor;
use axum::body;
use axum::http::StatusCode;
use chrono::{TimeZone, Utc};

async fn body_json(resp: Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[test]
fn limit_default_when_missing() {
    assert_eq!(validate_limit(None).unwrap(), 20);
}

#[test]
fn limit_within_bounds_accepted() {
    assert_eq!(validate_limit(Some("42")).unwrap(), 42);
    assert_eq!(validate_limit(Some("100")).unwrap(), 100);
    assert_eq!(validate_limit(Some("1")).unwrap(), 1);
}

#[tokio::test]
async fn limit_zero_rejected_with_invalid_limit() {
    let err = validate_limit(Some("0")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["received"], 0);
}

#[tokio::test]
async fn limit_above_max_rejected() {
    let err = validate_limit(Some("101")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["max"], 100);
}

#[tokio::test]
async fn limit_non_numeric_rejected() {
    let err = validate_limit(Some("many")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["received"], "many");
}

#[tokio::test]
async fn limit_empty_string_rejected_with_invalid_limit() {
    // ?limit= → axum/serde_urlencoded yields Some("") (not None). Without
    // an explicit guard the parse path catches this as a numeric error,
    // but lock the behaviour here so a future refactor cannot silently
    // change `?limit=` from 400 to "use default".
    let err = validate_limit(Some("")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["received"], "");
}

#[tokio::test]
async fn limit_negative_rejected_with_invalid_limit() {
    // ?limit=-1 fails u32 parse before the bounds check; assert this
    // path so a future signed-int refactor does not start accepting
    // negatives and clamping silently.
    let err = validate_limit(Some("-1")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["received"], "-1");
}

#[test]
fn cursor_none_when_missing() {
    let (dir, result): (Direction, Option<TsIdCursor>) = decode_cursor(None).unwrap();
    assert_eq!(dir, Direction::Next);
    assert!(result.is_none());
}

#[test]
fn cursor_decoded_when_valid() {
    let encoded = cursor::encode(
        &TsIdCursor::new(Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap(), 42),
        Direction::Next,
    );
    let (dir, decoded): (Direction, Option<TsIdCursor>) = decode_cursor(Some(&encoded)).unwrap();
    assert_eq!(dir, Direction::Next);
    assert_eq!(decoded.unwrap().id, 42);
}

#[test]
fn cursor_decoded_propagates_prev_direction() {
    let encoded = cursor::encode(
        &TsIdCursor::new(Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap(), 42),
        Direction::Prev,
    );
    let (dir, decoded): (Direction, Option<TsIdCursor>) = decode_cursor(Some(&encoded)).unwrap();
    assert_eq!(dir, Direction::Prev);
    assert_eq!(decoded.unwrap().id, 42);
}

#[tokio::test]
async fn cursor_malformed_rejected_with_invalid_cursor() {
    let err = decode_cursor::<TsIdCursor>(Some("not!!base64")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn cursor_wrong_schema_rejected_with_invalid_cursor() {
    use base64::Engine;
    let bad = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{}");
    let err = decode_cursor::<TsIdCursor>(Some(&bad)).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn an_events_cursor_minted_before_the_rpc_id_is_rejected() {
    // The pre-0541 keyset (`transaction_id` + our flat counter).
    let old = cursor::encode(
        &serde_json::json!({"src": "ch", "ledger_sequence": 1, "transaction_id": 2, "event_index": 3}),
        Direction::Next,
    );
    let err = decode_cursor::<crate::contracts::dto::EventCursor>(Some(&old)).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn cursor_empty_string_rejected_with_invalid_cursor() {
    // ?cursor= yields Some("") at this layer. base64 decode of "" is
    // technically Ok([]), so the failure surfaces at JSON decode of the
    // empty byte slice (`InvalidPayload`). Either branch maps to the
    // same envelope — locked here so future input sanitisation can't
    // accidentally accept it as "no cursor".
    let err = decode_cursor::<TsIdCursor>(Some("")).unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn extractor_parses_full_query_with_unknown_field() {
    // Real-router happy path: limit + valid cursor + an unrelated
    // `filter[...]` query key. Pagination must accept the unknown
    // field (FromRequestParts uses `Query<PaginationRaw>` which
    // tolerates unknowns) so it can coexist with a sibling
    // `Query<ListParams>` extractor on the same handler.
    use axum::extract::FromRequestParts;
    use axum::http::Request;

    let encoded = cursor::encode(
        &TsIdCursor::new(Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap(), 42),
        Direction::Next,
    );
    let uri = format!("/?limit=10&cursor={encoded}&filter%5Bsource_account%5D=GAA");
    let req = Request::builder().uri(&uri).body(()).unwrap();
    let (mut parts, _) = req.into_parts();

    let p: Pagination<TsIdCursor> = Pagination::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert_eq!(p.limit, 10);
    assert_eq!(p.direction, Direction::Next);
    assert_eq!(p.cursor.unwrap().id, 42);
}

#[tokio::test]
async fn extractor_propagates_prev_direction() {
    // Prev-direction cursor in the wire: extractor must surface
    // `direction = Prev` so handlers branch the SQL accordingly.
    use axum::extract::FromRequestParts;
    use axum::http::Request;

    let encoded = cursor::encode(
        &TsIdCursor::new(Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap(), 42),
        Direction::Prev,
    );
    let uri = format!("/?limit=10&cursor={encoded}");
    let req = Request::builder().uri(&uri).body(()).unwrap();
    let (mut parts, _) = req.into_parts();

    let p: Pagination<TsIdCursor> = Pagination::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert_eq!(p.direction, Direction::Prev);
    assert_eq!(p.cursor.unwrap().id, 42);
}
