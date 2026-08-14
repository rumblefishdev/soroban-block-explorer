//! Full end-to-end for the on-chain token-metadata pipeline (task 0297), all
//! THREE fields (`name` / `symbol` / `decimals`) against a live ClickHouse:
//!
//!   raw `ScVal` METADATA struct (as `ledger_entry_changes` produces it)
//!     → `xdr_parser::extract_token_metadata`        (parse all 3)
//!     → `db_clickhouse::build_metadata_rows`        (map to the side-table row)
//!     → INSERT `soroban_contract_metadata`          (real CH write)
//!     → read back two ways:
//!         (a) `SELECT … FROM soroban_contract_metadata FINAL`   (round-trip)
//!         (b) the assets read-compose join shape (`… FINAL` + COALESCE name,
//!             nullIf symbol, coalesce(decimals,7))             (API read path)
//!
//! Uses a Soroban token with `decimals = 6` (NOT the protocol default 7) so the
//! read provably surfaces the stored value, not the fallback. The whole chain —
//! parse → write → read-compose — runs against a real server, closing the gap
//! the per-layer unit tests cannot.
//!
//! Gated on `CLICKHOUSE_URL` (skips cleanly when unset). Run locally:
//!   CLICKHOUSE_URL=http://localhost:18123 CLICKHOUSE_USER=default \
//!     CLICKHOUSE_PASSWORD=clickhouse cargo test -p db-clickhouse --test metadata_e2e

use clickhouse::Row;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::SorobanContractMetadataRow;
use db_clickhouse::persist::stage::build_metadata_rows;
use db_clickhouse::{Config, apply_init_sql, client};
use serde::Deserialize;
use stellar_xdr::{
    ContractExecutable, Hash, ScContractInstance, ScMap, ScMapEntry, ScString, ScSymbol, ScVal,
};
use xdr_parser::token_metadata::extract_token_metadata;
use xdr_parser::types::ExtractedContractMetadata;

const CONTRACT: &str = "CMETAE2E0000000000000000000000000000000000000000000000001";
const E2E_LEDGER: u32 = 99_999_777;

fn sym(s: &str) -> ScVal {
    ScVal::Symbol(ScSymbol(s.try_into().unwrap()))
}
fn sstr(s: &str) -> ScVal {
    ScVal::String(ScString(s.try_into().unwrap()))
}

/// A contract-instance `ScVal` carrying the SEP-41/OZ `METADATA` struct, exactly
/// the shape `ledger_entry_changes` hands to `extract_token_metadata`.
fn instance_with_metadata(name: &str, symbol: &str, decimals: u32) -> ScVal {
    let meta = ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: sym("decimal"),
                val: ScVal::U32(decimals),
            },
            ScMapEntry {
                key: sym("name"),
                val: sstr(name),
            },
            ScMapEntry {
                key: sym("symbol"),
                val: sstr(symbol),
            },
        ]
        .try_into()
        .unwrap(),
    )));
    let storage = ScMap(
        vec![ScMapEntry {
            key: sym("METADATA"),
            val: meta,
        }]
        .try_into()
        .unwrap(),
    );
    ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::Wasm(Hash([0xAB; 32])),
        storage: Some(storage),
    })
}

#[derive(Debug, Row, Deserialize)]
struct MetaRow {
    name: Option<String>,
    symbol: Option<String>,
    decimals: Option<u32>,
}

#[derive(Debug, Row, Deserialize)]
struct ComposedRow {
    name: Option<String>,
    symbol: Option<String>,
    decimals: u32,
}

