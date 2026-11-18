//! Contract values leave the API with every integer as decimal text.
//!
//! A browser's `JSON.parse` keeps an integer exact only up to 2^53 − 1, so a
//! u64 such as a muxed account id 6952519163801588163 reaches the page as
//! …588000. The parser and the stored rows keep exact JSON numbers (Rust
//! reads them exactly); only the response the API sends changes. One rule for
//! every integer type, small ones too, so a client never checks which form a
//! value came in (lore 0363, decision 13).
//!
//! u256 and i256 are the exception in input only: the parser writes their
//! four 64-bit parts as 64 hex characters, which a reader takes for a decimal
//! number. They leave as the signed decimal the official stellar-xdr JSON
//! (SEP-51) writes.
//!
//! Call it last, on the value about to be sent: Rust code that reads these
//! numbers runs before it.

use serde_json::Value;
use stellar_xdr::{Int256Parts, UInt256Parts};

/// Rewrites, in place, every typed contract value (`{"type", "value"}`)
/// inside `value`, at any depth: an integer becomes decimal text, and a
/// u256/i256 hex value becomes its decimal. Anything else is left as it is.
pub fn scval_integers_as_text(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(scval_integers_as_text),
        Value::Object(fields) => {
            let kind = fields
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let (Some(kind), Some(inner)) = (kind, fields.get_mut("value"))
                && let Some(text) = integer_text(&kind, inner)
            {
                *inner = Value::String(text);
            }
            fields.values_mut().for_each(scval_integers_as_text);
        }
        _ => {}
    }
}

/// The decimal text of an integer contract value, or `None` when `kind` is
/// not an integer type the parser writes as a number or as hex.
fn integer_text(kind: &str, inner: &Value) -> Option<String> {
    match (kind, inner) {
        (
            "u32" | "i32" | "u64" | "i64" | "timepoint" | "duration" | "ledger_key_nonce",
            Value::Number(n),
        ) => Some(n.to_string()),
        ("u256", Value::String(hex)) => {
            let [hi_hi, hi_lo, lo_hi, lo_lo] = hex_parts(hex)?;
            Some(
                UInt256Parts {
                    hi_hi,
                    hi_lo,
                    lo_hi,
                    lo_lo,
                }
                .to_string(),
            )
        }
        ("i256", Value::String(hex)) => {
            let [hi_hi, hi_lo, lo_hi, lo_lo] = hex_parts(hex)?;
            // The top part carries the sign: the same 64 bits, read signed.
            let hi_hi = hi_hi as i64;
            Some(
                Int256Parts {
                    hi_hi,
                    hi_lo,
                    lo_hi,
                    lo_lo,
                }
                .to_string(),
            )
        }
        _ => None,
    }
}

/// The four big-endian 64-bit parts of a 64-character hex value, or `None`
/// for any other text (left as it arrived).
fn hex_parts(hex: &str) -> Option<[u64; 4]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let part = |i: usize| u64::from_str_radix(&hex[i * 16..(i + 1) * 16], 16).ok();
    Some([part(0)?, part(1)?, part(2)?, part(3)?])
}

#[cfg(test)]
mod tests;
