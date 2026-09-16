//! Reading a `(holder_id, asset_id)`-keyed table in bounded slices — shared by
//! every table the checkpoint seed compares.

/// Number of `holder_id` slices. 64 keeps each chunk near the ~760k groups
/// measured for 1/64 of the key space — two orders under the server's limit.
pub(crate) const KEY_SLICES: i128 = 64;

/// The per-slice read. Its SELECT list is aliased to the FIELD NAMES of
/// [`crate::snapshot::verdict::OurRow`], which is what the driver matches on: `clickhouse` 0.15
/// builds a name-to-field mapping per cursor and returns `SchemaMismatch` on a
/// count mismatch or an unknown column, so a renamed alias fails the query
/// rather than shifting a column silently. Decoding is not positional — an
/// earlier version of this comment said it was, and two reviews built findings
/// on that sentence.
///
/// `filter` narrows the rows (`AND …`); `snapshot::claimable` reads its own table
/// through the same statement with none.
pub(crate) fn slice_sql(table: &str, filter: &str, from: i128, to: i128) -> String {
    // The aggregates are aliased INSIDE a subquery and renamed outside. Aliasing
    // `max(last_updated_ledger) AS last_updated_ledger` directly shadows the
    // column, so the next `argMax(..., last_updated_ledger)` binds the alias and
    // ClickHouse rejects it: "Aggregate function max(...) is found inside
    // another aggregate function" (ILLEGAL_AGGREGATION).
    //
    // ONE `argMax` over a TUPLE, not one per column. Two independent `argMax`
    // aggregates resolve a same-version tie independently, so in principle they
    // could take `amount` from one row and `closed_at_ledger` from another and
    // hand back a row that exists in no part on disk. Measured, they do not:
    // over a full key slice (762,955 keys, 19,142 ties argMax actually has to
    // resolve) the two forms returned identical results, because ClickHouse
    // keeps the first-encountered maximum and both states walk the same rows in
    // the same order. That is an implementation property, not a contract, and
    // the tuple costs nothing — so the guarantee is structural instead.
    //
    // Not a repair of the 1,238,583 known ties: every one of those carries
    // `closed_at_ledger = 0` on BOTH sides (they predate the column), so no
    // assembly of them can differ from a real row. This closes the shape a
    // FUTURE tie could take, once the deployed writer's stamps start appearing
    // at contended versions.
    format!(
        "SELECT holder_id, asset_id, tupleElement(best, 1) AS amount, \
                led AS last_updated_ledger, tupleElement(best, 2) AS closed_at_ledger \
         FROM ( \
             SELECT holder_id, \
                    asset_id, \
                    argMax((amount, closed_at_ledger), last_updated_ledger) AS best, \
                    max(last_updated_ledger) AS led \
             FROM {table} \
             WHERE holder_id BETWEEN {from} AND {to} {filter} \
             GROUP BY holder_id, asset_id \
         )"
    )
}

/// The slice boundaries, covering the i64 key space exactly once.
pub(crate) fn key_slices() -> impl Iterator<Item = (i128, i128)> {
    let lo = i128::from(i64::MIN);
    let hi = i128::from(i64::MAX);
    let step = (hi - lo + 1) / KEY_SLICES;
    (0..KEY_SLICES).map(move |s| {
        let from = lo + s * step;
        let to = if s == KEY_SLICES - 1 {
            hi
        } else {
            lo + (s + 1) * step - 1
        };
        (from, to)
    })
}
