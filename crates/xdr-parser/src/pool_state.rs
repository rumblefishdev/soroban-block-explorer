//! Pool state read from ledger entries (task 0374, step 7).
//!
//! Two `ContractData` shapes carry a pool's state, and BOTH are recognised by
//! shape, never by address (planes are per-deployment — router B writes to
//! its own):
//!
//! 1. **Plane `PoolData`** — the deployment's scoreboard, one entry per pool,
//!    updated on every pool action. Key `[Symbol("PoolData"), Address(pool)]`,
//!    value a map of `reserves` (vec of u128), `pool_type` (symbol) and
//!    `init_args`. This is THE reserve source for the whole timeline (T4;
//!    event arithmetic failed its oracle 6/49, the plane piloted 80/80).
//! 2. **Pool instance storage** — written in the SAME transaction as
//!    `add_pool`, and on later config changes. Carries `TokenShare` (the
//!    share token, as state — the fundamental source that demoted the
//!    deposit⇄mint rule to a cross-check), `Plane`, `Router`, the token list
//!    and mirror reserves. A concentrated pool has NO `TokenShare` key —
//!    structurally, matching `share_id()` returning the pool itself.
//!
//! Values stay RAW (u128 as strings, verbatim symbols): scaling and
//! vocabulary-folding are read-time concerns. The plane spells the constant
//! product type `standard` while the event spells it `constant` — the third
//! live vocabulary for one concept; nothing here folds them.

use serde_json::Value;

use crate::types::ExtractedLedgerEntryChange;

/// One plane `PoolData` write: a pool's reserves (and registration facts) as
/// the deployment's scoreboard records them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanePoolData {
    /// The plane contract that owns the entry (the emitter of truth).
    pub plane: String,
    /// The pool the entry describes.
    pub pool: String,
    /// Raw u128 reserve amounts, in leg order.
    pub reserves: Vec<String>,
    /// The plane's own spelling of the pool type (`standard`, `stable`, …).
    pub pool_type_raw: String,
    /// Raw constructor args as the plane mirrors them.
    pub init_args: Vec<String>,
}

/// Decode a plane `PoolData` entry from a `ContractData` change: `owner` is
/// the plane contract, `key`/`val` are house typed-JSON ScVals
/// (`scval_to_typed_json` dialect: `{"type":"sym","value":…}`, map entries
/// `{"key":…,"value":…}`). `None` when the shape is not a plane entry — the
/// normal case for every other `ContractData` write on the chain.
pub fn parse_plane_pool_data(owner: &str, key: &Value, val: &Value) -> Option<PlanePoolData> {
    let parts = typed(key, "vec")?.as_array()?;
    if typed(parts.first()?, "sym")?.as_str()? != "PoolData" {
        return None;
    }
    let pool = typed(parts.get(1)?, "address")?.as_str()?.to_string();
    let map = typed(val, "map")?.as_array()?;
    let field = |name: &str| -> Option<&Value> {
        map.iter()
            .find(|kv| {
                kv.get("key")
                    .and_then(|k| typed(k, "sym"))
                    .and_then(Value::as_str)
                    == Some(name)
            })
            .and_then(|kv| kv.get("value"))
    };
    Some(PlanePoolData {
        plane: owner.to_string(),
        pool,
        reserves: raw_u128_vec(field("reserves")?)?,
        pool_type_raw: typed(field("pool_type")?, "sym")?.as_str()?.to_string(),
        init_args: field("init_args")
            .and_then(raw_u128_vec)
            .unwrap_or_default(),
    })
}

