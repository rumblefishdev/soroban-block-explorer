use xdr_parser::contract_instance::ExtractedContractInstance;

use super::contract_instance_rows;

fn instance(contract: u8, value: u8, ledger: u32) -> ExtractedContractInstance {
    ExtractedContractInstance {
        contract: [contract; 32],
        data_xdr: vec![value],
        ledger_sequence: ledger,
    }
}

#[test]
fn the_last_change_of_a_contract_in_a_ledger_wins() {
    // Contract 1 changes twice in one ledger (two transactions), contract 2 once.
    let rows = contract_instance_rows(&[
        instance(1, 10, 500),
        instance(2, 20, 500),
        instance(1, 11, 500),
    ]);

    let got: Vec<_> = rows
        .iter()
        .map(|r| (r.contract[0], r.data_xdr[0], r.ledger))
        .collect();
    assert_eq!(got, vec![(1, 11, 500), (2, 20, 500)]);
}
