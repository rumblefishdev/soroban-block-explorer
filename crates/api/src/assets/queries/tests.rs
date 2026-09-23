use super::*;

#[test]
fn union_keyset_arms_merges_both_arms_in_one_statement() {
    // task 0446: the two arms are independent reads; they must cost ONE round
    // trip, with the merge (order, cross-arm dedup, truncate) pushed into CH.
    let sql = union_keyset_arms("SELECT a", "SELECT b", "DESC", 21);
    assert!(sql.contains("(SELECT a) UNION ALL (SELECT b)"));
    assert!(sql.contains("ORDER BY ledger_sequence DESC, transaction_id DESC"));
    // Drops a transaction returned by BOTH arms — the old Rust `keys.dedup()`.
    assert!(sql.contains("LIMIT 1 BY ledger_sequence, transaction_id"));
    assert!(sql.ends_with("LIMIT 21"));
}

#[test]
fn asset_key_tuples_inlines_type_code_issuer_contract() {
    let keys = vec![
        AssetKeyChRow {
            asset_type: 1,
            asset_code: "USDC".to_string(),
            issuer_id: 42,
            contract_id: 0,
            id: 7,
        },
        AssetKeyChRow {
            asset_type: 0,
            asset_code: String::new(), // native — empty code
            issuer_id: 0,
            contract_id: 0,
            id: 9,
        },
    ];
    assert_eq!(asset_key_tuples(&keys), "(1,'USDC',42,0),(0,'',0,0)");
}

#[test]
fn dedup_consecutive_keeps_first_version_per_key_and_truncates() {
    // Both physical versions of an asset share one aggregate row, so they
    // stay adjacent under the holder walk as they were under the old one —
    // which is what lets a CONSECUTIVE dedup collapse them.
    let key = |code: &str, id: i64| AssetKeyChRow {
        asset_type: 1,
        asset_code: code.to_string(),
        issuer_id: 1,
        contract_id: 0,
        id,
    };
    // Two physical versions of AAA (contiguous in PK order), then BBB, CCC.
    let raw = vec![key("AAA", 1), key("AAA", 1), key("BBB", 2), key("CCC", 3)];
    let out = dedup_consecutive(raw, 2);
    // Collapses AAA's versions, then truncates to the page limit.
    assert_eq!(
        out.iter()
            .map(|k| k.asset_code.as_str())
            .collect::<Vec<_>>(),
        ["AAA", "BBB"]
    );
}

#[test]
fn asset_type_name_matches_pg_function() {
    assert_eq!(asset_type_name(0).as_deref(), Some("native"));
    assert_eq!(asset_type_name(1).as_deref(), Some("classic_credit"));
    // 2 (`sac`) retired — ADR 0051.
    assert_eq!(asset_type_name(2), None);
    assert_eq!(asset_type_name(3).as_deref(), Some("soroban"));
    assert_eq!(asset_type_name(99), None);
}

#[test]
fn list_sql_searches_name_and_symbol_when_code_present() {
    // task 0370: type-3 (Soroban-native) assets have an empty `asset_code`;
    // their name/symbol live in the joined contract metadata, so the list
    // search must match those columns too or they are unfindable by name.
    let params = ResolvedListParams {
        limit: 10,
        cursor: None,
        asset_type: None,
        asset_code: Some("solv".to_string()),
        sac_only: false,
    };
    let sql = build_list_seek_sql(&params, Direction::Next);
    assert!(sql.contains("toString(a.asset_code)"));
    assert!(sql.contains("coalesce(m.name, '')"));
    assert!(sql.contains("coalesce(m.symbol, '')"));
    // Classic enrichment names (ae.name) are intentionally NOT matched —
    // substring-matching them adds noise ("Opulent Insolvent" ~ "solv").
    assert!(!sql.contains("coalesce(ae.name, '')"));
    // 3 needle placeholders (the shared code test + m.name + m.symbol);
    // the LIMIT is inlined, so this MUST equal the 3×`.bind(code)` in
    // `fetch_list`. A drift here is a runtime-only failure.
    assert_eq!(sql.matches('?').count(), 3);
}

