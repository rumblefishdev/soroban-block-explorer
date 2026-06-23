//! Task 0294 — one-shot batch relabel of un-deployed-SAC "orphan" rows in
//! `soroban_contracts`.
//!
//! ## Why this exists
//!
//! "Orphans" (`is_sac=false`, no deploy — `coalesce(deployed_at_ledger,0)=0` —
//! and NULL `wasm_hash`) that emit SAC events are actually **un-deployed SACs**
//! surfaced via direct SAC host-function invocation (Protocol 20+) and, post-
//! P23, CAP-67 unified asset events. The live parser fix
//! (`xdr_parser::derive_sac_overrides_from_events`) labels them going forward;
//! this pass fixes the EXISTING history so the `nft-reclassify` step (task 0303)
//! DROPS their false-positive `nfts_pending` rows (the i128 transfer amount
//! mis-read as a token_id) instead of churning.
//!
//! ## Scope (read-only prod measurement, 2026-06-23)
//!
//! Of 5,607 orphans by the predicate above, **5,558 emit a SAC-control event and
//! ALL 5,558 crypto-confirm** as un-deployed SACs — the gate rejected **zero** (no
//! false positives). Only 49 emit no SAC-control event; they are not provably SACs
//! and this pass intentionally leaves them untouched (a tiny separate residual).
//!
//! ## Mechanism
//!
//! 1. Pull one sample SAC-control event's topics per orphan from
//!    `soroban_events` (`soroban_events.contract_id` is the surrogate id =
//!    `soroban_contracts.id`).
//! 2. **Crypto-match gate in Rust** (ClickHouse can't SHA256/XDR) via the
//!    SHARED `xdr_parser::sac_override_from_event_topics` — byte-identical to the
//!    live path, so a bespoke WASM contract emitting a SAC-shaped event with an
//!    asset-string topic (`emitter != derive_sac(asset)`) is rejected.
//! 3. Re-INSERT a corrected `soroban_contracts` row (`is_sac=true`,
//!    `contract_type=Token`, `wasm_uploaded_at_ledger=0`). RMT
//!    (`ORDER BY (contract_id)`) collapses: the override wins over the
//!    `is_sac=false` skeleton (same `version=0` sentinel as the live SAC
//!    override at `stage.rs:556-597`), while a future real deploy
//!    (`version>0`) still wins — so this can never downgrade a real deploy.
//!
//! Idempotent (after the flip the row is no longer an orphan, so a re-run is a
//! no-op). `--dry-run` reports the crypto-confirmed count without writing.
//! CH-only — the Postgres target short-circuits.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::info;
use xdr_parser::{network_id, sac_override_from_event_topics};

use crate::ch_staging::drop_if_exists;
use crate::error::BackfillError;
use crate::sink::Sink;

#[derive(Debug, Default, Clone, Copy)]
pub struct SacOrphanRelabelStats {
    /// Orphans that emit at least one SAC-control event (the scan population).
    pub orphans_scanned: u64,
    /// Orphans whose emitter id == `derive_sac(topic asset)` (the C1 gate).
    pub crypto_confirmed: u64,
    /// Corrected `soroban_contracts` rows written (0 on dry-run).
    pub rows_inserted: u64,
    pub dry_run: bool,
}

/// `(orphan strkey, one sample event's topics JSON)`. `topics_xdr` is
/// `serde_json::to_string(topics)` despite the name (`stage.rs:866`).
#[derive(Row, Deserialize)]
struct OrphanEventRow {
    contract_id: String,
    topics_xdr: String,
}

/// `(surrogate id, strkey)` for every orphan — the light first pass that the
/// chunked event fetch maps over (no heavy `topics_xdr`, no join).
#[derive(Row, Deserialize)]
struct OrphanIdRow {
    id: i64,
    contract_id: String,
}

/// One sample event's topics keyed by the emitter's surrogate id.
#[derive(Row, Deserialize)]
struct EventTopicRow {
    id: i64,
    topics_xdr: String,
}

#[derive(Row, Serialize)]
struct ConfirmedKey {
    contract_id: String,
}

/// `is_sac=false`, no deploy (NULL or the `=0` sentinel), no WASM link.
const ORPHAN_PREDICATE: &str =
    "is_sac = false AND coalesce(deployed_at_ledger, 0) = 0 AND wasm_hash IS NULL";
