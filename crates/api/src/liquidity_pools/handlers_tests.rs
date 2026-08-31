//! Unit tests for [`super`] (the LP handlers) — tests live in their own
//! file per the repo's test-extraction convention (task 0374).
mod normalize_asset_code_tests {
    use super::super::normalize_asset_codes;

    #[test]
    fn none_passes_through() {
        assert!(normalize_asset_codes(None).is_empty());
    }

    #[test]
    fn empty_string_becomes_none() {
        assert!(normalize_asset_codes(Some(String::new())).is_empty());
        assert!(normalize_asset_codes(Some("   ".into())).is_empty());
    }

    #[test]
    fn lowercase_is_uppercased() {
        assert_eq!(normalize_asset_codes(Some("usdc".into())), ["USDC"]);
    }

    #[test]
    fn mixed_case_is_uppercased() {
        assert_eq!(normalize_asset_codes(Some("UsDc".into())), ["USDC"]);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(normalize_asset_codes(Some("  xlm  ".into())), ["XLM"]);
    }

    #[test]
    fn pair_splits_into_two_needles() {
        assert_eq!(
            normalize_asset_codes(Some("usdc/xlm".into())),
            ["USDC", "XLM"]
        );
    }

    #[test]
    fn pair_tolerates_spaces_around_the_slash() {
        assert_eq!(
            normalize_asset_codes(Some(" usdc / xlm ".into())),
            ["USDC", "XLM"]
        );
    }

    #[test]
    fn half_written_pair_keeps_the_written_half() {
        // Mid-typing state: the field debounces and fires on `USDC/`.
        assert_eq!(normalize_asset_codes(Some("USDC/".into())), ["USDC"]);
        assert_eq!(normalize_asset_codes(Some("/XLM".into())), ["XLM"]);
        assert!(normalize_asset_codes(Some("/".into())).is_empty());
    }

    #[test]
    fn third_code_stays_inside_the_second_needle() {
        // `splitn(2)` bounds the needle count. The remainder is not discarded —
        // it becomes a needle no asset code can contain, so the query returns
        // nothing rather than silently answering a narrower question.
        assert_eq!(
            normalize_asset_codes(Some("USDC/XLM/BTC".into())),
            ["USDC", "XLM/BTC"]
        );
        assert!(normalize_asset_codes(Some("/".repeat(5_000))).len() <= 2);
    }

    #[test]
    fn unicode_lower_uppercases_too() {
        // Stellar codes are ASCII-only in practice, but the normalizer
        // should not panic on UTF-8 — `String::to_uppercase` handles it.
        assert_eq!(normalize_asset_codes(Some("usdc🪙".into())), ["USDC🪙"]);
    }
}

mod map_pool_item_tests {
    use super::super::*;
    use crate::liquidity_pools::queries::PoolRow;

    fn base_row() -> PoolRow {
        PoolRow {
            pool_id_hex: "0".repeat(64),
            asset_a_type: 0,
            asset_a_type_name: Some("native".into()),
            asset_a_code: None,
            asset_a_issuer: None,
            asset_a_contract_id: None,
            asset_a_icon_url: None,
            asset_b_type: 1,
            asset_b_type_name: Some("credit_alphanum4".into()),
            asset_b_code: Some("USDC".into()),
            asset_b_issuer: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
            asset_b_contract_id: None,
            asset_b_icon_url: None,
            fee_bps: 30,
            fee_percent: "0.30".into(),
            created_at_ledger: 100,
            cursor_ledger: 100,
            participant_count: Some(0),
            latest_snapshot_ledger: None,
            reserve_a: None,
            reserve_b: None,
            total_shares: None,
            tvl: None,
            volume: None,
            fee_revenue: None,
            latest_snapshot_at: None,
            pool_kind: 0,
            deployment_id: 0,
            pool_type_raw: String::new(),
            legs: Vec::new(),
        }
    }

    #[test]
    fn native_leg_has_no_contract_id() {
        let item = map_pool_item(base_row(), None);
        let (a, b) = (item.asset_a.unwrap(), item.asset_b.unwrap());
        assert_eq!(a.asset_type, 0, "asset_a is native");
        assert_eq!(a.contract_id, None);
        assert_eq!(b.asset_type, 1, "asset_b is classic credit");
    }

