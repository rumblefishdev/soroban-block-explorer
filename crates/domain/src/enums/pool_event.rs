//! What an operation or a pool event did to a liquidity pool: trade,
//! deposit or withdrawal.

use serde::{Deserialize, Serialize};

/// What an operation did to the pool, named by the SIGN PAIR of its two legs
/// and nothing else — `pool_operation_amounts.amount` is signed from the pool's
/// perspective, so `+/+` is a deposit, `-/-` a withdrawal and `+/-` a trade.
/// There is no operation-type column to read and no join to `operations`.
///
/// Classified in SQL rather than here, because the same expression is the
/// `filter[event]` predicate: two classifiers would eventually disagree, and
/// the one the user sees must be the one the filter used. This deliberately
/// reverses the client-side policy the retired `/transactions` shape carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum PoolEvent {
    Trade,
    Deposit,
    Withdrawal,
}

impl PoolEvent {
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
        match value {
            "trade" => Some(Self::Trade),
            "deposit" => Some(Self::Deposit),
            "withdrawal" => Some(Self::Withdrawal),
            _ => None,
        }
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

#[cfg(test)]
mod tests;
