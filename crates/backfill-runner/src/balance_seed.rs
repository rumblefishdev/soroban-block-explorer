//! Task 0331 step 7 — one-shot RPC-snapshot seed of per-holder Soroban token
//! balances for bespoke type-3 tokens.
//!
//! ## Why this exists
//!
//! The live parser writes the unified `balances` table only when it observes a
//! `ContractData` `Balance(Address)` change. Holders that have not moved their
//! tokens since the parser was deployed are never seen, so `balances`
//! under-counts both supply (`sum(amount)`) and holders (`countIf(amount > 0)`).
//! This pass captures the CURRENT on-chain state once, directly from mainnet RPC,
//! making the table complete without reprocessing all historical ledgers.
//!
//! ## Mechanism
//!
//! 1. Per type-3 token, enumerate holder candidates = every `G…`/`C…` StrKey in
//!    the token's `soroban_events` topics + data (the event SET — the value comes
//!    from ledger STATE via RPC, never an event-fold; see task README DECISION
//!    2026-06-29).
//! 2. Build `Balance(Address)` persistent ledger keys and fetch them via
//!    `getLedgerEntries` (batched by the shared [`RpcClient`]); decode the
//!    bare-`i128` value with the same contract the live parser uses.
//! 3. Write `balances` rows (`ReplacingMergeTree`, keyed `(holder_id, asset_id)`)
//!    with version = the entry's last-modified ledger, so the live writer cleanly
//!    supersedes the seed once ingest catches up.
//!
//! Reads CURRENT state, so it is **freshness-immune to the indexer lag** — the
//! seed is correct at run time no matter how far behind live ingest is.
//! Idempotent: a re-run re-reads + re-upserts. `--dry-run` reports counts without
//! writing. CH-only — the unified `balances` model lives in ClickHouse, so a
//! Postgres target no-ops.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use db_clickhouse::persist::stage::build_balance_rows;
use serde::Deserialize;
use tracing::info;
use xdr_parser::ExtractedSorobanBalance;

use crate::error::BackfillError;
use crate::util::insert_rows;
use crate::rpc_snapshot::{RpcClient, balance_ledger_key, decode_balance_entry};
use crate::sink::Sink;

/// Per-run funnel LEVELS — read top-to-bottom they show where holders drop off
/// (enumerated → keyed → found on-chain → decoded), so an operator can tell an
/// empty result apart from an RPC / shape problem. Only the distinct levels are
/// stored; the drops between them are plain subtraction (e.g. non-standard
/// shapes skipped = `entries_returned - balances_decoded`), not redundant fields.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BalanceSeedStats {
    /// type-3 tokens with at least one holder candidate in their event stream.
    pub tokens: u64,
    /// `(token, holder)` candidate pairs enumerated from the event scan.
    pub holders_enumerated: u64,
    /// Candidate keys built + requested from RPC (malformed `StrKey`s dropped).
    pub keys_requested: u64,
    /// Ledger entries RPC returned (a key that exists on-chain); the shortfall
    /// vs `keys_requested` is holders with no live entry (false-positive scrape
    /// or a since-zeroed balance).
    pub entries_returned: u64,
    /// Standard bare-`i128` balances decoded + staged; the shortfall vs
    /// `entries_returned` is non-standard shapes (e.g. a SAC struct) skipped.
    pub balances_decoded: u64,
    pub dry_run: bool,
}

impl BalanceSeedStats {
    /// Snapshot the funnel levels from the raw stage sizes.
    fn from_funnel(
        candidates: &[SeedCandidate],
        keys_requested: u64,
        entries_returned: u64,
        balances_decoded: u64,
        dry_run: bool,
    ) -> Self {
        Self {
            tokens: candidates.len() as u64,
            holders_enumerated: candidates.iter().map(|c| c.holders.len() as u64).sum(),
            keys_requested,
            entries_returned,
            balances_decoded,
            dry_run,
        }
    }
}

/// One row of the candidate query: a token's C-StrKey + every holder StrKey
/// seen in its event stream.
#[derive(Row, Deserialize)]
struct SeedCandidate {
    token_strkey: String,
    holders: Vec<String>,
}

