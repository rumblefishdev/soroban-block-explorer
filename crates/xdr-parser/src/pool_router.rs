//! Router-registry pool discovery — decoding `add_pool` events (task 0374).
//!
//! # Scope: one discovery mechanism, not every AMM
//!
//! This covers the **router-registry** family only. A router contract announces
//! each new pool with `add_pool`, and emitting one **is** what makes a contract
//! a router — 10 contracts on mainnet do, all of one protocol family, together
//! registering 497 pools (count taken 2026-08-27; the registry is live and this
//! number moves).
//!
//! It does **not** cover AMMs that have no router registry. Soroswap pairs, for
//! instance, announce themselves — `("SoroswapPair", "deposit")` — with no
//! registry event anywhere, so they need their own discovery entirely
//! (task 0518, and blocked on 0517 because their event name sits in topic 1).
//!
//! Per-protocol decoding is not a design preference: Soroban defines no AMM
//! standard, so each protocol names and lays out its events differently and
//! there is nothing universal to decode. What *is* shared — the pool model,
//! the leg list, reserve storage, the position shape — is shared (task 0516);
//! reading events is the only place the chain forces a difference.
//! Classic protocol pools are not contracts at all and never reach here.
//!
//! **Shape-driven, never address-driven.** Nothing here matches a contract
//! address: the vendor documents one router, and keying on that address
//! silently drops ~6 % of live pools — see the verification pass in task 0374.
//!
//! # Payload — from the vendor's own source, not inferred
//!
//! `liquidity_pool_router/src/events.rs` publishes:
//!
//! ```rust,ignore
//! self.env().events().publish(
//!     (Symbol::new(self.env(), "add_pool"), tokens),       // topics
//!     (pool_address, pool_type, subpool_salt, init_args),  // data
//! );
//! ```
//!
//! which reaches us as ScVal JSON:
//!
//! ```text
//! topics = [ {sym "add_pool"}, {vec [ {address token}, … ]} ]
//! data   = {vec [ {address pool_address},
//!                 {sym pool_type},
//!                 {bytes subpool_salt},
//!                 {vec [ {u32 fee}, … ]} ]}
//! ```
//!
//! Corroboration, 2026-08-27: the vendor repository is unreachable (404 on
//! every branch and through the API), so the snippet above comes from an
//! archived capture (research 0008, fetched 2026-03-27) and **cannot be
//! re-confirmed at source today**. Two independent checks stand in for it: the
//! deployed contract's own spec, pulled from chain, agrees on every part it
//! covers — `pool_type` really is a `Symbol`, and the `BytesN<32>` really is
//! the router's pool lookup key. Note the spec carries **no event definitions
//! at all**, so an event's shape can only ever be confirmed by source or by
//! observation; there is no third route.
//!
//! Structural conformance measured 2026-08-27: **497 of 497** `add_pool`
//! events in all history match this shape, across all ten emitting contracts,
//! with pool types drawn from one vocabulary. No false positives exist today —
//! but the shape is the only guard, so a contract using the name for something
//! else with the same layout would be accepted. Behaviour confirms what the
//! registry only claims: 23 registrations from five dead deployments have
//! never emitted a single pool event, so a caller should treat a registration
//! as a candidate and let activity confirm it.
//!
//! Token arity tracks the pool: two-token pools carry two addresses, the
//! three-token stable pools carry three. **The leg list is a list** — treating
//! it as a pair is the classic-AMM assumption that does not survive here.

use serde_json::Value;

