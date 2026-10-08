//! The live path of task 0620 against a real ClickHouse: a contract whose
//! instance changed in a ledger gets its metadata from its own functions when
//! its program declares `decimals` (a token) or `name` and `symbol` (an NFT);
//! a program declaring only `name` gets nothing.
//!
//! The token is mainnet's `CDDNHP3B…X4RV` (the most common token program);
//! its program and instance are the `contract-executor` fixtures, and the
//! expected values are what Soroban RPC `simulateTransaction` returned.
//!
//! Gated on `CLICKHOUSE_URL`; uses its own database, created and dropped here.

use contract_executor::Ledger;
use db_clickhouse::{Config, apply_init_sql, client};
use indexer::contract_metadata::ledger_metadata_writes;
use stellar_xdr::{LedgerEntryData, Limits, ReadXdr, WriteXdr};
use xdr_parser::contract_instance::ExtractedContractInstance;

const DB: &str = "indexer_test_0620_live_metadata";
const TOKEN: [u8; 32] = [
    0xc6, 0xd3, 0xbf, 0x61, 0xc4, 0xc4, 0xad, 0x1f, 0x0f, 0x6f, 0x56, 0x93, 0x98, 0xbf, 0xad, 0x5c,
    0xa7, 0x63, 0x72, 0xf1, 0x0a, 0x07, 0x5c, 0x7c, 0x5d, 0x6b, 0xcd, 0xf4, 0x91, 0x22, 0xa5, 0xcb,
];
const PROGRAM: &str = "0a41411f7250a78da15d6cd6cf78086e19d98bc917dd0a51e03a34f5ab969781";

fn decoded(base64: &str) -> LedgerEntryData {
    LedgerEntryData::from_xdr_base64(base64.trim(), Limits::none()).unwrap()
}

/// `instance` under another contract id, running another program.
fn changed_instance(
    instance: &LedgerEntryData,
    contract: [u8; 32],
    program: [u8; 32],
) -> ExtractedContractInstance {
    let mut instance = instance.clone();
    if let LedgerEntryData::ContractData(ref mut data) = instance {
        data.contract =
            stellar_xdr::ScAddress::Contract(stellar_xdr::ContractId(stellar_xdr::Hash(contract)));
        if let stellar_xdr::ScVal::ContractInstance(ref mut i) = data.val {
            i.executable = stellar_xdr::ContractExecutable::Wasm(stellar_xdr::Hash(program));
        }
    }
    ExtractedContractInstance {
        contract,
        data_xdr: instance.to_xdr(Limits::none()).unwrap(),
        ledger_sequence: 64_818_615,
    }
}