#[tokio::test]
async fn token_metadata_three_fields_end_to_end() {
    let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
        eprintln!("CLICKHOUSE_URL not set — skipping metadata e2e");
        return;
    };
    let cl = client(&Config {
        url,
        ..Config::from_env()
    });
    apply_init_sql(&cl).await.expect("apply init.sql");

    let cid_surrogate = ids::contract_id(CONTRACT);
    let clear = [
        format!("ALTER TABLE soroban_contract_metadata DELETE WHERE contract_id = '{CONTRACT}'"),
        format!("ALTER TABLE soroban_contracts DELETE WHERE id = {cid_surrogate}"),
        format!("ALTER TABLE assets DELETE WHERE contract_id = {cid_surrogate}"),
    ];
    for stmt in &clear {
        let _ = cl.query(stmt).execute().await;
    }

    // 1) PARSE — recover all three fields from the raw on-chain METADATA ScVal
    //    (the exact value `ledger_entry_changes` feeds the producer). decimals = 6
    //    (a non-default) so the stored value provably flows end to end.
    let val = instance_with_metadata("liquidFi bridge token", "lUSDC", 6);
    let md = extract_token_metadata(&val).expect("METADATA parsed from raw ScVal");
    assert_eq!(md.name.as_deref(), Some("liquidFi bridge token"));
    assert_eq!(md.symbol.as_deref(), Some("lUSDC"));
    assert_eq!(md.decimals, Some(6));

    let writes = vec![ExtractedContractMetadata {
        contract_id: CONTRACT.to_string(),
        metadata: md,
        ledger: E2E_LEDGER,
    }];

    // 2) BUILD + WRITE — real map fn → side-table row → CH insert.
    let rows = build_metadata_rows(&writes);
    assert_eq!(rows.len(), 1);
    let mut insert = cl
        .insert::<SorobanContractMetadataRow>("soroban_contract_metadata")
        .await
        .expect("open insert");
    insert.write(&rows[0]).await.expect("write metadata row");
    insert.end().await.expect("commit metadata insert");

    // Seed the contract + a Soroban asset so the read-compose join resolves:
    //   assets.contract_id (surrogate) → soroban_contracts.id → .contract_id (strkey)
    //   → soroban_contract_metadata.contract_id (strkey).
    cl.query(
        "INSERT INTO soroban_contracts \
         (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, \
          deployed_at_ledger, contract_type, is_sac) \
         VALUES (?, ?, NULL, ?, NULL, ?, 3, false)",
    )
    .bind(cid_surrogate)
    .bind(CONTRACT)
    .bind(i64::from(E2E_LEDGER))
    .bind(i64::from(E2E_LEDGER))
    .execute()
    .await
    .expect("seed soroban_contracts");

    cl.query(
        "INSERT INTO assets \
         (asset_type, asset_code, issuer_id, contract_id) \
         VALUES (3, '', 0, ?)",
    )
    .bind(cid_surrogate)
    .execute()
    .await
    .expect("seed assets");

    // 3a) READ-BACK from the side table (FINAL = latest whole row).
    let direct: MetaRow = cl
        .query(
            "SELECT name, symbol, decimals FROM soroban_contract_metadata FINAL \
             WHERE contract_id = ?",
        )
        .bind(CONTRACT)
        .fetch_one()
        .await
        .expect("metadata row round-trips");
    assert_eq!(direct.name.as_deref(), Some("liquidFi bridge token"));
    assert_eq!(direct.symbol.as_deref(), Some("lUSDC"));
    assert_eq!(direct.decimals, Some(6));

    // 3b) READ-COMPOSE via the API assets shape (FINAL join + COALESCE name /
    //     nullIf symbol / coalesce decimals).
    let composed: ComposedRow = cl
        .query(
            "SELECT \
                coalesce(nullIf(m.name, ''), if(a.asset_type = 0, 'Stellar Lumens', NULL)) AS name, \
                nullIf(m.symbol, '')    AS symbol, \
                coalesce(m.decimals, 7) AS decimals \
             FROM assets a FINAL \
             LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id \
             LEFT JOIN ( \
                 SELECT contract_id, name, symbol, decimals \
                 FROM soroban_contract_metadata FINAL \
             ) m ON m.contract_id = sc.contract_id \
             WHERE a.contract_id = ?",
        )
        .bind(cid_surrogate)
        .fetch_one()
        .await
        .expect("compose row");
    assert_eq!(composed.name.as_deref(), Some("liquidFi bridge token"));
    assert_eq!(composed.symbol.as_deref(), Some("lUSDC"));
    assert_eq!(
        composed.decimals, 6,
        "decimals surfaces the STORED 6, not the default 7"
    );

    for stmt in &clear {
        let _ = cl.query(stmt).execute().await;
    }
}
