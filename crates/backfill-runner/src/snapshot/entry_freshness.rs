//! Whether a network entry adds anything to what we hold — the rule the
//! classic-pool pass applies ([`super::pools`]). Account entry state used it
//! too until task 0629, which writes every live account instead.

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