/// The slice of a pool's instance storage this task consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolInstanceState {
    /// The pool contract whose instance this is.
    pub pool: String,
    /// `TokenShare` — the share token as state. `None` is STRUCTURAL for
    /// concentrated pools, which have no such key.
    pub token_share: Option<String>,
    /// The deployment's plane this pool reports to.
    pub plane: Option<String>,
    /// The registering router, as the pool itself records it.
    pub router: Option<String>,
    /// Per-operation reserves for a CONCENTRATED pool (`Reserve0`/`Reserve1`
    /// in instance storage). Concentrated pools do NOT update the plane per
    /// operation — measured on a hot ledger: 8 instance writes, zero plane
    /// writes for the busiest concentrated pool — so for them the instance IS
    /// the reserve stream, refining T4's claim (whose pilot predates the
    /// first concentrated pool). Empty for fungible pools: their `ReserveA/B`
    /// mirror is a corroborator only, the plane is their source, and staging
    /// both would duplicate snapshot rows.
    pub reserves: Vec<String>,
}

/// Decode the pool-relevant slice of an instance-storage list (house typed
/// JSON: `[{"key":{"type":"vec","value":[{"type":"sym",…}]},"value":…},…]`).
///
/// Returns `None` unless the instance IS a router-family pool — recognised by
/// shape: it carries both `Router` and `Plane` keys (measured on live
/// creations; no other contract family writes that pair).
pub fn parse_pool_instance(pool: &str, storage: &Value) -> Option<PoolInstanceState> {
    let entries = storage.as_array()?;
    let get = |name: &str| -> Option<&Value> {
        entries.iter().find_map(|kv| {
            let k = typed(kv.get("key")?, "vec")?.as_array()?;
            let is = k.len() == 1 && typed(&k[0], "sym").and_then(Value::as_str) == Some(name);
            is.then(|| kv.get("value"))?
        })
    };
    let router = get("Router")?;
    let plane = get("Plane")?;
    let addr = |v: &Value| {
        typed(v, "address")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let u128s = |v: &Value| typed(v, "u128").and_then(Value::as_str).map(str::to_string);
    // Reserve0/Reserve1 is the CONCENTRATED layout; fungible pools use
    // ReserveA/ReserveB, deliberately not read here — the plane is their
    // source, and reading both would double-write snapshots.
    let reserves = match (
        get("Reserve0").and_then(u128s),
        get("Reserve1").and_then(u128s),
    ) {
        (Some(r0), Some(r1)) => vec![r0, r1],
        _ => Vec::new(),
    };
    Some(PoolInstanceState {
        pool: pool.to_string(),
        token_share: get("TokenShare").and_then(&addr),
        plane: addr(plane),
        router: addr(router),
        reserves,
    })
}

/// One plane `PoolData` write, with the coordinates the snapshot key needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedPlanePoolData {
    pub data: PlanePoolData,
    pub transaction_hash: String,
    /// Zero-based change index within the transaction — the intra-tx
    /// tiebreaker of the snapshot key (a pool updates up to 12x per ledger).
    pub change_index: u32,
    pub ledger_sequence: u32,
}

/// Extract plane `PoolData` writes from a transaction's ledger-entry changes
/// (task 0374, step 7 — the reserve source, T4).
///
/// Mirrors `extract_soroban_token_balances`: only `created`/`updated`/
/// `restored` carry a value; the `state` pre-image is skipped (same-ledger
/// clobber) and `removed` has nothing to record.
pub fn extract_plane_pool_data(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedPlanePoolData> {
    let mut out = Vec::new();
    for change in changes {
        if change.entry_type != "contract_data" {
            continue;
        }
        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored"
        ) {
            continue;
        }
        let Some(owner) = change.key.get("contract").and_then(Value::as_str) else {
            continue;
        };
        let (Some(key), Some(val)) = (
            change.key.get("key"),
            change.data.as_ref().and_then(|d| d.get("val")),
        ) else {
            continue;
        };
        if let Some(data) = parse_plane_pool_data(owner, key, val) {
            out.push(ExtractedPlanePoolData {
                data,
                transaction_hash: change.transaction_hash.clone(),
                change_index: change.change_index,
                ledger_sequence: change.ledger_sequence,
            });
        }
    }
    out
}

