//! Helpers that turn a `limit+1` row fetch into the canonical
//! [`Paginated`](crate::openapi::schemas::Paginated) envelope.
//!
//! The helpers live outside any specific ORM layer on purpose: every
//! list endpoint in this crate builds its SQL with a hand-written
//! `sqlx::QueryBuilder`, so the pagination contract is "fetch one extra
//! row, hand it here, receive a ready envelope back". This keeps the
//! SQL plans per-endpoint explicit (no generic WHERE clause injection)
//! while centralising the wire-shape choreography.

use crate::openapi::schemas::{PageInfo, Paginated};

use super::cursor::{self, Direction};

/// Trim a `limit+1` row slice to `limit` rows and derive the
/// [`PageInfo`] for the page, including both forward (`next_cursor`)
/// and backward (`prev_cursor`) continuations.
///
/// **Inputs**
///
/// - `rows` — up to `limit + 1` items in the order the SQL fetch
///   produced them (DESC for [`Direction::Next`], ASC for
///   [`Direction::Prev`]). The extra row, when present, is the "peek"
///   that signals another page exists in the fetched direction.
/// - `direction` — which way the handler walked. Determines whether the
///   extra row is at the tail (DESC fetch) or head (ASC fetch after
///   reverse), and which side of the displayed slice each cursor
///   anchors to.
/// - `has_predecessor` — `true` when the request carried a
///   `?cursor=`. A first-page request (no cursor) cannot have a
///   `prev_cursor` because nothing precedes the newest rows in the
///   table.
/// - `cursor_of(dir, row)` — encode an opaque cursor for `row` in
///   direction `dir`. Called at most twice (once for `cursor`, once for
///   `prev_cursor`).
///
/// **Output behaviour**
///
/// | `direction` | `has_predecessor` | `has_excess` | `next_cursor` | `prev_cursor` |
/// |-------------|-----------------------|--------------|--------------|---------------|
/// | Next | false (page 1) | true | last displayed (Next) | None |
/// | Next | false (page 1) | false | None | None |
/// | Next | true (mid) | true | last displayed (Next) | first displayed (Prev) |
/// | Next | true (last) | false | None | first displayed (Prev) |
/// | Prev | true | true | last displayed (Next) | first displayed (Prev) |
/// | Prev | true (hit start) | false | last displayed (Next) | None |
///
/// For `Direction::Prev`, the helper reverses `rows` after trimming so
/// the caller can map them straight into the response DTO in DESC
/// presentation order (same as a forward walk).
pub fn finalize_page<Row>(
    rows: &mut Vec<Row>,
    limit: u32,
    direction: Direction,
    has_predecessor: bool,
    cursor_of: impl Fn(Direction, &Row) -> String,
) -> PageInfo {
    let limit_usize = limit as usize;
    let has_excess = rows.len() > limit_usize;

    let (next_cursor, prev_cursor) = match direction {
        Direction::Next => {
            // DESC fetch: rows already in presentation order. Excess is
            // at the tail.
            if has_excess {
                rows.truncate(limit_usize);
            }
            let next = if has_excess {
                rows.last().map(|r| cursor_of(Direction::Next, r))
            } else {
                None
            };
            // First-page request (no input cursor) ⇒ nothing precedes;
            // otherwise the front of the page is the boundary the client
            // can walk back through.
            let prev = if has_predecessor {
                rows.first().map(|r| cursor_of(Direction::Prev, r))
            } else {
                None
            };
            (next, prev)
        }
        Direction::Prev => {
            // ASC fetch: rows are in reverse-presentation order, oldest
            // first. The excess (if any) is the row FURTHEST from the
            // cursor anchor — drop it from the tail before reversing so
            // the rows we keep are the N closest to the previous page.
            let has_more_prev = has_excess;
            if has_excess {
                rows.truncate(limit_usize);
            }
            rows.reverse();

            // After reverse: rows are in DESC presentation order.
            let prev = if has_more_prev {
                rows.first().map(|r| cursor_of(Direction::Prev, r))
            } else {
                None
            };
            // Forward continuation always exists when a backward fetch
            // returns rows: ledger data is immutable, so the cursor
            // anchor (sourced from a prior forward fetch) still
            // references a row in the table. Anchor `cursor` at the
            // oldest displayed row unconditionally. See ADR 0008.
            let next = rows.last().map(|r| cursor_of(Direction::Next, r));
            (next, prev)
        }
    };

    PageInfo {
        next_cursor,
        prev_cursor,
        limit,
    }
}

/// Convenience over [`finalize_page`] for the overwhelmingly common case
/// of a `(created_at, id)` cursor: pass the accessor closures for those
/// two fields and this helper assembles the [`cursor::TsIdCursor`]
/// payload and encodes it.
pub fn finalize_ts_id_page<Row>(
    rows: &mut Vec<Row>,
    limit: u32,
    direction: Direction,
    has_predecessor: bool,
    ts_of: impl Fn(&Row) -> chrono::DateTime<chrono::Utc>,
    id_of: impl Fn(&Row) -> i64,
) -> PageInfo {
    finalize_page(rows, limit, direction, has_predecessor, |dir, r| {
        cursor::encode(&cursor::TsIdCursor::new(ts_of(r), id_of(r)), dir)
    })
}

