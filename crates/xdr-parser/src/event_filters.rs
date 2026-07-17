//! SEP-41 / CAP-67 token-event decoding for Soroban contract events.
//!
//! [`parse_token_event`] classifies a contract event's topics as a fungible
//! token movement (transfer / mint / burn / clawback) and extracts the account
//! operands + the moved asset. Centralising the shape rules here keeps ingest
//! (`db_clickhouse::persist::stage`) and the `soroban-token-flow` backfill on a
//! single decode.

use serde_json::Value;

/// The SEP-41 / CAP-67 token-event verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEventKind {
    Transfer,
    Mint,
    Burn,
    Clawback,
}

/// The asset a token event moved, as identified by the event itself.
///
/// CAP-67 "unified" SAC events carry the classic asset as a trailing SEP-11
/// string topic (`"native"` or `"CODE:ISSUER"`). Bespoke non-SAC tokens omit
/// it — their asset identity is the emitting contract, surfaced here as
/// `Contract` so the caller (which holds `contract_id`) resolves the surrogate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventAsset {
    Native,
    Credit { code: String, issuer: String },
    Contract,
}

/// A decoded SEP-41 / CAP-67 token event (transfer / mint / burn / clawback).
///
/// `from` is `None` for mint; `to` is `None` for burn and clawback. No amount:
/// the presence indexes never store it, and the tx-detail page decodes amounts
/// from archive XDR at read time (E3, ADR 0029) — so it is not needed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenEvent {
    pub kind: TokenEventKind,
    pub from: Option<String>,
    pub to: Option<String>,
    pub asset: EventAsset,
}

/// Decode any SEP-41 / CAP-67 token event from its topics. Returns `None` when
/// topics do not match a known token-event shape.
///
/// Shapes (verified against prod, task 0383):
/// - transfer `[sym, addr(from), addr(to), string(asset)?]`
/// - mint     `[sym, addr(to), string(asset)?]`
/// - burn     `[sym, addr(from), string(asset)?]`
/// - clawback `[sym, addr(from), string(asset)?]`
///
/// The trailing SEP-11 asset string is present on SAC events and absent on
/// bespoke tokens (→ `EventAsset::Contract`).
pub fn parse_token_event(topics: &Value) -> Option<TokenEvent> {
    let arr = topics.as_array()?;
    let verb = arr.first()?;
    if verb.get("type").and_then(Value::as_str)? != "sym" {
        return None;
    }
    let sym = verb.get("value").and_then(Value::as_str)?;

    // (kind, from, to, asset_idx) — asset string, if any, sits after the
    // address operand(s).
    let (kind, from, to, asset_idx) = if sym.eq_ignore_ascii_case("transfer") {
        (
            TokenEventKind::Transfer,
            Some(address_topic(arr.get(1)?)?),
            Some(address_topic(arr.get(2)?)?),
            3,
        )
    } else if sym.eq_ignore_ascii_case("mint") {
        (
            TokenEventKind::Mint,
            None,
            Some(address_topic(arr.get(1)?)?),
            2,
        )
    } else if sym.eq_ignore_ascii_case("burn") {
        (
            TokenEventKind::Burn,
            Some(address_topic(arr.get(1)?)?),
            None,
            2,
        )
    } else if sym.eq_ignore_ascii_case("clawback") {
        (
            TokenEventKind::Clawback,
            Some(address_topic(arr.get(1)?)?),
            None,
            2,
        )
    } else {
        return None;
    };

    Some(TokenEvent {
        kind,
        from,
        to,
        asset: event_asset(arr.get(asset_idx)),
    })
}

/// Decode the moved **amount** (raw, unscaled `i128`) from a token event's
/// `data` payload (task 0393). Returns `None` when `data` is not an
/// amount-bearing shape.
///
/// Two shapes occur on mainnet (both proven in `nft.rs` / `nft_reparse.rs`):
/// - **bare scalar** `{"type":"i128"|"u128","value":"<decimal>"}` — the classic
///   SEP-41 transfer/mint/burn amount rides directly in `data`.
/// - **muxed map** `{"type":"map","value":[…]}` carrying an `amount` key
///   (CAP-67 `map{amount, to_muxed_id}`) — the amount is that entry's scalar.
///
/// The value is a decimal string (`i128::to_string()` from `scval_to_typed_json`);
/// a `u128` amount above `i128::MAX` cannot be stored as `Int128` and yields
/// `None` rather than a wrong value.
pub fn token_event_amount(data: &Value) -> Option<i128> {
    // Muxed map: the amount is the "amount" entry's scalar. Otherwise `data` is
    // the amount scalar itself.
    if data.get("type").and_then(Value::as_str) == Some("map") {
        return amount_scalar(map_get(data, "amount")?);
    }
    amount_scalar(data)
}