/// One pool-instance write (creation or config change) carrying the
/// state-sourced relations: share token, plane, router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedPoolInstance {
    pub state: PoolInstanceState,
    pub transaction_hash: String,
    /// Intra-tx tiebreaker for instance-sourced snapshot rows (a concentrated
    /// pool's instance is rewritten per operation — 8x in one hot ledger).
    pub change_index: u32,
    pub ledger_sequence: u32,
}

/// Extract router-family pool instances from ledger-entry changes.
///
/// The instance is written in the SAME transaction as `add_pool` (probed on
/// raw meta), so registration-time facts — `TokenShare`, `Plane` — arrive as
/// state, no deposit needed. Later instance rewrites (the 13 measured
/// share-token migrations) flow through the same arm and converge in the RMT
/// side table by ledger version.
pub fn extract_pool_instances(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedPoolInstance> {
    let mut out = Vec::new();
    for change in changes {
        if change.entry_type != "contract_data" {
            continue;
        }
        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored"
        ) {
            continue;
        }
        // The instance entry's key is the `ledger_key_contract_instance`
        // sentinel; the storage rides inside the value's contract_instance.
        let key_is_instance = change
            .key
            .get("key")
            .and_then(|k| k.get("type"))
            .and_then(Value::as_str)
            == Some("ledger_key_contract_instance");
        if !key_is_instance {
            continue;
        }
        let Some(pool) = change.key.get("contract").and_then(Value::as_str) else {
            continue;
        };
        let Some(storage) = change
            .data
            .as_ref()
            .and_then(|d| d.get("val"))
            .and_then(|v| typed(v, "contract_instance"))
            .and_then(|ci| ci.get("storage"))
        else {
            continue;
        };
        if let Some(state) = parse_pool_instance(pool, storage) {
            out.push(ExtractedPoolInstance {
                state,
                transaction_hash: change.transaction_hash.clone(),
                change_index: change.change_index,
                ledger_sequence: change.ledger_sequence,
            });
        }
    }
    out
}

/// `value` of a house typed-JSON node, iff its `type` tag matches.
fn typed<'a>(v: &'a Value, tag: &str) -> Option<&'a Value> {
    (v.get("type")?.as_str()? == tag).then(|| v.get("value"))?
}

