//! Every deposit transaction on mainnet, through the real share-token
//! detector (task 0374, step 15).
//!
//! The inline tests pin four hand-copied payloads; this runs the compiled
//! `detect_share_tokens` over ALL 85k+ deposit transactions of every
//! registered pool and checks the result against ground truth that was
//! verified ON CHAIN (T6: 16 ambiguous pools confirmed 16/16 against
//! `share_id()`, plus 12 clean-pool spot checks). The T6 research measured
//! its rule with SQL imitating the logic; this test runs the logic.
//!
//! Skipped (passes trivially) when `SHARE_TOKEN_CORPUS` is unset. Harvest:
//! the deposit-tx list plus `deposit_liquidity`/`mint` events grouped per
//! transaction — the exact recipe lives in task 0374 (step 15 notes).
//!
//! ```sh
//! SHARE_TOKEN_CORPUS=/path/share_token_corpus.jsonl \
//!   cargo test -p xdr-parser --test share_token_real_corpus -- --nocapture
//! ```

use std::collections::BTreeMap;

use domain::ContractEventType;
use xdr_parser::pool_router::detect_share_tokens;
use xdr_parser::types::{EventSource, ExtractedEvent};

#[derive(serde::Deserialize)]
struct CorpusEvent {
    contract: String,
    idx: u32,
    topics: String,
    data: String,
}

#[derive(serde::Deserialize)]
struct CorpusTx {
    ledger: i64,
    tx: i64,
    events: Vec<CorpusEvent>,
}

/// (pool, share token) pairs confirmed against `share_id()` by read-only
/// simulation on mainnet — the T6 resolution table, verbatim.
const ON_CHAIN_TRUTH: &[(&str, &str)] = &[
    (
        "CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O",
        "CDOY7ILRR7PDGLBXZUPSENB6XOET77PR2JY3HXDGQS3TS4T764OYBUGO",
    ),
    (
        "CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE",
        "CAVKLYY4RWFQBRA2YI5GTGGXKUKJQI3JLAHDGXMS7L5RDH6X6A47NMOZ",
    ),
    (
        "CAB6MICC2WKRT372U3FRPKGGVB5R3FDJSMWSLPF2UJNJPYMBZ76RQVYE",
        "CA2KG5ZCUZSAHC2MMDSNPRLTFPRAS7OUR5EYNGJW3FHJPLYB3WLO3WQK",
    ),
    (
        "CAMXZXXBD7DFBLYLHUW24U4MY37X7SU5XXT5ZVVUBXRXWLAIM7INI7G2",
        "CDMRHKJCYYHZTRQVR7NY43PR7ISMRBYC2O57IMVAQ7B7P2I2XGIZLI5E",
    ),
    (
        "CB7FKGSTHP75ORTIZGGMVUTQLEMVTSEOI4QORQPCABJSGTAATDFCE2YV",
        "CDXFLNBIBD75DKS2HLNMMWMBMTTYWAJLH7OUUH3S5NWZ3FD7LERNOE4W",
    ),
    (
        "CBL7MWLEZ4SU6YC5XL4T3WXKNKNO2UQVDVONOQSW5VVCYFWORROHY4AM",
        "CDA7DVSWI2VKGK72ZZW4C2ASBOJEVFNA72FOXXR7BYE6QEBYQUTKMIXU",
    ),
    (
        "CBP67KMDEOZHRJ5YM7DS4MOVYJH66YCEFCVBDBQVNTI46X37DU6KOZMB",
        "CDWSTBWL2355YNAE3FCV3WWSUDEDURHAHWFMO5UZ4AZIN7V7O6Q7LW2A",
    ),
    (
        "CCFGZJTHQZGDZP5PK6WMLKHKJ72ACSVMJGCI2NFR7Q6EAVSKWLJB3ZH3",
        "CAFEXSVTYRXJSAQWI6JZE5VL3US5XMDKDNQFACPYTKJNSHCHCU2ER4YE",
    ),
    (
        "CCLZQDL5LY2DBPNNFBRKPSROGFGTT7Y7AI2SM6QUI3SUTTKA672X4PDF",
        "CA73MQDXDHT7Z37KIWP5BCRGAKOXDK2FLR3OPVKNUJUEHK5SWL74SE4K",
    ),
    (
        "CCPR2Q3F2TPOVKPKTAHGLMMWPL5RBNBYDWGGUNBYP7KV7W245IAJXRI7",
        "CDRYIHHD3V6E4L3445V62A77WYWQYAJWLHTRBRITBTCDIOZ2D6UJ7ICJ",
    ),
    (
        "CCSY43EHJAHT3NQDYKAMJXRFBEEH7OXDL3J3VNGO33UUSEXWNN27GBIZ",
        "CC4BPROIXISEFC7UKTB2HYBLNSNP27WNCR7YNZOHXLTPTGDKFMKYQ2YN",
    ),
    (
        "CCY2PXGMKNQHO7WNYXEWX76L2C5BH3JUW3RCATGUYKY7QQTRILBZIFWV",
        "CBOHAVUYKQD4C7FIVXEDJCVLUZYUO6RN3VIKEDOTIJGDDV3QN33Y4T4D",
    ),
    (
        "CD4ASKG2XVZRAUXSXPCGUSBIX4JOC2TNA2FDBAPUNJB7RSUG5YGRQRSF",
        "CBJQXWMAXJPBWQ75JW26HAI72I5D5GRHMKQNAZ2YHEDGFORXCVA3NFGJ",
    ),
    (
        "CDE57N6XTUPBKYYDGQMXX7E7SLNOLFY3JEQB4MULSMR2AKTSAENGX2HC",
        "CBWYOO6AFZ6RNOBDVDB4BMPJDUTN3L45SEKMMDOFNY4PWJNMELYJRUMC",
    ),
    (
        "CDRRLXLBXYVENKIUCCTFUT5WW6BW43NJMAPEMGU6VIU74LV6526OLS35",
        "CCKFOOBRTACG72QJ5FUAW72KB2PYYHYYB7QXBQSFHRS7F2YKF6JWIMKX",
    ),
    (
        "CDUX476HQ4JZBPLLVEVRIVXDRKAGYDNDL7OLBE5DD37JXOUFI2NACXJ5",
        "CB6DMPDOCSSIW4M5WX77FN5AR77Z765UBSMNJPJ4KDHVJ636OEE4JAN4",
    ),
];