/// Assemble rows + [`PageInfo`] into the canonical [`Paginated`] envelope.
///
/// Separate from [`finalize_page`] so handlers can map DB rows to
/// response DTOs between the two calls — the common shape is:
///
/// ```ignore
/// let page = finalize_ts_id_page(&mut db_rows, limit, |r| r.created_at, |r| r.id);
/// let data: Vec<ResponseItem> = db_rows.into_iter().map(into_response_item).collect();
/// Json(into_envelope(data, page))
/// ```
pub fn into_envelope<T>(data: Vec<T>, page: PageInfo) -> Paginated<T>
where
    T: utoipa::ToSchema,
{
    Paginated { data, page }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Timelike, Utc};

    #[derive(Debug, Clone)]
    struct Row {
        id: i64,
        ts: DateTime<Utc>,
    }

    fn row(id: i64, sec: u32) -> Row {
        Row {
            id,
            ts: Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, sec).unwrap(),
        }
    }

    fn label(_dir: Direction, r: &Row) -> String {
        format!("id-{}", r.id)
    }

    // ---------- Direction::Next ----------

    #[test]
    fn next_first_page_no_excess_no_cursors() {
        // First page request (no input cursor), result fits in `limit` —
        // no `cursor` (no further page) and no `prev_cursor` (nothing
        // precedes page 1).
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2)];
        let page = finalize_page(&mut rows, 5, Direction::Next, false, label);
        assert_eq!(rows.len(), 3);
        assert!(page.next_cursor.is_none());
        assert!(page.prev_cursor.is_none());
        assert_eq!(page.limit, 5);
    }

    #[test]
    fn next_first_page_with_excess_emits_only_forward_cursor() {
        // First page, `limit+1` rows fetched → forward cursor set, but
        // no `prev_cursor` because the request had no input cursor.
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2), row(4, 3)];
        let page = finalize_page(&mut rows, 3, Direction::Next, false, label);
        assert_eq!(rows.len(), 3);
        assert_eq!(page.next_cursor.as_deref(), Some("id-3"));
        assert!(page.prev_cursor.is_none());
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn next_mid_page_emits_both_cursors() {
        // Mid-walk: `has_predecessor = true` + excess row. Both
        // cursors set; `prev_cursor` anchors at the front of the slice,
        // `cursor` at the back.
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2), row(4, 3)];
        let page = finalize_page(&mut rows, 3, Direction::Next, true, label);
        assert_eq!(rows.len(), 3);
        assert_eq!(page.next_cursor.as_deref(), Some("id-3"));
        assert_eq!(page.prev_cursor.as_deref(), Some("id-1"));
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn next_last_page_with_input_cursor_emits_only_prev() {
        // Last forward page reached: no excess ⇒ no forward cursor; an
        // input cursor was present, so prev_cursor anchors at the front.
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2)];
        let page = finalize_page(&mut rows, 3, Direction::Next, true, label);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.prev_cursor.as_deref(), Some("id-1"));
        assert!(page.next_cursor.is_none());
    }

    // ---------- Direction::Prev ----------

    #[test]
    fn prev_mid_with_excess_emits_both_cursors_and_reverses_rows() {
        // ASC fetch yields rows oldest-first; helper reverses to DESC
        // presentation order. Excess at tail of ASC is the row furthest
        // from the cursor anchor (newest), which we drop before reverse.
        // Input ASC order: id=1 (oldest), id=2, id=3, id=4 (newest, excess).
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2), row(4, 3)];
        let page = finalize_page(&mut rows, 3, Direction::Prev, true, label);
        // After reverse: rows in DESC = [id=3, id=2, id=1].
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 2, 1]);
        // prev_cursor anchors at the newest displayed (front of DESC)
        // because that's where the user can walk further back from.
        assert_eq!(page.prev_cursor.as_deref(), Some("id-3"));
        // next_cursor anchors at the oldest displayed (tail of DESC) =
        // the row the user came from when they clicked Prev.
        assert_eq!(page.next_cursor.as_deref(), Some("id-1"));
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn prev_hit_first_page_clears_prev_cursor() {
        // ASC fetch returns fewer than limit+1 rows ⇒ no more backward
        // page. Forward continuation still exists (the user came from
        // there).
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2)];
        let page = finalize_page(&mut rows, 3, Direction::Prev, true, label);
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 2, 1]);
        assert!(page.prev_cursor.is_none());
        assert_eq!(page.next_cursor.as_deref(), Some("id-1"));
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn prev_empty_result_clears_both_cursors() {
        // Pathological: backward fetch returns 0 rows. Both cursors
        // None; FE will disable both buttons and the user navigates via
        // the route to recover.
        let mut rows: Vec<Row> = vec![];
        let page = finalize_page(&mut rows, 3, Direction::Prev, true, label);
        assert!(rows.is_empty());
        assert!(page.next_cursor.is_none());
        assert!(page.prev_cursor.is_none());
        assert!(page.next_cursor.is_none());
    }

    // ---------- ts_id helper ----------

    #[test]
    fn ts_id_helper_round_trips_via_envelope() {
        let mut rows = vec![row(1, 0), row(2, 1), row(3, 2), row(4, 3)];
        let page =
            finalize_ts_id_page(&mut rows, 3, Direction::Next, true, |r| r.ts, |r| r.id);
        assert_eq!(rows.len(), 3);
        assert!(page.next_cursor.is_some());

        let (next_dir, next): (Direction, cursor::TsIdCursor) =
            cursor::decode(&page.next_cursor.clone().unwrap()).unwrap();
        assert_eq!(next_dir, Direction::Next);
        assert_eq!(next.id, 3);
        assert_eq!(next.ts.second(), 2);

        let (prev_dir, prev): (Direction, cursor::TsIdCursor) =
            cursor::decode(&page.prev_cursor.clone().unwrap()).unwrap();
        assert_eq!(prev_dir, Direction::Prev);
        assert_eq!(prev.id, 1);
        assert_eq!(prev.ts.second(), 0);
    }
}