const CONFIRMED_TABLE: &str = "sac_orphan_confirmed_0294";
/// Orphan ids per event-fetch query. A whole-population join over
/// `soroban_events` (~344M rows) materialising `topics_xdr` blows the server
/// memory limit; a small `IN(...)` per chunk bounds each scan.
const FETCH_CHUNK: usize = 500;

pub async fn execute(
    sink: &Sink,
    dry_run: bool,
    network_passphrase: &str,
) -> Result<SacOrphanRelabelStats, BackfillError> {
    let Sink::Clickhouse(client) = sink else {
        info!("sac_orphan_relabel: skipped (PG target — CH-only maintenance)");
        return Ok(SacOrphanRelabelStats::default());
    };

    let mut stats = SacOrphanRelabelStats {
        dry_run,
        ..Default::default()
    };

    // ---- Phase 1: one sample SAC-event's topics per orphan ----
    let rows = fetch_orphan_events(client).await?;
    stats.orphans_scanned = rows.len() as u64;

    // ---- Phase 2: crypto-match gate in Rust (shared with the live path) ----
    let net_id = network_id(network_passphrase);
    let confirmed = confirmed_orphan_strkeys(&rows, &net_id);
    stats.crypto_confirmed = confirmed.len() as u64;
    info!(
        scanned = stats.orphans_scanned,
        confirmed = stats.crypto_confirmed,
        "sac_orphan_relabel: crypto-confirmed un-deployed SACs"
    );

    if dry_run || confirmed.is_empty() {
        return Ok(stats);
    }

    // ---- Phase 3: stage confirmed strkeys → INSERT corrected RMT rows ----
    drop_if_exists(client, CONFIRMED_TABLE).await?;
    create_confirmed_table(client, CONFIRMED_TABLE).await?;
    insert_confirmed(client, CONFIRMED_TABLE, &confirmed).await?;
    stats.rows_inserted = insert_overrides(client, CONFIRMED_TABLE).await?;
    drop_if_exists(client, CONFIRMED_TABLE).await?;

    info!(
        inserted = stats.rows_inserted,
        dry_run, "sac_orphan_relabel: completed"
    );
    Ok(stats)
}

/// One sample SAC-control event's topics per orphan, fetched in two memory-safe
/// passes (a single join over the full `soroban_events` table OOMs the server):
///
/// 1. light: every orphan's `(surrogate id, strkey)` — no heavy column, no join.
/// 2. chunked: `LIMIT 1 BY` one event per orphan id over a small `IN(...)` window.
///
/// Orphans with no SAC-control event are simply absent from the result (they are
/// not provably SACs, so the crypto gate could never confirm them anyway).
async fn fetch_orphan_events(
    client: &ClickhouseClient,
) -> Result<Vec<OrphanEventRow>, BackfillError> {
    let orphans = client
        .query(&format!(
            "SELECT id, contract_id FROM soroban_contracts FINAL WHERE {ORPHAN_PREDICATE}"
        ))
        .fetch_all::<OrphanIdRow>()
        .await
        .map_err(BackfillError::Ch)?;
    let strkey_by_id: std::collections::HashMap<i64, String> = orphans
        .iter()
        .map(|o| (o.id, o.contract_id.clone()))
        .collect();

    let ids: Vec<i64> = orphans.iter().map(|o| o.id).collect();
    let mut out = Vec::new();
    for chunk in ids.chunks(FETCH_CHUNK) {
        let inlist = chunk
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT e.contract_id AS id, e.topics_xdr AS topics_xdr \
             FROM soroban_events AS e \
             WHERE e.contract_id IN ({inlist}) \
               AND e.signature IN ('transfer','mint','burn','clawback','set_authorized') \
             LIMIT 1 BY e.contract_id"
        );
        let events = client
            .query(&sql)
            .fetch_all::<EventTopicRow>()
            .await
            .map_err(BackfillError::Ch)?;
        for ev in events {
            if let Some(strkey) = strkey_by_id.get(&ev.id) {
                out.push(OrphanEventRow {
                    contract_id: strkey.clone(),
                    topics_xdr: ev.topics_xdr,
                });
            }
        }
    }
    Ok(out)
}

