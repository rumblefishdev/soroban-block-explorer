//! Whether a network entry adds anything to what we hold — the rule both
//! checkpoint passes apply: classic pools ([`super::pools`]) and account entry
//! state ([`super::entry_state`]).

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Need {
    Missing,
    Stale,
    Current,
}

/// Whether the network's entry, last modified at `entry_ledger`, adds anything
/// to our newest snapshot of the same pool.
pub(crate) fn need(our_newest_ledger: Option<i64>, entry_ledger: u32) -> Need {
    match our_newest_ledger {
        None => Need::Missing,
        Some(ours) if ours < i64::from(entry_ledger) => Need::Stale,
        Some(_) => Need::Current,
    }
}

#[cfg(test)]
mod tests;