/// `{"type":"vec","value":[{"type":"u128","value":"…"},…]}` → raw decimal
/// strings, order preserved.
fn raw_u128_vec(v: &Value) -> Option<Vec<String>> {
    typed(v, "vec")?
        .as_array()?
        .iter()
        .map(|e| typed(e, "u128").and_then(Value::as_str).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verbatim from mainnet — plane `CCABO2IQ…`, registration ledger
    /// 63 893 403. Note the plane spells the type `standard`, not `constant`.
    #[test]
    fn decodes_a_real_plane_entry() {
        let key = json!({"type": "vec", "value": [
            {"type": "sym", "value": "PoolData"},
            {"type": "address", "value": "CBMWU3574VFWNBNMNYAAH4OBT7DPB27URDW4BWIV7XAPQG6YYMJW2LSH"}
        ]});
        let val = json!({"type": "map", "value": [
            {"key": {"type": "sym", "value": "init_args"}, "value": {"type": "vec", "value": [{"type": "u128", "value": "10"}]}},
            {"key": {"type": "sym", "value": "pool_type"}, "value": {"type": "sym", "value": "standard"}},
            {"key": {"type": "sym", "value": "reserves"}, "value": {"type": "vec", "value": [{"type": "u128", "value": "100000000000"}, {"type": "u128", "value": "30617317"}]}}
        ]});

        let got = parse_plane_pool_data(
            "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY",
            &key,
            &val,
        )
        .expect("real plane entry decodes");
        assert_eq!(
            got.pool,
            "CBMWU3574VFWNBNMNYAAH4OBT7DPB27URDW4BWIV7XAPQG6YYMJW2LSH"
        );
        assert_eq!(got.reserves, vec!["100000000000", "30617317"]);
        assert_eq!(
            got.pool_type_raw, "standard",
            "the plane's OWN vocabulary survives verbatim"
        );
        assert_eq!(got.init_args, vec!["10"]);
    }

    /// Verbatim from the same ledger — a 5-argument stable pool entry.
    #[test]
    fn keeps_every_stable_init_arg() {
        let key = json!({"type": "vec", "value": [
            {"type": "sym", "value": "PoolData"},
            {"type": "address", "value": "CCNXGPE4AQCSNEBZO3XJDKKDI3CRLYMVS6UWBBTVDLALLWMJEXBORQ2A"}
        ]});
        let val = json!({"type": "map", "value": [
            {"key": {"type": "sym", "value": "init_args"}, "value": {"type": "vec", "value": [
                {"type": "u128", "value": "10"}, {"type": "u128", "value": "1500"}, {"type": "u128", "value": "1764349837"},
                {"type": "u128", "value": "1500"}, {"type": "u128", "value": "1764349837"}
            ]}},
            {"key": {"type": "sym", "value": "pool_type"}, "value": {"type": "sym", "value": "stable"}},
            {"key": {"type": "sym", "value": "reserves"}, "value": {"type": "vec", "value": [{"type": "u128", "value": "7419859054"}, {"type": "u128", "value": "9364494398"}]}}
        ]});
        let got = parse_plane_pool_data(
            "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY",
            &key,
            &val,
        )
        .unwrap();
        assert_eq!(
            got.init_args.len(),
            5,
            "no argument may be dropped or shifted"
        );
    }

    #[test]
    fn other_contract_data_is_not_a_plane_entry() {
        let key = json!({"type": "vec", "value": [{"type": "sym", "value": "Balance"}, {"type": "address", "value": "GABC"}]});
        assert_eq!(parse_plane_pool_data("C", &key, &json!({})), None);
    }

    /// Verbatim slice of the real constant-pool instance at creation
    /// (ledger 63 893 403): TokenShare, Plane and Router all present.
    #[test]
    fn reads_token_share_from_a_real_instance() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Router"}]},
             "value": {"type": "address", "value": "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "TokenShare"}]},
             "value": {"type": "address", "value": "CC5PU23MKXHUFJKGG5FAUG7MFZX2KMWXPNZP26DDYW76VCB26UWMPEI6"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "FeeFraction"}]}, "value": {"type": "u32", "value": 10}}
        ]);
        let got = parse_pool_instance(
            "CBMWU3574VFWNBNMNYAAH4OBT7DPB27URDW4BWIV7XAPQG6YYMJW2LSH",
            &storage,
        )
        .expect("family pool instance");
        assert_eq!(
            got.token_share.as_deref(),
            Some("CC5PU23MKXHUFJKGG5FAUG7MFZX2KMWXPNZP26DDYW76VCB26UWMPEI6"),
            "the share token as STATE, at birth"
        );
        assert!(got.plane.is_some() && got.router.is_some());
    }

    /// A concentrated pool's instance (probed live at ledger 64 134 576) has
    /// no TokenShare key — the absence is structural and must come back as
    /// None, never as a fabricated value.
    #[test]
    fn concentrated_instance_has_no_share_token() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]}, "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Router"}]}, "value": {"type": "address", "value": "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "TickSpacing"}]}, "value": {"type": "u32", "value": 60}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Slot0"}]}, "value": {"type": "u128", "value": "0"}}
        ]);
        let got = parse_pool_instance(
            "CC642QYWXXR2HUZDNJ6KYN5LV5JFPFPT4Q6YNKLZLYEFWZZZ5SJYLA5G",
            &storage,
        )
        .unwrap();
        assert_eq!(got.token_share, None);
    }

    /// A random contract's instance (no Router+Plane pair) is not a pool.
    #[test]
    fn a_foreign_instance_is_rejected_by_shape() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Admin"}]}, "value": {"type": "address", "value": "GABC"}}
        ]);
        assert_eq!(parse_pool_instance("CFOREIGN", &storage), None);
    }
}
