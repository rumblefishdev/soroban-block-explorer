//! Pool state read from ledger entries (task 0374, step 7).
//!
//! Two `ContractData` shapes carry a pool's state, and BOTH are recognised by
//! shape, never by address (planes are per-deployment — router B writes to
//! its own):
//!
//! 1. **Plane `PoolData`** — the deployment's quote-input sheet, one entry
//!    per pool. Key `[Symbol("PoolData"), Address(pool)]`, value a map of
//!    `reserves` (vec of u128), `pool_type` (symbol) and `init_args`. NOT a
//!    reserve source any more (decision C′, task 0374): a stable pool writes
//!    `Reserves × PrecisionMul` there — the units its swap math runs in —
//!    which overstated one leg by 10× or 10^11× for the six non-empty
//!    mixed-decimal stable pools. Kept only to notice a pool whose instance
//!    carries no reserve key we read while the plane still shows reserves.
//! 2. **Pool instance storage** — written in the SAME transaction as
//!    `add_pool`, and on every later operation. Carries `TokenShare` (the
//!    share token, as state — the fundamental source that demoted the
//!    deposit⇄mint rule to a cross-check), `Plane`, `Router`, the token list
//!    and THE reserves, in raw units (T4's "plane state == reserves" held only
//!    where the multiplier is 1). A concentrated pool has NO `TokenShare`
//!    key — structurally, matching `share_id()` returning the pool itself.
//!
//! Values stay RAW (u128 as strings, verbatim symbols): scaling and
//! vocabulary-folding are read-time concerns. The plane spells the constant
//! product type `standard` while the event spells it `constant` — the third
//! live vocabulary for one concept; nothing here folds them.

use serde_json::Value;

use crate::scval::{address, map_get, symbol, typed, typed_str};
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
}