#[test]
fn every_mainnet_deposit_resolves_like_share_id() {
    let Ok(path) = std::env::var("SHARE_TOKEN_CORPUS") else {
        eprintln!("SHARE_TOKEN_CORPUS unset — skipping (see module docs)");
        return;
    };
    let raw = std::fs::read_to_string(&path).expect("corpus readable");

    // Final winner per pool = the sighting from the NEWEST ledger, mirroring
    // how the RMT side table converges (version = sighting ledger).
    let mut final_token: BTreeMap<String, (i64, String)> = BTreeMap::new();
    let mut txs = 0usize;
    let mut sightings = 0usize;

    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let tx: CorpusTx = serde_json::from_str(line).expect("corpus row");
        txs += 1;
        let events: Vec<ExtractedEvent> = tx
            .events
            .iter()
            .map(|e| ExtractedEvent {
                transaction_hash: tx.tx.to_string(),
                event_type: ContractEventType::Contract,
                source: EventSource::TxLevel,
                contract_id: Some(e.contract.clone()),
                topics: serde_json::from_str(&e.topics).expect("topics json"),
                data: serde_json::from_str(&e.data).expect("data json"),
                event_index: e.idx,
                op_index: None,
                stage: None,
                ledger_sequence: tx.ledger as u32,
                created_at: 0,
            })
            .collect();

        for s in detect_share_tokens(&[(tx.tx.to_string(), events)]) {
            sightings += 1;
            let e = final_token
                .entry(s.pool)
                .or_insert((tx.ledger, s.token.clone()));
            if tx.ledger >= e.0 {
                *e = (tx.ledger, s.token);
            }
        }
    }

    eprintln!(
        "\n--- share-token corpus: {txs} deposit txs, {sightings} sightings, {} pools resolved ---",
        final_token.len()
    );

    // Ground truth first: the 16 on-chain confirmations must match exactly.
    let mut wrong = Vec::new();
    for (pool, want) in ON_CHAIN_TRUTH {
        match final_token.get(*pool) {
            Some((_, got)) if got == want => {}
            Some((_, got)) => wrong.push(format!("{pool}: got {got}, chain says {want}")),
            None => wrong.push(format!("{pool}: no sighting at all")),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of 16 on-chain-verified pools disagree:\n{}",
        wrong.len(),
        wrong.join("\n")
    );

    // Coverage: T6 measured 305 share-minting pools. The registry is live, so
    // newer pools may push this up — it must never come back DOWN.
    assert!(
        final_token.len() >= 305,
        "expected at least 305 share-minting pools, got {}",
        final_token.len()
    );
    assert!(txs > 80_000, "corpus looks truncated: {txs} txs");
}
