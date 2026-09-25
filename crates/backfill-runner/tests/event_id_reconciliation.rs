//! Task 0541 / ADR 0059 — the event ids we store and the ones we parse are the
//! ids stellar-rpc returns.
//!
//! For five recent ledgers, `getEvents` is the arbiter and both sides answer to
//! it:
//!
//! ```text
//! rpc ids of ledger L  ==  ids built in SQL from soroban_events
//! rpc ids of ledger L  ==  ids assigned by the parser on the archive's XDR
//! ```
//!
//! The rpc window is short (about 7 days), so the ledgers are taken from the
//! index tip. `getEvents` returns contract events only — the fee events of the
//! native SAC are contract events too, so the charges and refunds, and with
//! them the sentinel ids, are included; diagnostic events exist on neither
//! side.
//!
//! Gated on a client certificate for the production ClickHouse (read-only),
//! found where `api --bin local` finds it — `infra-hetzner/ca/out/$USER/`
//! (`$USER.crt` + `$USER.key`, git-ignored) — or at absolute
//! `EVENT_CH_CERT`/`EVENT_CH_KEY`. Skips when neither exists, so `cargo test`
//! stays offline.
//!
//! ```bash
//! cargo test -p backfill-runner --test event_id_reconciliation -- --nocapture
//! ```
//!
//! Optional: `EVENT_CH_DOMAIN` (default `ch.sorobanscan.rumblefish.dev`),
//! `EVENT_RPC_URL` (default a public mainnet Soroban RPC),
//! `EVENT_ARCHIVE_URL` (default the public `aws-public-blockchain` bucket).

use std::collections::BTreeSet;
use std::time::Duration;

/// How far below the tip to sample, so the ledgers are fully indexed.
const BEHIND_TIP: u32 = 200;
const LEDGERS: u32 = 5;

#[tokio::test(flavor = "multi_thread")]
async fn stored_and_parsed_event_ids_match_stellar_rpc() {
    let user = std::env::var("USER").unwrap_or_default();
    let local = format!(
        "{}/../../infra-hetzner/ca/out/{user}/{user}",
        env!("CARGO_MANIFEST_DIR")
    );
    let cert = std::env::var("EVENT_CH_CERT").unwrap_or_else(|_| format!("{local}.crt"));
    let key = std::env::var("EVENT_CH_KEY").unwrap_or_else(|_| format!("{local}.key"));
    if !std::path::Path::new(&cert).exists() || !std::path::Path::new(&key).exists() {
        eprintln!("no client certificate at {cert} — skipping the event id reconciliation");
        return;
    }
    let domain = env_or("EVENT_CH_DOMAIN", "ch.sorobanscan.rumblefish.dev");
    let rpc_url = env_or(
        "EVENT_RPC_URL",
        "https://soroban-rpc.mainnet.stellar.gateway.fm/",
    );
    let archive = env_or(
        "EVENT_ARCHIVE_URL",
        "https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet",
    );
    let bundle = db_clickhouse::mtls::MtlsBundle {
        cert_pem: std::fs::read_to_string(&cert).expect("read EVENT_CH_CERT"),
        key_pem: std::fs::read_to_string(&key).expect("read EVENT_CH_KEY"),
        ca_pem: String::new(),
    };
    let ch = db_clickhouse::mtls::client_with_mtls(&domain, &bundle, db_clickhouse::PROD_DATABASE)
        .expect("mTLS ClickHouse client");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("soroban-block-explorer/event-id-reconciliation")
        .build()
        .unwrap();
    // SAFETY: set before any parse; `parse_ledger` needs the passphrase.
    unsafe {
        std::env::set_var(
            "STELLAR_NETWORK_PASSPHRASE",
            "Public Global Stellar Network ; September 2015",
        );
    }
    indexer::handler::process::init_network_id().expect("network id");

    // Before the cutover the table still carries the old key, and there is
    // nothing to reconcile (task 0541 phase 4).
    let rekeyed = ch
        .query(
            "SELECT count() FROM system.columns \
             WHERE database = currentDatabase() AND table = 'soroban_events' \
               AND name = 'transaction_index'",
        )
        .fetch_one::<u64>()
        .await
        .expect("soroban_events columns");
    if rekeyed == 0 {
        eprintln!("soroban_events is not keyed by the rpc id yet — skipping");
        return;
    }

    let tip = ch
        .query("SELECT toUInt32(max(sequence)) FROM ledgers")
        .fetch_one::<u32>()
        .await
        .expect("index tip");
    let ledgers: Vec<u32> = (0..LEDGERS).map(|i| tip - BEHIND_TIP - i * 37).collect();
    println!("tip {tip}, checking ledgers {ledgers:?}");

    for ledger in ledgers {
        let rpc = rpc_event_ids(&http, &rpc_url, ledger).await;
        assert!(
            !rpc.is_empty(),
            "getEvents returned nothing for ledger {ledger} — outside the rpc window?"
        );

        let stored = stored_event_ids(&ch, ledger).await;
        assert_eq!(
            stored,
            rpc,
            "ledger {ledger}: stored ids differ from getEvents ({} vs {}); \
             only in the table: {:?}; only in rpc: {:?}",
            stored.len(),
            rpc.len(),
            stored.difference(&rpc).take(5).collect::<Vec<_>>(),
            rpc.difference(&stored).take(5).collect::<Vec<_>>(),
        );

        let parsed = parsed_event_ids(&http, &archive, ledger).await;
        assert_eq!(
            parsed,
            rpc,
            "ledger {ledger}: parsed ids differ from getEvents; \
             only in the parse: {:?}; only in rpc: {:?}",
            parsed.difference(&rpc).take(5).collect::<Vec<_>>(),
            rpc.difference(&parsed).take(5).collect::<Vec<_>>(),
        );
        println!(
            "ledger {ledger}: {} ids match on all three sides",
            rpc.len()
        );
    }
}

