use super::*;
use serde_json::json;

const POOL_HEX: &str = "41270552ed479ebf3e86c420f1d9401d4cde64720b4c201bf9a5352e36bf05cf";
const TF_ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";

fn atom(sold: &str, amount_sold: i64, bought: &str, amount_bought: i64) -> Value {
    json!({
        "poolId": POOL_HEX,
        "assetSold": sold,
        "amountSold": amount_sold,
        "assetBought": bought,
        "amountBought": amount_bought,
    })
}

/// The pool SOLD native and BOUGHT the credit asset, twice in one op —
/// exactly the CAP-38 interleaved take that makes per-atom rows unsafe.
/// Both legs must fold into one row each, native negative (left the pool)
/// and credit positive (entered it).
#[test]
fn sums_repeated_takes_of_one_pool_and_signs_from_the_pools_side() {
    let tf = format!("TF:{TF_ISSUER}");
    let details = json!({
        "claimedAtoms": [
            atom("native", 1_396_629, &tf, 5_596_939),
            atom("native", 2_548_470, &tf, 10_208_417),
        ],
    });

    let native_id = ids::NATIVE_ASSET_ID;
    let tf_id = ids::credit_asset_id("TF", TF_ISSUER);
    let pool_id = <[u8; 32]>::try_from(hex::decode(POOL_HEX).unwrap().as_slice()).unwrap();

    let mut got = pool_fill_amounts(&details);
    got.sort_unstable_by_key(|(_, asset_id, _)| *asset_id);
    let mut want = vec![
        (pool_id, native_id, -(1_396_629 + 2_548_470)),
        (pool_id, tf_id, 5_596_939 + 10_208_417),
    ];
    want.sort_unstable_by_key(|(_, asset_id, _)| *asset_id);
    assert_eq!(got, want);
}

/// Deposits and withdrawals arrive as one already-signed `poolDelta`
/// instead of atoms, and must land in the same rows.
#[test]
fn reads_deposit_and_withdrawal_deltas() {
    let tf = format!("TF:{TF_ISSUER}");
    let details = json!({
        "poolDelta": {
            "poolId": POOL_HEX,
            "assetA": "native",
            "amountA": -1_000,
            "assetB": tf,
            "amountB": -2_000,
        },
    });

    let pool_id = <[u8; 32]>::try_from(hex::decode(POOL_HEX).unwrap().as_slice()).unwrap();
    let got = pool_fill_amounts(&details);
    assert_eq!(got.len(), 2);
    assert!(got.contains(&(pool_id, ids::NATIVE_ASSET_ID, -1_000)));
    assert!(got.contains(&(pool_id, ids::credit_asset_id("TF", TF_ISSUER), -2_000)));
}

#[test]
fn no_atoms_no_rows() {
    assert!(pool_fill_amounts(&json!({})).is_empty());
    assert!(pool_fill_amounts(&json!({ "claimedAtoms": [] })).is_empty());
}
