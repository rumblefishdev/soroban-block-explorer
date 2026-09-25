use super::*;

const EMPTY: [&str; 2] = ["0", "0"];

fn raw(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn shares(raw: &str, decimals: Option<u32>) -> Option<(String, Option<u32>)> {
    Some((raw.to_string(), decimals))
}

/// A classic snapshot wins, at the protocol's 7 decimals.
#[test]
fn a_snapshot_value_wins() {
    assert_eq!(
        pool_total_shares(
            Some("7506999160".into()),
            Some("9516607233561"),
            Some(18),
            "",
            &[]
        ),
        shares("7506999160", Some(7))
    );
}

#[test]
fn a_soroban_pool_reads_its_instance_state() {
    let holding = raw(&["10", "20"]);
    assert_eq!(
        pool_total_shares(None, Some("252647541418"), Some(7), "constant", &holding),
        shares("252647541418", Some(7))
    );
    // The share token publishes no decimals: the raw value with no scale, which
    // the client renders as "—" — never a guessed 7.
    assert_eq!(
        pool_total_shares(None, Some("252647541418"), None, "constant", &holding),
        shares("252647541418", None)
    );
    assert_eq!(pool_total_shares(None, None, Some(7), "", &holding), None);
}

/// A stored 0 is a number only when it is a measurement (task 0374, decision 96 A: the zero-shares rule).
#[test]
fn a_zero_is_shown_only_when_measured() {
    let empty = raw(&EMPTY);
    let holding = raw(&["10100000000", "0"]);
    let zero = |ptr: &str, res: &[String]| pool_total_shares(None, Some("0"), Some(7), ptr, res);
    // Pair-factory: the family with no type marker always stores the key.
    assert_eq!(zero("", &holding), shares("0", Some(7)));
    // An empty router pool has nothing outstanding, whatever its storage says.
    assert_eq!(zero("constant", &empty), shares("0", Some(7)));
    // A router pool holding reserves with a 0: the key was absent — unknown.
    assert_eq!(zero("constant", &holding), None);
    assert_eq!(zero("concentrated", &holding), None);
    // No reserve row at all is not evidence of an empty pool.
    assert_eq!(zero("stable", &[]), None);
}