/// Decode a plane `PoolData` entry from a `ContractData` change: `owner` is
/// the plane contract, `key`/`val` are house typed-JSON ScVals
/// (`scval_to_typed_json` dialect: `{"type":"sym","value":…}`, map entries
/// `{"key":…,"value":…}`). `None` when the shape is not a plane entry — the
/// normal case for every other `ContractData` write on the chain.
pub fn parse_plane_pool_data(owner: &str, key: &Value, val: &Value) -> Option<PlanePoolData> {
    let parts = typed(key, "vec")?.as_array()?;
    if symbol(parts.first()?) != Some("PoolData") {
        return None;
    }
    let pool = address(parts.get(1)?)?.to_string();
    Some(PlanePoolData {
        plane: owner.to_string(),
        pool,
        reserves: raw_u128_vec(map_get(val, "reserves")?)?,
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
    /// `TotalShares` — the outstanding share supply, raw u128, same entry and
    /// same clock as the share token (write half of the "Total shares: —"
    /// gap, ranked item 1). `None` where the key is absent: structural for
    /// concentrated pools (nothing minted) and for the elastic layout.
    pub total_shares: Option<String>,
    /// The deployment's plane this pool reports to.
    pub plane: Option<String>,
    /// The registering router, as the pool itself records it.
    pub router: Option<String>,
    /// The pool's reserves in raw token units, in leg order, from whichever
    /// layout its code uses (`ReserveA`/`ReserveB`, `Reserves`,
    /// `Reserve0`/`Reserve1` — the only three in all 58 versions measured).
    /// Empty when the write carries no reserve key: administrative calls
    /// rewrite the instance too, and absence must never become zeros.
    pub reserves: Vec<String>,
}

/// Decode the pool-relevant slice of an instance-storage list (house typed
/// JSON: `[{"key":{"type":"vec","value":[{"type":"sym",…}]},"value":…},…]`).
///
/// Returns `None` unless the instance IS a router-family pool — recognised by
/// shape: it carries a `Plane` key, the deployment scoreboard every pool of
/// this family reports to.
///
/// **`Router` is NOT required, and requiring it was wrong.** The earlier rule
/// ("carries both `Router` and `Plane`") was measured on live creations only
/// and generalised to the whole population. Read from chain 2026-09-01: five
/// of the ten deployments run an older contract whose instance has `Plane`,
/// `TokenShare`, `ReserveA/B` and no `Router` key at all — 23 real pools.
/// Nothing states a guarantee either way: Stellar's docs say nothing about
/// trusting event or entry contents, the vendor's docs describe the roles
/// without promising the key, and the vendor's source is unreachable (404,
/// re-checked 2026-09-01). So a missing `Router` is an observed fact about an
/// older contract version, not evidence of a forgery, and must not be treated
/// as one.
///
/// Relaxing the shape test is safe because this decode is keyed on the entry's
/// OWNER: a foreign contract that writes a `Plane` key describes only itself,
/// and nothing reads an instance row for a pool that never reached the
/// registry.
pub fn parse_pool_instance(pool: &str, storage: &Value) -> Option<PoolInstanceState> {
    let entries = storage.as_array()?;
    let get = |name: &str| -> Option<&Value> {
        entries.iter().find_map(|kv| {
            let k = typed(kv.get("key")?, "vec")?.as_array()?;
            let is = k.len() == 1 && typed(&k[0], "sym").and_then(Value::as_str) == Some(name);
            is.then(|| kv.get("value"))?
        })
    };
    let plane = get("Plane")?;
    let router = get("Router");
    let addr = |v: &Value| address(v).map(str::to_string);
    let u128s = |v: &Value| typed_str(v, "u128").map(str::to_string);
    // Decision C′ (task 0374): the pool's own storage is the reserve source
    // for the whole family. Three layouts exist in every code version pools
    // have run (58 measured): `ReserveA`/`ReserveB` (constant, elastic),
    // `Reserves` (stable, one entry per leg) and `Reserve0`/`Reserve1`
    // (concentrated). All hold RAW token units; the plane row of a stable
    // pool carries `Reserves × PrecisionMul` instead. No key → no reserves,
    // never zeros: administrative calls rewrite the instance too.
    let pair = |a: &str, b: &str| Some(vec![get(a).and_then(u128s)?, get(b).and_then(u128s)?]);
    let reserves = pair("ReserveA", "ReserveB")
        .or_else(|| get("Reserves").and_then(raw_u128_vec))
        .or_else(|| pair("Reserve0", "Reserve1"))
        .unwrap_or_default();
    // A reserve key that no layout could read is a refusal, not an
    // administrative rewrite: without this line it is silent whenever the
    // plane does not write in the same ledger (staging's cross-check is the
    // only other signal).
    let unread = unread_reserve_keys(get, &reserves);
    if !unread.is_empty() {
        tracing::error!(
            pool,
            keys = ?unread,
            "pool instance carries reserve keys no known layout reads; no reserve row staged"
        );
    }
    Some(PoolInstanceState {
        pool: pool.to_string(),
        token_share: get("TokenShare").and_then(&addr),
        total_shares: get("TotalShares").and_then(u128s),
        plane: addr(plane),
        router: router.and_then(&addr),
        reserves,
    })
}

/// Reserve keys the instance carries although no layout could be read from
/// them — a value of an unexpected type, or half a pair. Empty when reserves
/// were read, or when the write carries no reserve key at all.
fn unread_reserve_keys<'a>(
    get: impl Fn(&str) -> Option<&'a Value>,
    reserves: &[String],
) -> Vec<&'static str> {
    if !reserves.is_empty() {
        return Vec::new();
    }
    // A `Reserves` vector that decodes is a read even when empty — a pool
    // with no liquidity yet — not a refusal.
    if get("Reserves").is_some_and(|v| raw_u128_vec(v).is_some()) {
        return Vec::new();
    }
    ["ReserveA", "ReserveB", "Reserves", "Reserve0", "Reserve1"]
        .into_iter()
        .filter(|k| get(k).is_some())
        .collect()
}

/// One plane `PoolData` write. Per-write coordinates (tx hash, change
/// index) were dropped with the grain collapse: rows are one-per-(pool,
/// ledger) and the last-in-apply-order pick needs only the vector order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedPlanePoolData {
    pub data: PlanePoolData,
    pub ledger_sequence: u32,
}

