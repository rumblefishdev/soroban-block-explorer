//! Schema conventions `init.sql` must keep. Pure text checks, no ClickHouse.
//!
//! A transaction is located by its position `(ledger_sequence,
//! application_order)`, never by the `transaction_id` surrogate (ADR 0059,
//! programme 0538): the surrogate is a hash, compresses at ratio 1.0 and
//! costs 8 B/row where the position costs ~0.1–1.3.

use db_clickhouse::INIT_SQL;

/// Tables that still carry the surrogate. The 0538 programme removes one
/// entry per table it migrates; nothing is ever added here.
const TRANSACTION_ID_ALLOWLIST: &[&str] = &[
    "soroban_invocations_appearances",
    "nft_ownership",
    "nft_ownership_pending",
];

/// `(table, column names)` for every `CREATE TABLE` in `init.sql`.
fn tables() -> Vec<(String, Vec<String>)> {
    let stripped: String = INIT_SQL
        .lines()
        .map(|line| line.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    stripped
        .split(';')
        .filter_map(|stmt| {
            let rest = stmt.trim().strip_prefix("CREATE TABLE IF NOT EXISTS ")?;
            let (name, body) = rest.split_once('(')?;
            let columns = body
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .map(|c| c.trim_end_matches(',').to_string())
                .collect();
            Some((name.trim().to_string(), columns))
        })
        .collect()
}

#[test]
fn no_new_table_locates_a_transaction_by_its_surrogate() {
    let tables = tables();
    assert!(
        tables.len() > 20,
        "parser found only {} tables",
        tables.len()
    );

    let carrying: Vec<&str> = tables
        .iter()
        .filter(|(_, cols)| cols.iter().any(|c| c == "transaction_id" || c == "tx_id"))
        .map(|(name, _)| name.as_str())
        .collect();

    for table in &carrying {
        assert!(
            TRANSACTION_ID_ALLOWLIST.contains(table),
            "`{table}` has a `transaction_id` column. Locate the transaction by \
             `(ledger_sequence, application_order)` instead (ADR 0059, task 0538)."
        );
    }
    for table in TRANSACTION_ID_ALLOWLIST {
        assert!(
            carrying.contains(table),
            "`{table}` no longer has `transaction_id` — remove it from \
             TRANSACTION_ID_ALLOWLIST."
        );
    }
}