#[test]
fn native_is_matched_by_type_not_by_stored_code() {
    // Task 0470. Native XLM is stored with an EMPTY `asset_code`, so a bare
    // `positionCaseInsensitive(a.asset_code, 'XLM')` matched 6 404 credit
    // assets minted under a code containing "XLM" and missed the real one
    // (measured on production: the native row matched 0 times). Same defect
    // and same fix as the pools predicate — see the guard test beside it in
    // `common::pool_asset_codes`.
    //
    // Pinned on the SQL rather than on a result set because the CH-backed
    // tests only run with `CH_URL` set; this one runs everywhere.
    let params = ResolvedListParams {
        limit: 10,
        cursor: None,
        asset_type: None,
        asset_code: Some("XLM".to_string()),
        sac_only: false,
    };
    let sql = build_list_seek_sql(&params, Direction::Next);
    assert!(
        sql.contains(SHOWN),
        "the needle must be matched against the DISPLAYED code, so native \
         XLM is reachable; got: {sql}"
    );
    // The bare form is what made native unfindable — it must not come back.
    assert!(!sql.contains("positionCaseInsensitive(a.asset_code, ?)"));
}

#[test]
fn the_walk_is_by_holders_so_the_list_opens_on_what_people_hold() {
    // Task 0485 needed native XLM to OPEN the list and got it from the
    // alphabet (empty code = the minimum of the old key). It still opens
    // the list, now because it has the most holders — so the outcome no
    // longer rests on a property of the storage key.
    let mut params = ResolvedListParams {
        limit: 10,
        cursor: Some(AssetKeyCursor {
            holder_rank: 684_170,
            id: 7,
        }),
        asset_type: None,
        asset_code: Some("xlm".to_string()),
        sac_only: false,
    };
    for sql in [build_list_seek_sql(&params, Direction::Next), {
        params.asset_code = None;
        build_list_seek_sql(&params, Direction::Next)
    }] {
        assert!(
            sql.contains("ORDER BY holder_rank DESC, a.id DESC"),
            "{sql}"
        );
        // The seek projects the rank it orders by, so the cursor is built
        // from this query's snapshot, not from the hydration read (task 0559).
        assert!(
            sql.contains("coalesce(ba.holder_count, -1) AS holder_rank"),
            "{sql}"
        );
        // LEFT: an asset with no aggregate row still appears, ordered
        // last. An inner join would delete it from the list.
        assert!(
            sql.contains("LEFT JOIN balance_aggregates ba ON ba.asset_id = a.id"),
            "{sql}"
        );
        // The fold has to be identical in the ORDER BY, the projection and
        // the cursor comparator, or a page boundary skips or repeats rows.
        // Twice: the projection and the cursor comparator. The ORDER BY
        // reuses the `holder_rank` alias rather than a third copy.
        assert_eq!(
            sql.matches("coalesce(ba.holder_count, -1)").count(),
            2,
            "{sql}"
        );
        // The comparator must point INTO the walk, or the cursor pages
        // backwards on every "next".
        assert!(sql.contains(") < (?, ?)"), "{sql}");
        // No relevance machinery on this surface, by decision (0485):
        // ranking it means carrying the rank in the cursor.
        assert!(!sql.contains("rank_tier"), "{sql}");
    }
}

#[test]
fn list_sql_has_no_search_predicate_without_a_term() {
    let params = ResolvedListParams {
        limit: 10,
        cursor: None,
        asset_type: None,
        asset_code: None,
        sac_only: false,
    };
    let sql = build_list_seek_sql(&params, Direction::Next);
    assert!(!sql.contains("positionCaseInsensitive"));
    // Neither SEARCH join is paid for. The aggregate join is always
    // present (it carries the browse order), hence asserting the two by
    // name rather than that "JOIN" is absent.
    assert!(!sql.contains("soroban_contracts sc"), "{sql}");
    assert!(!sql.contains("soroban_contract_metadata"), "{sql}");
    assert!(sql.contains("LEFT JOIN balance_aggregates"), "{sql}");
    // No cursor and no needle → nothing to bind (the LIMIT is inlined).
    assert_eq!(sql.matches('?').count(), 0);
}