/// Extract plane `PoolData` writes from a transaction's ledger-entry changes
/// (task 0374, step 7). Since decision C′ not a reserve source: staging uses it
/// only to notice an instance layout whose reserve keys we do not read.
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
                ledger_sequence: change.ledger_sequence,
            });
        }
    }
    out
}

/// One pool-instance write (creation or config change) carrying the
/// state-sourced relations: share token, plane, router. Per-write
/// coordinates dropped with the grain collapse (see the plane twin above).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedPoolInstance {
    pub state: PoolInstanceState,
    pub ledger_sequence: u32,
    /// The entry was CREATED in this ledger (vs updated/restored). Load-bearing
    /// for the router-less registration arm: a genuine registration deploys
    /// and initialises the pool in ONE transaction (497/497 measured), so its
    /// instance is always a creation — while an attacker inducing a same-ledger
    /// instance write on an existing victim pool can only produce an update.
    pub created: bool,
    /// The reserves differ from the instance's pre-image in this transaction,
    /// or there is no pre-image to compare (a creation or a restore).
    pub reserves_changed: bool,
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
    // The `state` pre-image precedes its `updated` post-image within a
    // transaction; keeping its reserves lets rule 2 of decision C′ tell a
    // reserve move from an administrative rewrite of the same instance.
    let mut pre_image: std::collections::HashMap<&str, Vec<String>> =
        std::collections::HashMap::new();
    for change in changes {
        if change.entry_type != "contract_data" {
            continue;
        }
        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored" | "state"
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
        let Some(state) = parse_pool_instance(pool, storage) else {
            continue;
        };
        if change.change_type == "state" {
            pre_image.insert(pool, state.reserves);
            continue;
        }
        let reserves_changed = pre_image
            .remove(pool)
            .is_none_or(|before| before != state.reserves);
        out.push(ExtractedPoolInstance {
            state,
            ledger_sequence: change.ledger_sequence,
            created: change.change_type == "created",
            reserves_changed,
        });
    }
    out
}

/// `{"type":"vec","value":[{"type":"u128","value":"…"},…]}` → raw decimal
/// strings, order preserved.
fn raw_u128_vec(v: &Value) -> Option<Vec<String>> {
    typed(v, "vec")?
        .as_array()?
        .iter()
        .map(|e| typed_str(e, "u128").map(str::to_string))
        .collect()
}