pub async fn execute(
    sink: &Sink,
    rpc_url: Option<&str>,
    dry_run: bool,
) -> Result<BalanceSeedStats, BackfillError> {
    // ClickHouse-only — the unified balances model is CH; PG retired.
    let Sink::Clickhouse(client) = sink else {
        return Err(BackfillError::Incomplete(
            "balance-seed is ClickHouse-only (unified balances model; PG retired)".to_string(),
        ));
    };

    let rpc_url = rpc_url.ok_or_else(|| {
        BackfillError::Incomplete(
            "balance_seed requires --soroban-rpc-url (or SOROBAN_RPC_URL)".to_string(),
        )
    })?;

    let candidates = read_seed_candidates(client).await?;
    if candidates.is_empty() {
        info!("balance_seed: no type-3 token holder candidates found — nothing to do");
        return Ok(BalanceSeedStats::from_funnel(&candidates, 0, 0, 0, dry_run));
    }

    // Build every (token, holder) balance key. `decode_balance_entry` recovers
    // the token + holder from the returned entry itself, so request→response
    // order need not be tracked. `balance_ledger_key` returns `None` on a
    // malformed holder (the drop shows as `holders_enumerated - keys_requested`).
    let mut keys = Vec::new();
    for cand in &candidates {
        for holder in &cand.holders {
            if let Some(key) = balance_ledger_key(&cand.token_strkey, holder) {
                keys.push(key);
            }
        }
    }
    let keys_requested = keys.len() as u64;

    let rpc = RpcClient::new(rpc_url)?;
    let records = rpc.get_ledger_entries(&keys).await?;
    let entries_returned = records.len() as u64;

    let mut balances: Vec<ExtractedSorobanBalance> = Vec::with_capacity(records.len());
    for rec in records {
        if let Some((contract_id, holder, balance)) = decode_balance_entry(&rec.data) {
            balances.push(ExtractedSorobanBalance {
                contract_id,
                holder,
                balance,
                ledger: rec.last_modified_ledger,
            });
        }
    }

    let stats = BalanceSeedStats::from_funnel(
        &candidates,
        keys_requested,
        entries_returned,
        balances.len() as u64,
        dry_run,
    );
    let balance_rows = build_balance_rows(&balances);

    if dry_run {
        info!(
            tokens = stats.tokens,
            holders_enumerated = stats.holders_enumerated,
            keys_requested = stats.keys_requested,
            entries_returned = stats.entries_returned,
            balances_decoded = stats.balances_decoded,
            "balance_seed: dry-run, no rows written"
        );
        return Ok(stats);
    }

    insert_rows(client, "balances", &balance_rows).await?;
    info!(
        balances = balance_rows.len(),
        entries_returned = stats.entries_returned,
        balances_decoded = stats.balances_decoded,
        "balance_seed: wrote seed rows (RMT supersede; live ingest takes over on catch-up)"
    );
    Ok(stats)
}