#[tokio::test]
async fn a_changed_instance_yields_metadata_from_its_functions() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping live token metadata e2e test");
        return;
    };
    let base = client(&Config {
        url,
        ..Config::from_env()
    });
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .unwrap();
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .unwrap();
    let ch = base.clone().with_database(DB);
    apply_init_sql(&ch).await.expect("apply init.sql");

    let LedgerEntryData::ContractCode(code) = decoded(include_str!(
        "../../contract-executor/tests/fixtures/code.xdr.b64"
    )) else {
        panic!("code fixture is not contract code");
    };
    // The token's program, declaring the three functions; and the same bytes
    // under two other hashes: one declaring `name` and `symbol` as an NFT
    // does, one declaring only `name`.
    ch.query(
        "INSERT INTO wasm_programs (wasm_hash, metadata, code) VALUES \
         (unhex(?), '{\"functions\":[{\"name\":\"decimals\"},{\"name\":\"name\"},{\"name\":\"symbol\"}]}', unhex(?)), \
         (unhex(repeat('dd', 32)), '{\"functions\":[{\"name\":\"name\"},{\"name\":\"symbol\"}]}', unhex(?)), \
         (unhex(repeat('ee', 32)), '{\"functions\":[{\"name\":\"name\"}]}', unhex(?))",
    )
    .bind(PROGRAM)
    .bind(hex::encode(code.code.as_slice()))
    .bind(hex::encode(code.code.as_slice()))
    .bind(hex::encode(code.code.as_slice()))
    .execute()
    .await
    .expect("seed programs");

    let token_instance = decoded(include_str!(
        "../../contract-executor/tests/fixtures/instance.xdr.b64"
    ));
    // The same instance under two other contract ids, pointing at the other
    // two programs.
    let changed = [
        ExtractedContractInstance {
            contract: TOKEN,
            data_xdr: token_instance.to_xdr(Limits::none()).unwrap(),
            ledger_sequence: 64_818_615,
        },
        changed_instance(&token_instance, [6; 32], [0xdd; 32]),
        changed_instance(&token_instance, [7; 32], [0xee; 32]),
    ];
    let ledger = Ledger {
        sequence: 64_818_615,
        timestamp: 1_791_380_000,
        protocol_version: 29,
        network_id: [0; 32],
    };

    let writes = ledger_metadata_writes(&ch, &ledger, &changed, &[])
        .await
        .expect("query runs");

    assert_eq!(writes.len(), 2, "the token and the NFT get a write");
    let nft = &writes[0];
    assert_eq!(
        nft.contract_id,
        "CADAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMSST"
    );
    assert_eq!(nft.metadata.decimals, None);
    assert_eq!(nft.metadata.symbol.as_deref(), Some("SMOL"));
    let w = &writes[1];
    assert_eq!(
        w.contract_id,
        "CDDNHP3BYTCK2HYPN5LJHGF7VVOKOY3S6EFAOXD4LVV435EREKS4X4RV"
    );
    assert_eq!(w.metadata.decimals, Some(0));
    assert_eq!(w.metadata.symbol.as_deref(), Some("SMOL"));
    assert_eq!(
        w.metadata.name.as_deref(),
        Some("We\u{2019}re Gonna Make the Best of It")
    );
    assert_eq!(w.ledger, 64_818_615);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .unwrap();
}

/// Verification over real mainnet ledgers: set `TOKEN_METADATA_LEDGERS` to a
/// directory of `<sequence>.xdr.zst` ledger files (e.g. ledgers that deployed
/// tokens, from the public data lake) and `TOKEN_METADATA_REAL_DB` to a
/// database holding production's programs and instances. Prints every write
/// the live path produces, to compare with Soroban RPC `simulateTransaction`.
#[tokio::test]
async fn real_ledgers_print_live_writes() {
    let (Ok(url), Ok(db), Ok(dir)) = (
        std::env::var("CLICKHOUSE_URL"),
        std::env::var("TOKEN_METADATA_REAL_DB"),
        std::env::var("TOKEN_METADATA_LEDGERS"),
    ) else {
        eprintln!(
            "CLICKHOUSE_URL / TOKEN_METADATA_REAL_DB / TOKEN_METADATA_LEDGERS not set — skipping"
        );
        return;
    };
    let ch = client(&Config {
        url,
        ..Config::from_env()
    })
    .with_database(&db);
    let mut writes_total = 0;
    for file in std::fs::read_dir(&dir).unwrap() {
        let raw = std::fs::read(file.unwrap().path()).unwrap();
        let xdr = xdr_parser::decompress_zstd(&raw).unwrap();
        let batch = xdr_parser::deserialize_batch(&xdr).unwrap();
        let parsed = indexer::handler::process::parse_ledger(&batch.ledger_close_metas[0]);
        let ledger = Ledger {
            sequence: parsed.ledger.sequence,
            timestamp: parsed.ledger.closed_at as u64,
            protocol_version: parsed.ledger.protocol_version,
            network_id: xdr_parser::sac::network_id(xdr_parser::MAINNET_PASSPHRASE),
        };
        let writes =
            ledger_metadata_writes(&ch, &ledger, &parsed.contract_instances, &parsed.programs)
                .await
                .unwrap();
        for w in writes {
            writes_total += 1;
            println!(
                "LIVE\t{}\t{}\t{:?}\t{:?}\t{:?}",
                w.ledger, w.contract_id, w.metadata.decimals, w.metadata.name, w.metadata.symbol
            );
        }
    }
    assert!(
        writes_total > 0,
        "the ledgers must change at least one token instance"
    );
}
