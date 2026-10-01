//! What an operation or a pool event did to a liquidity pool: trade,
//! deposit or withdrawal.

use serde::{Deserialize, Serialize};

use super::EnumDecodeError;

/// What an operation did to the pool. A classic operation is named by the SIGN
/// PAIR of its two legs and nothing else — `pool_operation_amounts.amount` is
/// signed from the pool's perspective, so `+/+` is a deposit, `-/-` a
/// withdrawal and `+/-` a trade.
/// There is no operation-type column to read and no join to `operations`.
/// A soroban pool's row is one event, named by the kind the pool declared,
/// stored in `pool_movements.event_kind` as this enum's discriminant — not
/// inferred from the signs: a trade may carry a zero leg (42 on production)
/// and a withdrawal may pay out nothing, which the signs alone would misread.
///
/// Classified on the server rather than in the page, and by the same function
/// the `filter[event]` predicate calls: two classifiers would eventually
/// disagree, and the one the user sees must be the one the filter used. This
/// deliberately reverses the client-side policy the retired `/transactions`
/// shape carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum PoolEvent {
    Trade = 0,
    Deposit = 1,
    Withdrawal = 2,
}

impl PoolEvent {
    pub const VARIANTS: &'static [Self] = &[Self::Trade, Self::Deposit, Self::Withdrawal];

    /// The whole classifier: the signs of an operation's leg amounts.
    ///
    /// Every amount is signed from the pool's perspective, so a leg that
    /// entered the pool is positive. Anything that is not "all in" or "all
    /// out" moved value across the pool in opposite directions, which is a
    /// trade — including the zero-amount edge a dust swap can produce, since
    /// it is still not a deposit and not a withdrawal.
    ///
    /// Callers must only reach here with EVERY leg present; a half-row has no
    /// event (see the API's `PoolActivityItem::event`).
    pub fn from_signs(amounts: &[i64]) -> Self {
        if amounts.iter().all(|&a| a > 0) {
            Self::Deposit
        } else if amounts.iter().all(|&a| a < 0) {
            Self::Withdrawal
        } else {
            Self::Trade
        }
    }

    /// Parse a `filter[event]` value.
    pub fn from_param(value: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|v| v.as_param() == value)
    }

    /// The accepted spelling, for the `allowed` list a rejection returns.
    /// `const` so that list can be built from these three arms instead of
    /// being retyped next to the handler and drifting from the parser.
    pub const fn as_param(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Deposit => "deposit",
            Self::Withdrawal => "withdrawal",
        }
    }
}

/// A stored `pool_movements.event_kind`.
impl TryFrom<u8> for PoolEvent {
    type Error = EnumDecodeError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Trade),
            1 => Ok(Self::Deposit),
            2 => Ok(Self::Withdrawal),
            _ => Err(EnumDecodeError::UnknownDiscriminant {
                enum_name: "PoolEvent",
                value: i16::from(v),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