// NO parser-side fold for either stream (decision karolkow 2026-09-01):
// plane writes and instance images both collapse in STAGING, one fold per
// destination table (`fold_pool_state_changes` / `fold_pool_instance_state`
// in db-clickhouse), symmetric and in one home. A parser-side pre-fold was
// a second copy with zero effect — staging order preserves ledger apply
// order end-to-end, so the stage folds see the same last-wins sequence.

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
    }

    /// Verbatim from the same ledger — a 5-init-arg stable pool entry still
    /// decodes (the extractor consumes only `reserves`; the other map keys
    /// must not confuse it).
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
        assert_eq!(got.reserves, vec!["7419859054", "9364494398"]);
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

    /// Verbatim shape of an OLDER deployment's pool, read from chain
    /// 2026-09-01 (`CD2X3JY7…`, one of five such deployments): `Plane`,
    /// `TokenShare` and reserves, but NO `Router` key. These are real pools;
    /// requiring `Router` dropped 23 of them.
    #[test]
    fn an_older_pool_without_a_router_key_is_still_a_pool() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Admin"}]},
             "value": {"type": "address", "value": "GAV5FBMKD2ZF4X2MGWDNQYUP7KFL7MRM6HZBY7HKQLB4BRHSCCX5J6VS"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CDYX2OSS4XYZUT2LWWH2NXOQMEFF4JSARGSF3NEB7RM5VOMUHE3X2UN2"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "TokenShare"}]},
             "value": {"type": "address", "value": "CC5PU23MKXHUFJKGG5FAUG7MFZX2KMWXPNZP26DDYW76VCB26UWMPEI6"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "TotalShares"}]}, "value": {"type": "u128", "value": "5"}}
        ]);
        let got = parse_pool_instance(
            "CD2X3JY7PWJBXUU6PB52K3O547L2NX35XUUKKKGYED3UF6FWFTV5NI3N",
            &storage,
        )
        .expect("an older pool is still a pool");
        assert_eq!(
            got.plane.as_deref(),
            Some("CDYX2OSS4XYZUT2LWWH2NXOQMEFF4JSARGSF3NEB7RM5VOMUHE3X2UN2"),
            "the plane is what makes reserve provenance checkable — it must survive"
        );
        assert_eq!(
            got.router, None,
            "absent, and reported as absent — never invented"
        );
        assert!(got.token_share.is_some());
        assert_eq!(
            got.total_shares.as_deref(),
            Some("5"),
            "TotalShares rides the same entry — raw u128, no scaling"
        );
    }

    /// Decision C′ (task 0374): a fungible pool's reserves are read from its
    /// OWN instance. Verbatim keys of mixed-decimal stable pool `CCI5UGNC…`
    /// read via `getLedgerEntries` on 2026-09-14 (last modified 64 393 803):
    /// storage holds RAW units, while its plane row carried the second leg
    /// × `PrecisionMul` (8287758700000000000) — the figure we used to store.
    #[test]
    fn a_stable_pool_reports_raw_reserves_from_its_own_storage() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Decimals"}]},
             "value": {"type": "vec", "value": [{"type": "u32", "value": 18}, {"type": "u32", "value": 7}]}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "PrecisionMul"}]},
             "value": {"type": "vec", "value": [{"type": "u128", "value": "1"}, {"type": "u128", "value": "100000000000"}]}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Reserves"}]},
             "value": {"type": "vec", "value": [{"type": "u128", "value": "22168059846400376042"}, {"type": "u128", "value": "82877587"}]}}
        ]);
        let got = parse_pool_instance(
            "CCI5UGNCHE5PBINLZKSFFCMBUVJYWYDPKDZS54JD6TGJBA7MCG3YXNT5",
            &storage,
        )
        .unwrap();
        assert_eq!(
            got.reserves,
            vec!["22168059846400376042", "82877587"],
            "raw token units, never the plane's normalised figure"
        );
    }

    /// Verbatim keys of constant pool `CDDLTOOD…` (last modified 63 116 736):
    /// the two-key layout, equal to the plane row for this pool.
    #[test]
    fn a_constant_pool_reports_reserves_from_its_own_storage() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "ProtocolFeeA"}]}, "value": {"type": "u128", "value": "1434978"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "ReserveA"}]}, "value": {"type": "u128", "value": "5560272127"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "ReserveB"}]}, "value": {"type": "u128", "value": "9066888378507"}}
        ]);
        let got = parse_pool_instance(
            "CDDLTOODRYTIJE4KPS4BIIXWPLVMSIYVG7GE4W7MG2UBQ3QCCET4SUFA",
            &storage,
        )
        .unwrap();
        assert_eq!(got.reserves, vec!["5560272127", "9066888378507"]);
    }

    /// An instance write that carries no reserve key (an older layout written
    /// by an administrative call) reports NO reserves — never zeros.
    #[test]
    fn an_instance_without_reserve_keys_reports_no_reserves() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "FeeFraction"}]}, "value": {"type": "u32", "value": 30}}
        ]);
        let got = parse_pool_instance("CPOOL", &storage).unwrap();
        assert!(got.reserves.is_empty());
    }

    /// Task 0559. A reserve key no layout can read is a refusal and must be
    /// named; an administrative rewrite without reserve keys is not.
    #[test]
    fn a_reserve_key_no_layout_reads_is_named() {
        let u128v = json!({"type": "u128", "value": "7"});
        let i128_vec = json!({"type": "vec", "value": [{"type": "i128", "value": "7"}]});
        let empty_vec = json!({"type": "vec", "value": []});
        let fee = json!({"type": "u32", "value": 30});
        let keys = |present: &[(&str, &Value)], reserves: &[String]| {
            unread_reserve_keys(
                |k| present.iter().find(|(n, _)| *n == k).map(|(_, v)| *v),
                reserves,
            )
        };

        // Wrong value type: `Reserves` holds i128s, nothing parsed.
        assert_eq!(keys(&[("Reserves", &i128_vec)], &[]), vec!["Reserves"]);
        // Half a pair.
        assert_eq!(keys(&[("ReserveA", &u128v)], &[]), vec!["ReserveA"]);
        // An empty `Reserves` that decodes is a pool with no liquidity yet.
        assert!(keys(&[("Reserves", &empty_vec)], &[]).is_empty());
        // No reserve key at all: an administrative rewrite, not a refusal.
        assert!(keys(&[("FeeFraction", &fee)], &[]).is_empty());
        // Reserves were read: nothing to report.
        let read = ["7".to_string(), "7".to_string()];
        assert!(keys(&[("ReserveA", &u128v), ("ReserveB", &u128v)], &read).is_empty());
    }

    fn instance_change(change_type: &str, reserve_a: &str, fee: u32) -> ExtractedLedgerEntryChange {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "FeeFraction"}]}, "value": {"type": "u32", "value": fee}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Plane"}]},
             "value": {"type": "address", "value": "CCABO2IQYDWRGGQ4DYQ73CV3ZFDBRZTEQNDDJMFT7JZO54CLS4RYJROY"}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "ReserveA"}]}, "value": {"type": "u128", "value": reserve_a}},
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "ReserveB"}]}, "value": {"type": "u128", "value": "500"}}
        ]);
        let key = json!({"type": "ledger_key_contract_instance"});
        ExtractedLedgerEntryChange {
            transaction_hash: "ab".repeat(32),
            change_type: change_type.to_string(),
            entry_type: "contract_data".to_string(),
            key: json!({"contract": "CPOOL", "key": key, "durability": "persistent"}),
            data: Some(json!({
                "contract": "CPOOL", "key": key, "durability": "persistent",
                "val": {"type": "contract_instance", "value": {"storage": storage}}
            })),
            change_index: 0,
            operation_index: Some(0),
            ledger_sequence: 64_400_000,
            created_at: 1_789_000_000,
            token_metadata: None,
        }
    }

    /// Rule 2 of decision C′: every administrative or reward call rewrites
    /// the instance; only a write that MOVES the reserves is a reserve event.
    #[test]
    fn an_instance_rewrite_with_unchanged_reserves_is_not_a_reserve_change() {
        let got = extract_pool_instances(&[
            instance_change("state", "100", 30),
            instance_change("updated", "100", 10),
        ]);
        assert_eq!(got.len(), 1, "the declaration still flows");
        assert!(!got[0].reserves_changed);
    }

    #[test]
    fn an_instance_rewrite_that_moves_the_reserves_is_a_change() {
        let got = extract_pool_instances(&[
            instance_change("state", "100", 30),
            instance_change("updated", "101", 30),
        ]);
        assert!(got[0].reserves_changed);
    }

    /// No pre-image to compare: a creation always counts, so a pool's first
    /// reserves are never lost.
    #[test]
    fn a_created_instance_is_a_reserve_change() {
        let got = extract_pool_instances(&[instance_change("created", "100", 30)]);
        assert!(got[0].reserves_changed);
    }

    /// An instance with no `Plane` is not of this family — the plane is the
    /// one key the shape test still rests on.
    #[test]
    fn an_instance_without_a_plane_is_not_a_pool() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Router"}]},
             "value": {"type": "address", "value": "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK"}}
        ]);
        assert_eq!(parse_pool_instance("CNOPLANE", &storage), None);
    }

    /// A random contract's instance (no `Plane`) is not a pool.
    #[test]
    fn a_foreign_instance_is_rejected_by_shape() {
        let storage = json!([
            {"key": {"type": "vec", "value": [{"type": "sym", "value": "Admin"}]}, "value": {"type": "address", "value": "GABC"}}
        ]);
        assert_eq!(parse_pool_instance("CFOREIGN", &storage), None);
    }
}