/// Every event id `getEvents` reports for one ledger.
async fn rpc_event_ids(http: &reqwest::Client, url: &str, ledger: u32) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let pagination = match &cursor {
            Some(c) => serde_json::json!({ "cursor": c, "limit": 10_000 }),
            None => serde_json::json!({ "limit": 10_000 }),
        };
        // `endLedger` is exclusive: `[ledger, ledger]` is empty.
        let mut params = serde_json::json!({
            "endLedger": ledger + 1,
            "filters": [],
            "pagination": pagination,
        });
        if cursor.is_none() {
            params["startLedger"] = serde_json::json!(ledger);
        }
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "getEvents", "params": params,
        });
        let resp: serde_json::Value = http
            .post(url)
            .json(&body)
            .send()
            .await
            .expect("getEvents request")
            .json()
            .await
            .expect("getEvents json");
        assert!(
            resp.get("error").is_none(),
            "getEvents error for ledger {ledger}: {}",
            resp["error"]
        );
        let events = resp["result"]["events"]
            .as_array()
            .expect("getEvents events")
            .clone();
        let last = events
            .last()
            .and_then(|e| e["id"].as_str().map(String::from));
        for e in &events {
            if e["ledger"].as_u64() == Some(u64::from(ledger)) {
                out.insert(e["id"].as_str().expect("event id").to_string());
            }
        }
        // `endLedger` bounds the answer, so a short page is the last one.
        match last {
            Some(c) if events.len() == 10_000 => cursor = Some(c),
            _ => return out,
        }
    }
}

/// The same ids, built from the stored key (ADR 0059's id format).
async fn stored_event_ids(ch: &clickhouse::Client, ledger: u32) -> BTreeSet<String> {
    ch.query(&format!(
        "SELECT DISTINCT concat( \
            leftPad(toString(bitShiftLeft(toUInt64(ledger_sequence), 32) \
                             + bitShiftLeft(toUInt64(transaction_index), 12) \
                             + toUInt64(operation_index)), 19, '0'), \
            '-', leftPad(toString(event_index), 10, '0')) \
         FROM soroban_events \
         WHERE ledger_sequence = {ledger} AND intDiv(ledger_sequence, 500000) = {partition}",
        partition = ledger / 500_000
    ))
    .fetch_all::<String>()
    .await
    .expect("stored event ids")
    .into_iter()
    .collect()
}

/// The same ids, assigned by the parser on the archive's own XDR.
async fn parsed_event_ids(http: &reqwest::Client, archive: &str, ledger: u32) -> BTreeSet<String> {
    const FILES_PER_PARTITION: u32 = 64_000;
    let part_start = (ledger / FILES_PER_PARTITION) * FILES_PER_PARTITION;
    let part_end = part_start + FILES_PER_PARTITION - 1;
    let url = format!(
        "{archive}/{:08X}--{part_start}-{part_end}/{:08X}--{ledger}.xdr.zst",
        u32::MAX - part_start,
        u32::MAX - ledger,
    );
    let bytes = http
        .get(&url)
        .send()
        .await
        .expect("archive request")
        .error_for_status()
        .expect("archive object")
        .bytes()
        .await
        .expect("archive body");
    let xdr = xdr_parser::decompress_zstd(&bytes).expect("zstd");
    let batch = xdr_parser::deserialize_batch(&xdr).expect("LedgerCloseMetaBatch");
    batch
        .ledger_close_metas
        .iter()
        .flat_map(|meta| indexer::handler::process::parse_ledger(meta).events)
        .flat_map(|(_, events)| events)
        // `getEvents` reports contract-scoped events only.
        .filter(|e| e.contract_id.is_some())
        .map(|e| e.event_id.to_rpc_string())
        .collect()
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}
