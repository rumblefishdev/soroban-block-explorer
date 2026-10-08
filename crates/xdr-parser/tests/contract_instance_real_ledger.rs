//! Contract instances from real mainnet ledgers (task 0620).
//!
//! The extractor reads instance entries straight from the transaction meta.
//! The JSON change path (`extract_ledger_entry_changes`), which every other
//! state table is built from, sees the same entries independently. On each
//! committed fixture ledger both must name the same instances in the same
//! order, and every stored entry must decode back to an instance of the
//! contract the JSON path names — so no instance is missed or misattributed.

use stellar_xdr::{
    ContractId, Hash, LedgerCloseMeta, LedgerEntryData, Limits, ReadXdr, ScAddress, ScVal,
    TransactionMeta,
};
use xdr_parser::contract_instance::extract_contract_instances;

const LEDGERS: [&[u8]; 4] = [
    include_bytes!("fixtures/ledgers/ledger_50500000.xdr.zst"),
    include_bytes!("fixtures/ledgers/ledger_58762517.xdr.zst"),
    include_bytes!("fixtures/ledgers/ledger_58762518.xdr.zst"),
    include_bytes!("fixtures/ledgers/ledger_64550000.xdr.zst"),
];

#[test]
fn instances_match_the_json_change_path_on_real_ledgers() {
    let mut total = 0;
    for raw in LEDGERS {
        let xdr = xdr_parser::decompress_zstd(raw).expect("zstd");
        let batch = xdr_parser::deserialize_batch(&xdr).expect("LedgerCloseMetaBatch");
        let meta = &batch.ledger_close_metas[0];
        let seq = match meta {
            LedgerCloseMeta::V0(v) => v.ledger_header.header.ledger_seq,
            LedgerCloseMeta::V1(v) => v.ledger_header.header.ledger_seq,
            LedgerCloseMeta::V2(v) => v.ledger_header.header.ledger_seq,
        };
        for tx_meta in tx_metas(meta) {
            let from_meta = extract_contract_instances(tx_meta, seq);
            let from_json: Vec<String> =
                xdr_parser::extract_ledger_entry_changes(tx_meta, "", seq, 0)
                    .into_iter()
                    .filter(|c| c.entry_type == "contract_data")
                    .filter(|c| {
                        matches!(c.change_type.as_str(), "created" | "updated" | "restored")
                    })
                    .filter_map(|c| {
                        let data = c.data?;
                        let key_type = data.get("key")?.get("type")?.as_str()?.to_string();
                        (key_type == "ledger_key_contract_instance")
                            .then(|| data.get("contract")?.as_str().map(str::to_string))
                            .flatten()
                    })
                    .collect();

            let decoded: Vec<String> = from_meta
                .iter()
                .map(|i| {
                    let data =
                        LedgerEntryData::from_xdr(&i.data_xdr, Limits::none()).expect("decodes");
                    let LedgerEntryData::ContractData(cd) = data else {
                        panic!("ledger {seq}: not contract data");
                    };
                    assert_eq!(cd.key, ScVal::LedgerKeyContractInstance, "ledger {seq}");
                    assert_eq!(
                        cd.contract,
                        ScAddress::Contract(ContractId(Hash(i.contract)))
                    );
                    cd.contract.to_string()
                })
                .collect();
            assert_eq!(decoded, from_json, "ledger {seq}");
            total += decoded.len();
        }
    }
    assert!(total > 0, "the fixtures must contain instance changes");
    eprintln!("instances checked: {total}");
}

fn tx_metas(meta: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
    }
}
