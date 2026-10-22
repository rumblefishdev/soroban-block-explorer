//! The asset detail page's SEP-1 text — `description` (the currency's `desc`)
//! and `home_page` (the issuer's `DOCUMENTATION.ORG_URL`) — as the enrichment
//! worker stored them from its stellar.toml read (task 0655). The API makes no
//! SEP-1 request of its own.

use clickhouse::Row;
use serde::Deserialize;

#[derive(Debug, Row, Deserialize)]
struct DetailTextRow {
    description: Option<String>,
    home_page: Option<String>,
}

/// `(description, home_page)` of the latest read for `asset_code` issued by the
/// `issuer` G-StrKey; `(None, None)` when nothing was read. The two values are
/// taken from one row as a tuple: `argMax` skips NULLs, so separate calls could
/// mix an older read's text into a newer read that has none.
pub(super) async fn fetch(
    client: &clickhouse::Client,
    asset_code: &str,
    issuer: &str,
) -> Result<(Option<String>, Option<String>), clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT tupleElement(latest, 1) AS description, \
                    tupleElement(latest, 2) AS home_page \
             FROM ( \
                 SELECT asset_type, argMax(tuple(description, home_page), version) AS latest \
                 FROM asset_enrichment \
                 WHERE asset_code = ? \
                   AND issuer_id = (SELECT id FROM accounts WHERE account_id = ? LIMIT 1) \
                 GROUP BY asset_type \
                 ORDER BY asset_type \
                 LIMIT 1 \
             )",
        )
        .bind(asset_code)
        .bind(issuer)
        .fetch_optional::<DetailTextRow>()
        .await?;
    Ok(match row {
        Some(r) => (r.description, r.home_page),
        None => (None, None),
    })
}

#[cfg(test)]
mod tests;