/// One decoded `add_pool` event: a pool joining a router's registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPoolEvent {
    /// Pool contract StrKey (`C…`).
    pub pool: String,
    /// Pool shape under one spelling, or `None` when the on-chain string is
    /// one nobody has catalogued. `None` is a signal to count, never a reason
    /// to drop the registration — the pool is real either way.
    pub pool_type: Option<&'static str>,
    /// The spelling exactly as the router wrote it. Kept even when it
    /// normalises, because two sources disagree on it (`constant` here vs
    /// `standard` in pool state) and flattening that at write time would hide
    /// a real divergence.
    pub pool_type_raw: String,
    /// Pool legs, in the order the router registered them. This order is the
    /// one the pool's own `get_tokens()` reports, so reserve vectors line up
    /// with it index-for-index and need no reordering.
    pub tokens: Vec<String>,
    /// `subpool_salt` — the key by which the router addresses this pool
    /// within its token set. Confirmed against the deployed contract's own
    /// spec, pulled from chain: `get_pools(tokens) -> Map<BytesN<32>, Address>`
    /// and `pool_type(tokens, pool_index: BytesN<32>) -> Symbol` both take
    /// exactly this value as the lookup key.
    ///
    /// **Not a WASM hash** — joining it against `soroban_contracts.wasm_hash`
    /// matches nothing, or matches wrongly.
    /// Decoded to bytes; `None` when absent or not 32 bytes.
    pub subpool_salt: Option<[u8; 32]>,
    /// `init_args` — the arguments the router passed when constructing the
    /// pool. First element is the fee in basis points for every pool type
    /// observed so far; `concentrated` pools carry a second value (tick
    /// spacing). Kept as a list rather than named fields because the arity is
    /// a property of the pool type, and the vendor's own signature types it as
    /// an untyped `Vec<Val>`.
    pub init_args: Vec<i64>,
}

/// Decode an `add_pool` event from its topics and data payloads.
///
/// Returns `None` when the event is not a well-formed `add_pool` — a caller
/// staging events should count those rather than drop them silently, since a
/// rejected registration is a pool that would otherwise go missing.
pub fn parse_add_pool(topics: &Value, data: &Value) -> Option<AddPoolEvent> {
    let topics = topics.as_array()?;
    if symbol_value(topics.first()?)? != "add_pool" {
        return None;
    }

    // Legs ride in topic 1 as a vec of addresses.
    let tokens: Vec<String> = vec_elements(topics.get(1)?)?
        .iter()
        .map(address_value)
        .collect::<Option<_>>()?;
    if tokens.is_empty() {
        return None;
    }

    let fields = vec_elements(data)?;
    let pool = address_value(fields.first()?)?;
    let pool_type_raw = symbol_value(fields.get(1)?)?.to_string();

    // Remaining fields are positional and have been stable across all history,
    // but a shorter payload is a decodable registration all the same: the pool
    // and its legs are what the registry is for.
    let subpool_salt = fields
        .get(2)
        .and_then(typed_str("bytes"))
        .and_then(decode_bytes32);
    let init_args = fields
        .get(3)
        .and_then(vec_elements)
        .map(|items| items.iter().filter_map(int_value).collect())
        .unwrap_or_default();

    Some(AddPoolEvent {
        pool,
        pool_type: normalise_pool_type(&pool_type_raw),
        pool_type_raw,
        tokens,
        subpool_salt,
        init_args,
    })
}

/// Collapse the spellings of a pool shape onto one.
///
/// Not a closed domain: these strings are chosen by a third-party contract,
/// Soroban defines no AMM standard, and a deployment can introduce a shape
/// without telling anyone. So this is deliberately **not** one of the
/// `domain::enums` — those pin closed protocol domains to a SMALLINT column,
/// and pretending this is one would mean editing an enum every time the chain
/// surprises us.
///
/// Two spellings for one shape are already live: the router's `add_pool` says
/// `constant` while the pool-state entry for the same pool says `standard`.
/// Total by construction — an unrecognised string yields `None` rather than a
/// plausible wrong value, and the caller keeps `pool_type_raw` and counts it.
fn normalise_pool_type(raw: &str) -> Option<&'static str> {
    match raw {
        "constant" | "standard" => Some("constant"),
        "stable" => Some("stable"),
        "concentrated" => Some("concentrated"),
        "elastic" => Some("elastic"),
        _ => None,
    }
}

/// Base64 → exactly 32 bytes. A payload of any other length is not the salt
/// the vendor's `BytesN<32>` promises; truncating or padding one would produce
/// a value that looks usable and identifies the wrong pool slot.
fn decode_bytes32(b64: &str) -> Option<[u8; 32]> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()?
        .try_into()
        .ok()
}

/// Elements of a `{"type":"vec","value":[…]}` payload.
fn vec_elements(v: &Value) -> Option<&[Value]> {
    has_type(v, "vec").then_some(())?;
    v.get("value").and_then(Value::as_array).map(Vec::as_slice)
}

