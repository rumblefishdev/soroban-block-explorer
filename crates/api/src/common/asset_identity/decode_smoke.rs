//! Wire-type contract for the icon read, asserted against a REAL ClickHouse.
//!
//! A read onto a side table is a new decode, and the failure mode is a 500 on a
//! live page rather than a compile error — the same class the sibling
//! statement's `toBool` and `CAST(… AS Array(Int64))` notes were paid for.
//!
//! Written so EMPTY tables still exercise every decode: the statements are
//! bounded by values that match nothing, so what is under test is that the
//! statement is accepted and its columns decode, independently of any fixture.
//!
//! ```text
//! docker compose up -d
//! CH_URL=http://localhost:8125 CH_USER=default CH_PASSWORD=clickhouse \
//!   cargo test -p api asset_identity
//! ```

use super::{AssetIdentityChRow, resolve_icons};

fn client() -> Option<clickhouse::Client> {
    let url = std::env::var("CH_URL").ok()?;
    let mut c = clickhouse::Client::default().with_url(url);
    if let Ok(u) = std::env::var("CH_USER") {
        c = c.with_user(u);
    }
    if let Ok(p) = std::env::var("CH_PASSWORD") {
        c = c.with_password(p);
    }
    Some(c)
}

/// One known identity row, in the shape the icon read keys on.
fn identity(
    id: i64,
    asset_type: i16,
    code: &str,
    issuer_id: i64,
    contract_id: i64,
) -> AssetIdentityChRow {
    AssetIdentityChRow {
        id,
        known: true,
        asset_type,
        asset_code: (!code.is_empty()).then(|| code.to_string()),
        issuer_id,
        contract_id,
        contract_strkey: None,
        symbol: None,
        decimals: Some(7),
    }
}

/// THE REAL STATEMENT, over one identity of every family — native (empty code,
/// no issuer), classic credit, and a Soroban token (empty code, a contract).
/// The empty-code and zero-issuer values are the ones a naive bound would let
/// through as a wildcard, so they belong in the fixture rather than outside it.
#[tokio::test]
async fn the_icon_statement_decodes_every_family() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping icon decode smoke");
        return;
    };
    let rows = [
        identity(-6_959_166_271_784_855_184, 0, "", 0, 0),
        identity(1, 1, "USDC", 42, 0),
        identity(2, 3, "", 0, 99),
    ];
    resolve_icons(&ch, &rows)
        .await
        .expect("the icon statement must be accepted and decode");
}

/// An empty input must not build a statement with an empty `IN ()`, which is a
/// syntax error rather than an empty result.
#[tokio::test]
async fn an_empty_input_makes_no_statement() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping empty-input smoke");
        return;
    };
    let got = resolve_icons(&ch, &[])
        .await
        .expect("empty input returns without querying");
    assert!(got.is_empty());
}
