use super::*;

const BALANCE: &str = "BAAD6DBUX6J22DMZOHIEZTEQ64CVCHEDRKWZONFEUL5Q26QD7R76RGR4TU";
const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

fn holding(amount: i64, ledger: u32, closed: bool) -> ExtractedClaimableBalance {
    ExtractedClaimableBalance {
        balance_id: BALANCE.to_string(),
        asset: ClaimableBalanceAsset::Credit {
            code: "USDC".to_string(),
            issuer: ISSUER.to_string(),
        },
        amount,
        ledger_sequence: ledger,
        closed,
    }
}

/// The writer inserts `BalanceRow` into both tables. RowBinary is positional, so
/// the moment one table's columns drift from the other's every row written here
/// is silently corrupt.
#[test]
fn table_columns_match_balances() {
    let columns = |table: &str| -> String {
        let start = crate::INIT_SQL
            .find(&format!("CREATE TABLE IF NOT EXISTS {table} ("))
            .unwrap_or_else(|| panic!("{table} missing from init.sql"));
        // The column list ends at the `)` that closes it on its own line.
        let body = &crate::INIT_SQL[start..];
        body[..body.find("\n)").unwrap()]
            .split_once('(')
            .unwrap()
            .1
            .to_string()
    };
    assert_eq!(columns("claimable_balance_holdings"), columns("balances"));
}

#[test]
fn keys_match_the_other_tables() {
    let rows = build_claimable_balance_rows(&[holding(5_000_000, 100, false)]);
    assert_eq!(rows.len(), 1);
    // The `asset_transfers` endpoint surrogate for the same `B…`, and the
    // trustline surrogate for the same asset — the joins the oracle and
    // `total_supply` rely on.
    assert_eq!(rows[0].holder_id, ids::address_id(BALANCE));
    assert_eq!(rows[0].asset_id, ids::credit_asset_id("USDC", ISSUER));
    assert_eq!(
        (
            rows[0].amount,
            rows[0].last_updated_ledger,
            rows[0].closed_at_ledger
        ),
        (5_000_000, 100, 0)
    );
}

/// Production `asset_transfers.from_id` for the AVLX claim in tx `23273fda…c7b8`
/// (ledger 64,438,024) — the same balance the xdr-parser real-corpus test
/// renders from its ledger entry. Equal surrogates are what let the oracle join
/// this table to `asset_transfers` per balance.
#[test]
fn holder_id_equals_the_asset_transfers_endpoint_on_mainnet() {
    assert_eq!(
        ids::address_id("BAAG4GHMU6GJVRBQLHS4LWHZPCOBJ6HYCUWBVZNQ7TPILPSGD6DQVTCBEM"),
        1_280_410_223_283_636_341
    );
}

#[test]
fn native_uses_the_native_surrogate() {
    let mut h = holding(1, 100, false);
    h.asset = ClaimableBalanceAsset::Native;
    assert_eq!(
        build_claimable_balance_rows(&[h])[0].asset_id,
        ids::NATIVE_ASSET_ID
    );
}

#[test]
fn claim_in_a_later_transaction_of_the_same_ledger_wins() {
    let rows =
        build_claimable_balance_rows(&[holding(5_000_000, 100, false), holding(0, 100, true)]);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].amount, rows[0].closed_at_ledger), (0, 100));
}

#[test]
fn different_ledgers_stay_separate_versions() {
    let rows =
        build_claimable_balance_rows(&[holding(5_000_000, 100, false), holding(0, 105, true)]);
    assert_eq!(rows.len(), 2);
}