/// Type-tag check on its own. Kept separate from reading the payload because
/// `value` is a string for scalars and an array for `vec` — conflating the two
/// makes every well-formed vec look malformed.
fn has_type(v: &Value, tag: &str) -> bool {
    v.get("type").and_then(Value::as_str) == Some(tag)
}

fn address_value(v: &Value) -> Option<String> {
    typed_str("address")(v)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn symbol_value(v: &Value) -> Option<&str> {
    typed_str("sym")(v).filter(|s| !s.is_empty())
}

/// Numeric ScVal JSON carries its value as a string for the wide types and as
/// a JSON number for the narrow ones; accept both.
fn int_value(v: &Value) -> Option<i64> {
    let raw = v.get("value")?;
    raw.as_i64().or_else(|| raw.as_str()?.parse().ok())
}

/// Curried string reader for scalar ScVal JSON: `typed_str("address")(v)`
/// yields the `value` string only when `v` carries that type tag.
fn typed_str(tag: &'static str) -> impl Fn(&Value) -> Option<&str> {
    move |v: &Value| {
        has_type(v, tag).then_some(())?;
        v.get("value").and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verbatim from mainnet — router `CBQDHNBF…6QUK`, a two-token constant
    /// pool. Kept exact so a payload-shape change fails here rather than in
    /// production.
    fn constant_pool_event() -> (Value, Value) {
        let topics = json!([
            {"type": "sym", "value": "add_pool"},
            {"type": "vec", "value": [
                {"type": "address", "value": "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"},
                {"type": "address", "value": "CDLWTKL7XIALOQPTV7R2KKTXTA6OPKT4T354Y7RG7S6TERQ7KI2VPXIW"}
            ]}
        ]);
        let data = json!({"type": "vec", "value": [
            {"type": "address", "value": "CDTSSTLKVVPWJZXVCGJJNGWKH5MY7OMINVXTB7DGFMDJTCCDBCSRG52O"},
            {"type": "sym", "value": "constant"},
            {"type": "bytes", "value": "suAvz8pslvitXL2E53hKd3s22clqJFlALE9FhGKqt/A="},
            {"type": "vec", "value": [{"type": "u32", "value": 10}]}
        ]});
        (topics, data)
    }

    #[test]
    fn decodes_a_mainnet_constant_pool() {
        let (topics, data) = constant_pool_event();
        let ev = parse_add_pool(&topics, &data).expect("well-formed add_pool");

        assert_eq!(
            ev.pool,
            "CDTSSTLKVVPWJZXVCGJJNGWKH5MY7OMINVXTB7DGFMDJTCCDBCSRG52O"
        );
        assert_eq!(ev.pool_type, Some("constant"));
        assert_eq!(ev.pool_type_raw, "constant", "raw spelling is preserved");
        assert_eq!(ev.tokens.len(), 2);
        assert_eq!(
            ev.tokens[0],
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        );
        assert_eq!(ev.init_args, vec![10]);
        assert!(ev.subpool_salt.is_some(), "32-byte subpool salt decodes");
    }

    /// Three-token stable pools exist on mainnet. The leg list must carry all
    /// three — a pair-shaped decoder would silently lose the third asset.
    #[test]
    fn keeps_all_three_legs_of_a_stable_pool() {
        let topics = json!([
            {"type": "sym", "value": "add_pool"},
            {"type": "vec", "value": [
                {"type": "address", "value": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"},
                {"type": "address", "value": "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"},
                {"type": "address", "value": "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC"}
            ]}
        ]);
        let data = json!({"type": "vec", "value": [
            {"type": "address", "value": "CDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD"},
            {"type": "sym", "value": "stable"},
            {"type": "bytes", "value": "AAAA"},
            {"type": "vec", "value": [{"type": "u32", "value": 6}]}
        ]});

        let ev = parse_add_pool(&topics, &data).expect("three-token stable pool");
        assert_eq!(ev.tokens.len(), 3, "third leg must survive decoding");
        assert_eq!(ev.pool_type, Some("stable"));
    }

    /// A pool type nobody has catalogued must arrive as itself. `elastic` is
    /// real — one router registers it — and was absent from the first pass of
    /// this research; the next unknown type must not fare worse.
    #[test]
    fn passes_through_an_unknown_pool_type() {
        let (topics, mut data) = constant_pool_event();
        data["value"][1] = json!({"type": "sym", "value": "elastic"});

        let ev = parse_add_pool(&topics, &data).expect("unknown type still decodes");
        assert_eq!(
            ev.pool_type,
            Some("elastic"),
            "no default arm may swallow this"
        );
    }

    /// Concentrated pools carry a second parameter beside the fee.
    #[test]
    fn keeps_every_trailing_parameter() {
        let (topics, mut data) = constant_pool_event();
        data["value"][1] = json!({"type": "sym", "value": "concentrated"});
        data["value"][3] = json!({"type": "vec", "value": [
            {"type": "u32", "value": 10},
            {"type": "u32", "value": 20}
        ]});

        let ev = parse_add_pool(&topics, &data).expect("concentrated pool");
        assert_eq!(
            ev.init_args,
            vec![10, 20],
            "tick spacing must not be dropped"
        );
    }

    /// A shape nobody has catalogued must still register the pool: `None` is
    /// a counter to increment, not a reason to lose a real pool.
    #[test]
    fn an_uncatalogued_shape_still_registers_the_pool() {
        let (topics, mut data) = constant_pool_event();
        data["value"][1] = json!({"type": "sym", "value": "hyperbolic"});

        let ev = parse_add_pool(&topics, &data).expect("pool is still real");
        assert_eq!(ev.pool_type, None, "no plausible wrong variant");
        assert_eq!(
            ev.pool_type_raw, "hyperbolic",
            "raw survives for the counter"
        );
        assert!(!ev.pool.is_empty(), "the pool itself must not be dropped");
    }

    /// A hash of the wrong length must not be truncated into one that would
    /// join against some other contract.
    #[test]
    fn refuses_a_wrong_length_salt() {
        let (topics, mut data) = constant_pool_event();
        data["value"][2] = json!({"type": "bytes", "value": "AAAA"});

        let ev = parse_add_pool(&topics, &data).expect("registration still decodes");
        assert_eq!(ev.subpool_salt, None);
    }

    /// The pool-state entry spells `constant` as `standard`; both must land on
    /// one value or every query needs to know about the disagreement.
    #[test]
    fn both_spellings_of_constant_collapse() {
        assert_eq!(normalise_pool_type("constant"), Some("constant"));
        assert_eq!(normalise_pool_type("standard"), Some("constant"));
    }

    #[test]
    fn every_measured_shape_normalises() {
        for raw in ["constant", "standard", "stable", "concentrated", "elastic"] {
            assert!(
                normalise_pool_type(raw).is_some(),
                "{raw} is live on mainnet"
            );
        }
    }

    #[test]
    fn an_unknown_spelling_yields_none() {
        assert_eq!(normalise_pool_type("hyperbolic"), None);
        assert_eq!(
            normalise_pool_type("Constant"),
            None,
            "unexpected case is a real signal"
        );
    }

    #[test]
    fn rejects_other_events() {
        let (_, data) = constant_pool_event();
        let topics = json!([{"type": "sym", "value": "trade"}]);
        assert_eq!(parse_add_pool(&topics, &data), None);
    }

    /// Soroswap-shaped events put a protocol label in topic 0 and the name in
    /// topic 1. This decoder must not claim them — that convention is task
    /// 0517's, and a false positive here would register junk pools.
    #[test]
    fn rejects_a_protocol_labelled_topic_zero() {
        let (_, data) = constant_pool_event();
        let topics = json!([
            {"type": "string", "value": "SoroswapPair"},
            {"type": "sym", "value": "add_pool"}
        ]);
        assert_eq!(parse_add_pool(&topics, &data), None);
    }

    #[test]
    fn rejects_a_registration_with_no_legs() {
        let (_, data) = constant_pool_event();
        let topics = json!([
            {"type": "sym", "value": "add_pool"},
            {"type": "vec", "value": []}
        ]);
        assert_eq!(parse_add_pool(&topics, &data), None);
    }
}
