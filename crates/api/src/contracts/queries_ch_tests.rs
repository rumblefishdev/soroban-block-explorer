//! ClickHouse-backed checks for the contract-detail SQL (task 0548).
//!
//! These queries are string literals, so a broken one compiles and fails only
//! when ClickHouse parses it. The first cut of the CAP-85 resolution did
//! exactly that: `argMax(wasm_hash, ledger) AS wasm_hash, max(ledger) AS ledger`
//! — the alias shadows the column inside `argMax`, ClickHouse rejects the query
//! (Code 184), and every contract page and decompiler request would have
//! returned 500. Nothing in the suite executed the SQL, so nothing noticed.
//!
//! This runs the real queries against the real `init.sql`, in a throwaway
//! database created and dropped here, so it can never touch shared data.
//! Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api contract_detail_sql_runs_and_resolves_references

use super::*;

const DB: &str = "api_test_0548_contract_detail";

fn hash_hex(byte: &str) -> String {
    byte.repeat(32)
}

async fn seed(ch: &clickhouse::Client) {
    for sql in [
        "INSERT INTO soroban_contracts \
         (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, \
          contract_type, is_sac, executable_owner_id, executable_tag) VALUES \
         (1,  'CWASM',         unhex(repeat('aa', 32)), 100, NULL, 100, 1, false, NULL, NULL), \
         (2,  'CSAC',          NULL,                    100, NULL, 100, 0, true,  NULL, NULL), \
         (3,  'CFLEET',        NULL,                    100, NULL, 100, 1, false, 42,   'fleet-v2'), \
         (4,  'CFLEETMISSING', NULL,                    100, NULL, 100, 1, false, 43,   'no-such-tag'), \
         (42, 'COWNER',        unhex(repeat('dd', 32)),  90, NULL,  90, 1, false, NULL, NULL)",
        // Two targets for one tag: the owner re-pointed the fleet. The NEWER one
        // must win, and it is inserted second so insert order cannot fake it.
        "INSERT INTO contract_executable_refs (owner_id, tag, wasm_hash, ledger) VALUES \
         (42, 'fleet-v2', unhex(repeat('bb', 32)), 64500123), \
         (42, 'fleet-v2', unhex(repeat('cc', 32)), 64500000)",
        r#"INSERT INTO wasm_interface_metadata (wasm_hash, metadata) VALUES
           (unhex(repeat('aa', 32)), '{"functions":[],"upgradeable":true}'),
           (unhex(repeat('bb', 32)), '{"functions":[{"name":"f"}],"upgradeable":false}')"#,
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }
}

#[tokio::test]
async fn contract_detail_sql_runs_and_resolves_references() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping contract-detail SQL check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    seed(&ch).await;

    // ---- fetch_contract: the query must RUN (the Code 184 regression) ----
    let detail = |cid: &'static str| {
        let ch = ch.clone();
        async move {
            fetch_contract(&ch, cid)
                .await
                .unwrap_or_else(|e| panic!("fetch_contract({cid}) must execute: {e}"))
                .unwrap_or_else(|| panic!("fetch_contract({cid}) must find the row"))
        }
    };

    let own = detail("CWASM").await;
    assert_eq!(own.wasm_hash.as_deref(), Some(hash_hex("aa").as_str()));
    assert_eq!(own.executable_owner, None);
    assert_eq!(own.executable_tag, None);
    assert_eq!(own.upgradeable, Some(true));

    let sac = detail("CSAC").await;
    assert_eq!(sac.wasm_hash, None, "a SAC has no code entry to hash");
    assert_eq!(sac.upgradeable, Some(false));

    let fleet = detail("CFLEET").await;
    assert_eq!(
        fleet.wasm_hash.as_deref(),
        Some(hash_hex("bb").as_str()),
        "a fleet member reports the code its reference resolves to — the NEWER target"
    );
    assert_eq!(fleet.executable_owner.as_deref(), Some("COWNER"));
    assert_eq!(fleet.executable_tag.as_deref(), Some("fleet-v2"));
    assert_eq!(
        fleet.upgradeable, None,
        "its own code is not what decides whether it can change"
    );

    let dangling = detail("CFLEETMISSING").await;
    assert_eq!(
        dangling.wasm_hash, None,
        "a tag with no target is NULL — never 32 zero bytes from a LEFT JOIN miss"
    );
    assert_eq!(dangling.executable_tag.as_deref(), Some("no-such-tag"));

    // ---- fetch_wasm_interface: the decompiler/ABI path ----
    let iface = |cid: &'static str| {
        let ch = ch.clone();
        async move {
            fetch_wasm_interface(&ch, cid)
                .await
                .unwrap_or_else(|e| panic!("fetch_wasm_interface({cid}) must execute: {e}"))
                .unwrap_or_else(|| panic!("fetch_wasm_interface({cid}) must find the row"))
        }
    };

    let own = iface("CWASM").await;
    assert_eq!(own.wasm_hash.as_deref(), Some(hash_hex("aa").as_str()));
    assert!(own.interface_metadata.is_some());

    let sac = iface("CSAC").await;
    assert_eq!(sac.wasm_hash, None);
    assert!(sac.interface_metadata.is_none());

    let fleet = iface("CFLEET").await;
    assert_eq!(
        fleet.wasm_hash.as_deref(),
        Some(hash_hex("bb").as_str()),
        "the decompiler fetches the code the fleet member actually runs"
    );
    assert!(
        fleet.interface_metadata.is_some(),
        "and its ABI is the ABI of that code"
    );

    let dangling = iface("CFLEETMISSING").await;
    assert_eq!(dangling.wasm_hash, None);
    assert!(dangling.interface_metadata.is_none());

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
