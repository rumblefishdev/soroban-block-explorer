//! Which tables a targeted (`--only`) re-parse may write.

/// The set of tables a targeted (`--only`) re-parse persists.
///
/// Only tables that are **additive** may be targeted — a new derived table
/// whose rows are deterministic from the XDR, carry no Tier-1 MIN-semantics
/// column, and can be rolled back with `DROP TABLE`. The list is closed on
/// purpose: adding a name here is a statement that the table meets those
/// conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedTables(Vec<&'static str>);

impl TargetedTables {
    pub const TARGETABLE: &'static [&'static str] = &[
        "pool_operation_amounts",
        "asset_transfers",
        "transaction_memos",
        // Task 0518 — the three pool tables, so a full-range targeted
        // re-parse carries the pool families' whole history (and the classic
        // `legs` migration) in the SAME descent instead of owing a second one.
        //
        // None carries a Tier-1 MIN-semantics column, which is the condition
        // that actually matters here: `pool_state_changes` is version-less
        // RMT keyed by the row's own ledger, and both others version on a
        // "when we last saw it" ledger where MAX is the correct answer — so a
        // re-parsed historical row LOSES to a newer live one, as it should.
        //
        // `liquidity_pools` is the one that is NOT droppable (it holds the
        // classic pools too), so its rollback is a 3 MiB table copy rather
        // than a DROP. It earns the seat: the re-parse puts its soroban rows
        // through the live two-stage registration gate, which the in-DB
        // registry generators structurally cannot do (instance storage is not
        // in `soroban_events`, so they need a hand-rolled duplicate guard
        // instead of corroboration).
        //
        // MEASURED tie-break caveat, why the run owes an `OPTIMIZE ... FINAL`
        // on `liquidity_pools`: a re-parsed row ties on version with the row
        // the original ingest wrote for the same last-change ledger. On a tie
        // a merge keeps the LAST INSERTED row — the backfill's, correctly —
        // but until that merge runs both rows are live, and a read's
        // `argMax(..., last_updated_ledger)` picks arbitrarily between them.
        "pool_state_changes",
        "pool_instance_state",
        "liquidity_pools",
    ];
    /// Parse a comma-separated list; rejects unknown or duplicate names.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut out: Vec<&'static str> = Vec::new();
        for raw in spec.split(',') {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            let Some(known) = Self::TARGETABLE.iter().copied().find(|t| *t == name) else {
                return Err(format!(
                    "`{name}` is not a targetable table (targetable: {})",
                    Self::TARGETABLE.join(", ")
                ));
            };
            if out.contains(&known) {
                return Err(format!("`{name}` listed twice"));
            }
            out.push(known);
        }
        if out.is_empty() {
            return Err("no table named".into());
        }
        Ok(Self(out))
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.0.iter().copied()
    }

    pub fn contains(&self, table: &str) -> bool {
        self.0.contains(&table)
    }
}