/// Candidate query: for every type-3 token, its C-StrKey + the distinct set of
/// `G…`/`C…` holder StrKeys appearing in its event stream. StrKeys are matched
/// with a base32 regex (`[GC]` + 55 base32 chars) over the tagged-JSON
/// `topics_xdr`/`data_xdr`; over-matching is harmless (a non-holder key just
/// returns an absent entry, dropped). Scoped to the type-3 set so the scan reads
/// ~the bespoke-token event subset, not the full firehose.
async fn read_seed_candidates(
    client: &ClickhouseClient,
) -> Result<Vec<SeedCandidate>, clickhouse::error::Error> {
    client
        .query(
            "SELECT sc.contract_id AS token_strkey, \
                    arrayDistinct(arrayConcat( \
                        groupArrayArray(extractAll(e.topics_xdr, '([GC][A-Z2-7]{55})')), \
                        groupArrayArray(extractAll(e.data_xdr, '([GC][A-Z2-7]{55})')) \
                    )) AS holders \
             FROM soroban_events e \
             INNER JOIN soroban_contracts sc FINAL ON sc.id = e.contract_id \
             WHERE e.contract_id IN ( \
                 SELECT contract_id FROM assets WHERE asset_type = 3 AND contract_id != 0 \
             ) \
             GROUP BY sc.contract_id",
        )
        .fetch_all::<SeedCandidate>()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(token: &str, holders: &[&str]) -> SeedCandidate {
        SeedCandidate {
            token_strkey: token.into(),
            holders: holders.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn from_funnel_snapshots_levels_and_derives_drops() {
        let candidates = vec![cand("CTOKENA", &["GA", "GB", "GC"]), cand("CTOKENB", &["GD"])];
        // 4 enumerated → 3 keyed (1 malformed dropped) → 2 on-chain → 2 decoded.
        let s = BalanceSeedStats::from_funnel(&candidates, 3, 2, 2, false);
        assert_eq!(s.tokens, 2);
        assert_eq!(s.holders_enumerated, 4);
        assert_eq!(s.keys_requested, 3);
        assert_eq!(s.entries_returned, 2);
        assert_eq!(s.balances_decoded, 2);
        assert!(!s.dry_run);
        // The drops are plain subtraction of stored levels — not redundant fields.
        assert_eq!(s.holders_enumerated - s.keys_requested, 1, "malformed dropped");
        assert_eq!(s.keys_requested - s.entries_returned, 1, "absent on-chain");
        assert_eq!(s.entries_returned - s.balances_decoded, 0, "non-standard skipped");
    }

    #[test]
    fn from_funnel_empty_is_all_zero() {
        let s = BalanceSeedStats::from_funnel(&[], 0, 0, 0, true);
        assert_eq!(
            s,
            BalanceSeedStats {
                dry_run: true,
                ..Default::default()
            }
        );
    }

    /// Integration: the candidate query's ClickHouse regex must scrape EVERY
    /// `G…`/`C…` StrKey from a type-3 token's event `topics_xdr` + `data_xdr`,
    /// distinct. Gated on `CLICKHOUSE_URL` (skipped when unset, like the
    /// db-clickhouse smoke test) — inserts a coherent contract+asset+event,
    /// runs the real query, asserts, then cleans up.
    #[tokio::test]
    async fn read_seed_candidates_scrapes_holders_from_events() {
        if std::env::var("CLICKHOUSE_URL").is_err() {
            eprintln!("CLICKHOUSE_URL not set — skipping balance-seed candidate integration test");
            return;
        }
        let cfg = db_clickhouse::Config::from_env();
        let client = db_clickhouse::client(&cfg);
        db_clickhouse::apply_init_sql(&client)
            .await
            .expect("init sql");

        const SID: i64 = 7_000_000_000_000_000_123;
        let token = "CCSNFZ5RA2EHTSMK2A5ZDXRCAQBYBVFAPJFNWP5BJECLIL4J5UBLLUQG";
        let g1 = "GAWOKP6NJAWNRPQDE4O3NZYDFJHEMLUIP36AC74HNBHLTA3GURYB4PYJ";
        let g2 = "GB7ZJDRFBU5JALPZRJKA3CVRGNCJMR2ZDCWZNPMNCMU2WAWDMGPYPM4F";
        let c1 = "CAVCJKFX33OI7CPKFNJXOHLXVZHFAWWAUUKUKZJG62YPHSDZ7CXWRH3J";

        let scrub = |c: &ClickhouseClient| {
            let c = c.clone();
            async move {
                for q in [
                    format!("ALTER TABLE soroban_events DELETE WHERE contract_id = {SID}"),
                    format!("ALTER TABLE soroban_contracts DELETE WHERE id = {SID}"),
                    format!("ALTER TABLE assets DELETE WHERE contract_id = {SID}"),
                ] {
                    let _ = c.query(&q).execute().await;
                }
            }
        };
        scrub(&client).await;

        client
            .query(&format!(
                "INSERT INTO soroban_contracts (id, contract_id) VALUES ({SID}, '{token}')"
            ))
            .execute()
            .await
            .expect("insert contract");
        client
            .query(&format!(
                "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id) \
                 VALUES (3, '', 0, {SID})"
            ))
            .execute()
            .await
            .expect("insert asset");
        let topics = format!(
            "[{{\"type\":\"address\",\"value\":\"{g1}\"}},{{\"type\":\"address\",\"value\":\"{g2}\"}}]"
        );
        let data = format!("{{\"type\":\"address\",\"value\":\"{c1}\"}}");
        client
            .query(&format!(
                "INSERT INTO soroban_events (contract_id, topics_xdr, data_xdr) \
                 VALUES ({SID}, '{topics}', '{data}')"
            ))
            .execute()
            .await
            .expect("insert event");

        let mine: Vec<_> = read_seed_candidates(&client)
            .await
            .expect("query")
            .into_iter()
            .filter(|c| c.token_strkey == token)
            .collect();
        assert_eq!(mine.len(), 1, "one candidate row for the token");
        let mut holders = mine[0].holders.clone();
        holders.sort();
        let mut want = vec![g1.to_string(), g2.to_string(), c1.to_string()];
        want.sort();
        assert_eq!(
            holders, want,
            "regex scrapes both G topics + the C data address, distinct"
        );

        scrub(&client).await;
    }
}