/// Read an `i128`/`u128` scalar's decimal-string value as `i128`. A `u128`
/// above `i128::MAX` fails to parse → `None` (unstorable, never a wrong value).
fn amount_scalar(v: &Value) -> Option<i128> {
    match v.get("type").and_then(Value::as_str)? {
        "i128" | "u128" => v.get("value").and_then(Value::as_str)?.parse::<i128>().ok(),
        _ => None,
    }
}

/// Look up `key` in a `{"type":"map","value":[{"key":sym,"value":…}]}` payload.
// ponytail: mirrors nft.rs::map_get; kept local to avoid coupling event decode
// to NFT internals — swap to a shared helper only if a third caller appears.
fn map_get<'a>(data: &'a Value, key: &str) -> Option<&'a Value> {
    data.get("value")?.as_array()?.iter().find_map(|entry| {
        let k = entry.get("key")?;
        if k.get("type").and_then(Value::as_str) == Some("sym")
            && k.get("value").and_then(Value::as_str) == Some(key)
        {
            entry.get("value")
        } else {
            None
        }
    })
}

/// Resolve the asset from a trailing SEP-11 string topic. Absent, empty, or
/// malformed → `Contract` (bespoke token; identity is the emitting contract).
fn event_asset(topic: Option<&Value>) -> EventAsset {
    let Some(s) = topic.and_then(string_topic) else {
        return EventAsset::Contract;
    };
    if s == "native" {
        return EventAsset::Native;
    }
    match s.split_once(':') {
        Some((code, issuer)) if !code.is_empty() && !issuer.is_empty() => EventAsset::Credit {
            code: code.to_string(),
            issuer: issuer.to_string(),
        },
        _ => EventAsset::Contract,
    }
}