    #[test]
    fn icon_url_propagates_per_leg() {
        // gap #5: each leg's icon_url threads from the row to the DTO leg,
        // independently. Native leg (no icon) stays None.
        let mut row = base_row();
        row.asset_b_icon_url = Some("https://cdn.example.test/icons/usdc.svg".into());
        let item = map_pool_item(row, None);
        assert_eq!(
            item.asset_a.unwrap().icon_url,
            None,
            "native leg has no icon"
        );
        assert_eq!(
            item.asset_b.unwrap().icon_url.as_deref(),
            Some("https://cdn.example.test/icons/usdc.svg")
        );
    }

    #[test]
    fn classic_credit_leg_surfaces_issuer_and_no_sac_mirror() {
        let item = map_pool_item(base_row(), None);
        let b = item.asset_b.unwrap();
        assert_eq!(b.asset_code.as_deref(), Some("USDC"));
        assert!(b.issuer.is_some());
        assert_eq!(
            b.contract_id, None,
            "no SAC mirror in `assets` → contract_id stays None"
        );
    }

    #[test]
    fn sac_mirror_contract_id_propagates_to_response() {
        let mut row = base_row();
        row.asset_b_contract_id =
            Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB".into());
        let item = map_pool_item(row, None);
        assert_eq!(
            item.asset_b.unwrap().contract_id.as_deref(),
            Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB")
        );
    }

    // ---- soroban rows (task 0374, step 17) ----

    fn soroban_row() -> PoolRow {
        let mut row = base_row();
        // Contract-address payload bytes; renders as `C...`, never `L...`.
        row.pool_id_hex = "1d3ab48b3b210df1a67f22809e7d84a533b93a583c76f728eec4bd6d68e33338".into();
        row.pool_kind = 1;
        row.deployment_id = 42;
        row.pool_type_raw = "constant".into();
        row.legs = vec![111, 222];
        row.participant_count = None;
        row
    }

    #[test]
    fn soroban_row_publishes_legs_and_hides_the_pair() {
        let view = SorobanView {
            legs: vec![
                PoolLegItem {
                    family: "native".into(),
                    asset_code: None,
                    issuer: None,
                    contract_id: Some(
                        "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".into(),
                    ),
                    symbol: None,
                    name: None,
                    decimals: Some(7),
                    reserve: Some("4112908590".into()),
                },
                PoolLegItem {
                    family: "soroban".into(),
                    asset_code: None,
                    issuer: None,
                    contract_id: Some(
                        "CC5PU23MKXHUFJKGG5FAUG7MFZX2KMWXPNZP26DDYW76VCB26UWMPEI6".into(),
                    ),
                    symbol: Some("AQUA".into()),
                    name: Some("Aquarius".into()),
                    decimals: Some(7),
                    reserve: Some("250000000000".into()),
                },
            ],
            protocol: Some("aquarius".into()),
        };
        let item = map_pool_item(soroban_row(), Some(view));
        assert!(item.pool_id.starts_with('C'), "soroban id is a C-strkey");
        assert_eq!(item.pool_kind, "soroban");
        assert_eq!(item.protocol.as_deref(), Some("aquarius"));
        assert_eq!(item.pool_type.as_deref(), Some("constant"));
        assert!(item.asset_a.is_none(), "pair columns must not surface");
        assert!(item.asset_b.is_none());
        assert_eq!(item.participant_count, None, "None ≠ 0 for soroban");
        let legs = item.legs.expect("legs published");
        assert_eq!(legs.len(), 2);
        assert_eq!(legs[1].reserve.as_deref(), Some("250000000000"));
    }

    #[test]
    fn classic_row_has_no_soroban_fields() {
        let item = map_pool_item(base_row(), None);
        assert!(item.pool_id.starts_with('L'), "classic id is an L-strkey");
        assert_eq!(item.pool_kind, "classic");
        assert_eq!(item.protocol, None);
        assert_eq!(item.pool_type, None, "empty pool_type_raw → absent field");
        assert!(item.legs.is_none());
    }

    /// An unverified-router pool (deployment shares Aquarius's code, admin
    /// roles disjoint) must stay UNLABELLED — the view carries no protocol
    /// and the item must not invent one.
    #[test]
    fn unlabelled_router_pool_has_null_protocol() {
        let view = SorobanView {
            legs: Vec::new(),
            protocol: None,
        };
        let item = map_pool_item(soroban_row(), Some(view));
        assert_eq!(item.protocol, None);
        assert_eq!(item.pool_kind, "soroban");
    }
}