/// Pure crypto-match gate over the fetched orphan events. Reuses the SHARED
/// `sac_override_from_event_topics`, so the batch verdict is identical to the
/// live path. Returns the distinct orphan strkeys confirmed as un-deployed SACs
/// (`emitter == derive_sac(asset)`); malformed/non-SAC/mismatch rows are
/// skipped.
fn confirmed_orphan_strkeys(rows: &[OrphanEventRow], net_id: &[u8; 32]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let Ok(topics) = serde_json::from_str::<serde_json::Value>(&row.topics_xdr) else {
            continue;
        };
        if sac_override_from_event_topics(&row.contract_id, &topics, net_id).is_some()
            && seen.insert(row.contract_id.clone())
        {
            out.push(row.contract_id.clone());
        }
    }
    out
}

async fn create_confirmed_table(
    client: &ClickhouseClient,
    table: &str,
) -> Result<(), BackfillError> {
    client
        .query(&format!(
            "CREATE TABLE {table} (contract_id String) ENGINE = Memory"
        ))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

async fn insert_confirmed(
    client: &ClickhouseClient,
    table: &str,
    strkeys: &[String],
) -> Result<(), BackfillError> {
    let mut insert = client
        .insert::<ConfirmedKey>(table)
        .await
        .map_err(BackfillError::Ch)?;
    for s in strkeys {
        insert
            .write(&ConfirmedKey {
                contract_id: s.clone(),
            })
            .await
            .map_err(BackfillError::Ch)?;
    }
    insert.end().await.map_err(BackfillError::Ch)?;
    Ok(())
}

/// Re-INSERT the corrected SAC rows. The re-applied orphan predicate is a
/// belt-and-suspenders guard: only flip rows that are STILL orphans (a row that
/// gained a real deploy since the export is left untouched — and even if
/// inserted, the `version=0` override would lose to the real deploy under RMT).
/// Returns the number of rows the INSERT writes.
async fn insert_overrides(
    client: &ClickhouseClient,
    confirmed_table: &str,
) -> Result<u64, BackfillError> {
    let count_sql = format!(
        "SELECT count() FROM soroban_contracts AS sc FINAL \
         INNER JOIN {confirmed_table} AS t ON t.contract_id = sc.contract_id \
         WHERE {ORPHAN_PREDICATE}"
    );
    let n: u64 = client
        .query(&count_sql)
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;

    let insert_sql = format!(
        "INSERT INTO soroban_contracts \
           (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, \
            deployed_at_ledger, contract_type, is_sac, name) \
         SELECT sc.id, sc.contract_id, \
                CAST(NULL AS Nullable(FixedString(32))), 0, \
                CAST(NULL AS Nullable(Int64)), CAST(NULL AS Nullable(Int64)), \
                CAST(0 AS Nullable(Int16)), true, CAST(NULL AS Nullable(String)) \
         FROM soroban_contracts AS sc FINAL \
         INNER JOIN {confirmed_table} AS t ON t.contract_id = sc.contract_id \
         WHERE {ORPHAN_PREDICATE}"
    );
    client
        .query(&insert_sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    const USDC_TOPICS: &str = r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GFROM"},{"type":"address","value":"GTO"},{"type":"string","value":"USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"}]"#;
    // A valid C-StrKey that is NOT the USDC SAC (the native SAC) — stands in for
    // a bespoke WASM contract emitting a USDC-shaped event.
    const NOT_USDC_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

    #[test]
    fn confirmed_strkeys_applies_crypto_match_gate() {
        let net = network_id(xdr_parser::MAINNET_PASSPHRASE);
        let rows = vec![
            // emitter IS the USDC SAC → confirmed
            OrphanEventRow {
                contract_id: USDC_SAC.into(),
                topics_xdr: USDC_TOPICS.into(),
            },
            // emitter is NOT the USDC SAC → gate rejects
            OrphanEventRow {
                contract_id: NOT_USDC_SAC.into(),
                topics_xdr: USDC_TOPICS.into(),
            },
            // malformed topics JSON → skipped
            OrphanEventRow {
                contract_id: USDC_SAC.into(),
                topics_xdr: "not json".into(),
            },
        ];
        let confirmed = confirmed_orphan_strkeys(&rows, &net);
        assert_eq!(confirmed, vec![USDC_SAC.to_string()]);
    }

    #[tokio::test]
    async fn pg_target_short_circuits() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://noop")
            .expect("lazy connect must succeed without I/O");
        let sink = Sink::Postgres(pool);
        let stats = execute(&sink, false, xdr_parser::MAINNET_PASSPHRASE)
            .await
            .expect("PG short-circuit must not error");
        assert_eq!(stats.crypto_confirmed, 0);
        assert_eq!(stats.rows_inserted, 0);
    }

    /// End-to-end against a real ClickHouse — exercises the orphan×events join,
    /// the Rust crypto-match gate, and the corrected-row INSERT/RMT collapse.
    /// Gated on `CLICKHOUSE_URL` (skips cleanly when unset). Isolated in a
    /// throwaway database.
    ///
    ///   docker compose up -d clickhouse
    ///   CLICKHOUSE_URL=http://localhost:8123 \
    ///       cargo test -p backfill-runner sac_orphan_relabel
    #[tokio::test]
    async fn relabel_e2e_flips_confirmed_sac_and_gates_non_sac() {
        use db_clickhouse::{Config, apply_init_sql, client};

        let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
            eprintln!("CLICKHOUSE_URL not set — skipping sac_orphan_relabel e2e");
            return;
        };
        let db = "ch_test_0294_relabel";
        let base = client(&Config {
            url: url.clone(),
            ..Config::from_env()
        });
        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("drop pre-existing throwaway db");
        base.query(&format!("CREATE DATABASE {db}"))
            .execute()
            .await
            .expect("create throwaway db");

        let cl = client(&Config {
            url,
            database: db.to_string(),
            ..Config::from_env()
        });
        apply_init_sql(&cl).await.expect("apply init schema");

        // Two orphans: id 7001 = the real USDC SAC (should flip), id 7002 = a
        // non-SAC contract emitting the same USDC-shaped event (gate must reject).
        cl.query(&format!(
            "INSERT INTO soroban_contracts \
             (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac, name) VALUES \
             (7001, '{USDC_SAC}', NULL, 0, NULL, NULL, NULL, false, NULL), \
             (7002, '{NOT_USDC_SAC}', NULL, 0, NULL, 0, NULL, false, NULL)"
        ))
        .execute()
        .await
        .expect("seed soroban_contracts orphans");

        cl.query(&format!(
            "INSERT INTO soroban_events \
             (contract_id, transaction_id, ledger_sequence, event_index, event_type, signature, topics_xdr, data_xdr) VALUES \
             (7001, 1, 60000000, 0, 2, 'transfer', '{USDC_TOPICS}', ''), \
             (7002, 2, 60000000, 0, 2, 'transfer', '{USDC_TOPICS}', '')"
        ))
        .execute()
        .await
        .expect("seed soroban_events");

        let sink = Sink::Clickhouse(cl.clone());
        let stats = execute(&sink, false, xdr_parser::MAINNET_PASSPHRASE)
            .await
            .expect("relabel run");
        assert_eq!(stats.orphans_scanned, 2, "both orphans emit a SAC event");
        assert_eq!(stats.crypto_confirmed, 1, "only the real USDC SAC matches");
        assert_eq!(stats.rows_inserted, 1);

        let row = |cid: &'static str| {
            let cl = cl.clone();
            async move {
                cl.query("SELECT is_sac, contract_type FROM soroban_contracts FINAL WHERE contract_id = ?")
                    .bind(cid)
                    .fetch_one::<(bool, Option<i16>)>()
                    .await
                    .expect("contract row")
            }
        };
        assert_eq!(
            row(USDC_SAC).await,
            (true, Some(0)),
            "USDC SAC flipped to is_sac/Token"
        );
        assert_eq!(
            row(NOT_USDC_SAC).await,
            (false, None),
            "non-SAC orphan untouched"
        );

        // Idempotent: after the flip, 7001 is no longer an orphan; 7002 stays
        // rejected → a re-run confirms/inserts nothing.
        let stats2 = execute(&sink, false, xdr_parser::MAINNET_PASSPHRASE)
            .await
            .expect("idempotent re-run");
        assert_eq!(stats2.crypto_confirmed, 0, "re-run confirms nothing");
        assert_eq!(stats2.rows_inserted, 0, "re-run inserts nothing");
        assert_eq!(row(USDC_SAC).await, (true, Some(0)), "still flipped");

        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("cleanup throwaway db");
    }
}