fn string_topic(topic: &Value) -> Option<String> {
    if topic.get("type").and_then(Value::as_str)? != "string" {
        return None;
    }
    topic
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn address_topic(topic: &Value) -> Option<String> {
    if topic.get("type").and_then(Value::as_str)? != "address" {
        return None;
    }
    let s = topic.get("value").and_then(Value::as_str)?;
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sym(value: &str) -> Value {
        json!({ "type": "sym", "value": value })
    }

    fn addr(value: &str) -> Value {
        json!({ "type": "address", "value": value })
    }

    // ---- parse_token_event (0383) ----------------------------------------

    fn string_topic(value: &str) -> Value {
        json!({ "type": "string", "value": value })
    }

    const ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";

    // ---- token_event_amount (0393) ---------------------------------------

    fn i128val(v: &str) -> Value {
        json!({ "type": "i128", "value": v })
    }
    fn u128val(v: &str) -> Value {
        json!({ "type": "u128", "value": v })
    }
    /// A `{"type":"map"}` payload from `(sym key, value)` entries.
    fn mapval(entries: &[(&str, Value)]) -> Value {
        json!({
            "type": "map",
            "value": entries.iter().map(|(k, v)| json!({
                "key": { "type": "sym", "value": k },
                "value": v,
            })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn amount_bare_i128() {
        assert_eq!(token_event_amount(&i128val("150")), Some(150));
    }

    #[test]
    fn amount_bare_u128() {
        assert_eq!(token_event_amount(&u128val("150")), Some(150));
    }

    #[test]
    fn amount_muxed_map_reads_amount_key() {
        // CAP-67 muxed transfer: map{ amount, to_muxed_id }.
        let data = mapval(&[
            ("amount", i128val("4200")),
            ("to_muxed_id", json!({ "type": "u64", "value": "7" })),
        ]);
        assert_eq!(token_event_amount(&data), Some(4200));
    }

    #[test]
    fn amount_map_without_amount_key_is_none() {
        let data = mapval(&[("token_id", i128val("93"))]);
        assert_eq!(token_event_amount(&data), None);
    }

    #[test]
    fn amount_non_amount_scalar_shapes_are_none() {
        assert_eq!(token_event_amount(&json!({ "type": "void" })), None);
        assert_eq!(
            token_event_amount(&json!({ "type": "vec", "value": [] })),
            None
        );
        assert_eq!(
            token_event_amount(&json!({ "type": "address", "value": "GBFOO" })),
            None
        );
    }

    #[test]
    fn amount_unparseable_value_is_none() {
        assert_eq!(token_event_amount(&i128val("not-a-number")), None);
        assert_eq!(token_event_amount(&json!({ "type": "i128" })), None);
    }

    #[test]
    fn amount_u128_above_i128_max_is_none() {
        // 2^127 — a valid u128, unrepresentable as i128 → None, not a wrong value.
        assert_eq!(
            token_event_amount(&u128val("170141183460469231731687303715884105728")),
            None
        );
    }

    #[test]
    fn token_event_transfer_sac_credit() {
        let ev = parse_token_event(&json!([
            sym("transfer"),
            addr("GBFROM"),
            addr("GBTO"),
            string_topic(&format!("USDC:{ISSUER}"))
        ]))
        .unwrap();
        assert_eq!(ev.kind, TokenEventKind::Transfer);
        assert_eq!(ev.from.as_deref(), Some("GBFROM"));
        assert_eq!(ev.to.as_deref(), Some("GBTO"));
        assert_eq!(
            ev.asset,
            EventAsset::Credit {
                code: "USDC".to_string(),
                issuer: ISSUER.to_string()
            }
        );
    }

    #[test]
    fn token_event_transfer_native() {
        let ev = parse_token_event(&json!([
            sym("transfer"),
            addr("GBFROM"),
            addr("GBTO"),
            string_topic("native")
        ]))
        .unwrap();
        assert_eq!(ev.asset, EventAsset::Native);
    }

    #[test]
    fn token_event_transfer_bespoke_no_asset_string_is_contract() {
        let ev =
            parse_token_event(&json!([sym("transfer"), addr("GBFROM"), addr("GBTO")])).unwrap();
        assert_eq!(ev.kind, TokenEventKind::Transfer);
        assert_eq!(ev.from.as_deref(), Some("GBFROM"));
        assert_eq!(ev.to.as_deref(), Some("GBTO"));
        assert_eq!(ev.asset, EventAsset::Contract);
    }

    #[test]
    fn token_event_mint_has_to_no_from() {
        let ev = parse_token_event(&json!([
            sym("mint"),
            addr("GBTO"),
            string_topic(&format!("BISMUTH:{ISSUER}"))
        ]))
        .unwrap();
        assert_eq!(ev.kind, TokenEventKind::Mint);
        assert_eq!(ev.from, None);
        assert_eq!(ev.to.as_deref(), Some("GBTO"));
    }

    #[test]
    fn token_event_burn_has_from_no_to() {
        let ev = parse_token_event(&json!([
            sym("burn"),
            addr("GBFROM"),
            string_topic(&format!("GOLD:{ISSUER}"))
        ]))
        .unwrap();
        assert_eq!(ev.kind, TokenEventKind::Burn);
        assert_eq!(ev.from.as_deref(), Some("GBFROM"));
        assert_eq!(ev.to, None);
    }

    #[test]
    fn token_event_clawback_has_from_no_to() {
        let ev = parse_token_event(&json!([
            sym("clawback"),
            addr("GBFROM"),
            string_topic(&format!("VELO:{ISSUER}"))
        ]))
        .unwrap();
        assert_eq!(ev.kind, TokenEventKind::Clawback);
        assert_eq!(ev.from.as_deref(), Some("GBFROM"));
        assert_eq!(ev.to, None);
    }

    #[test]
    fn token_event_mint_bespoke_two_topics_is_contract() {
        let ev = parse_token_event(&json!([sym("mint"), addr("GBTO")])).unwrap();
        assert_eq!(ev.kind, TokenEventKind::Mint);
        assert_eq!(ev.to.as_deref(), Some("GBTO"));
        assert_eq!(ev.asset, EventAsset::Contract);
    }

    #[test]
    fn token_event_is_case_insensitive_on_verb() {
        let ev =
            parse_token_event(&json!([sym("MINT"), addr("GBTO"), string_topic("native")])).unwrap();
        assert_eq!(ev.kind, TokenEventKind::Mint);
    }

    #[test]
    fn token_event_rejects_unknown_symbol() {
        assert!(parse_token_event(&json!([sym("swap"), addr("GBA"), addr("GBB")])).is_none());
    }

    #[test]
    fn token_event_rejects_mint_without_address() {
        assert!(parse_token_event(&json!([sym("mint"), sym("not_an_address")])).is_none());
    }
}
