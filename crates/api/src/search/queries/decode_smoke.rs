//! Live-CH decode smoke for the search read path. The curl `FORMAT` box smokes
//! do NOT exercise the clickhouse-rs RowBinary decoder, so a wire-type↔struct
//! mismatch (e.g. a Nullable column decoded into a non-Option field, or a
//! positional reorder) passes a curl check yet 500s the live endpoint. This
//! decodes rows a real CH produced for every bucket's Row struct.
//!
//! **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green.
//! Run against a reachable CH (local replica or SSH tunnel):
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib search::queries::decode_smoke -- --nocapture
//! ```

use super::super::classifier;
use super::*;

#[derive(Debug, Row, Deserialize)]
struct HashHexRow {
    hash_hex: String,
}

fn client() -> Option<clickhouse::Client> {
    let url = std::env::var("CH_URL").ok()?;
    let mut c = clickhouse::Client::default().with_url(url);
    if let Ok(u) = std::env::var("CH_USER") {
        c = c.with_user(u);
    }
    if let Ok(p) = std::env::var("CH_PASSWORD") {
        c = c.with_password(p);
    }
    if let Ok(d) = std::env::var("CH_DATABASE") {
        c = c.with_database(d);
    }
    Some(c)
}

/// Every search bucket's Row struct must decode the rows a real CH emits.
/// Text / prefix modes exercise account / contract / asset / nft on any
/// populated CH; a bootstrapped real hash exercises transaction + pool.
#[tokio::test]
async fn search_ch_rows_decode() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping search CH decode smoke");
        return;
    };
    let all = IncludeFlags::all();

    // Text mode → contract(name) + asset + nft. Prefix mode → account +
    // contract(prefix) + asset + nft. Both decode-exercise their structs
    // regardless of whether the corpus yields rows.
    for q in ["a", "GA", "CA"] {
        fetch_search(&ch, q, &classifier::classify(q), &all, 5)
            .await
            .unwrap_or_else(|e| panic!("search decode failed for q={q:?}: {e}"));
    }

    // Bootstrap a real tx hash so the transaction + pool buckets decode.
    let boot = ch
        .query("SELECT lower(hex(hash)) AS hash_hex FROM transactions LIMIT 1")
        .fetch_optional::<HashHexRow>()
        .await
        .expect("bootstrap hash query must run");
    let Some(boot) = boot else {
        eprintln!("transactions empty — text-mode decode ok, skipping hash mode");
        return;
    };
    fetch_search(
        &ch,
        &boot.hash_hex,
        &classifier::classify(&boot.hash_hex),
        &all,
        5,
    )
    .await
    .expect("transaction/pool bucket rows must decode");

    // A fee-bump's inner hash finds its transaction too, as the transaction
    // page already did.
    let inner = ch
        .query(
            "SELECT lower(hex(assumeNotNull(inner_tx_hash))) AS hash_hex FROM transactions \
             WHERE inner_tx_hash IS NOT NULL LIMIT 1",
        )
        .fetch_optional::<HashHexRow>()
        .await
        .expect("inner hash query must run");
    if let Some(inner) = inner {
        let hits = fetch_search(
            &ch,
            &inner.hash_hex,
            &classifier::classify(&inner.hash_hex),
            &all,
            5,
        )
        .await
        .expect("inner-hash search must decode");
        assert!(
            hits.iter().any(|(bucket, _)| bucket == "transaction"),
            "an inner hash must find its fee-bump transaction"
        );
    }
}

/// Task 0485. The tier ranking is only visible in the ORDER of the rows,
/// so the SQL-shape tests cannot see it — this runs the real read and
/// looks at the first asset hit. It also exercises the statement's 7
/// placeholders against the 7 `.bind(q)` calls; a mismatch is a runtime
/// failure no offline test reaches.
#[tokio::test]
async fn native_xlm_is_the_first_asset_hit_for_xlm() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping asset ranking smoke");
        return;
    };
    let has_native: u64 = ch
        .query("SELECT count() FROM assets WHERE asset_type = 0")
        .fetch_one()
        .await
        .expect("native probe must run");
    if has_native == 0 {
        eprintln!("no native row in this CH — ranking smoke not exercised");
        return;
    }

    let all = IncludeFlags::all();
    let hits = fetch_search(&ch, "xlm", &classifier::classify("xlm"), &all, 20)
        .await
        .expect("ranked asset search decodes");

    let first = hits
        .iter()
        .find(|(bucket, _)| bucket == "asset")
        .map(|(_, hit)| hit)
        .expect("a corpus with native XLM must yield an asset hit for `xlm`");
    assert_eq!(
        first.label, "native",
        "`xlm` answered with {:?} first — before the ranking this bucket \
         returned whichever look-alike codes the scan reached first and \
         native XLM never made the page",
        first.identifier
    );
}

/// A needle longer than a Stellar asset code cannot match a pool, so the
/// scan must not run at all. Guards the gate that keeps every account- and
/// contract-shaped search off the pools table (task 0470 review).
#[tokio::test]
async fn a_strkey_shaped_query_never_scans_the_pools_table() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping pools shape-gate smoke");
        return;
    };
    let all = IncludeFlags::all();

    // 56 characters: an account StrKey. Classified as a prefix, not a
    // hash, so before the gate this fell through to the code arm.
    let q = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let hits = fetch_search(&ch, q, &classifier::classify(q), &all, 20)
        .await
        .expect("search decodes");
    assert!(
        !hits.iter().any(|(bucket, _)| bucket == "pool"),
        "a StrKey-shaped query must not reach the pools bucket"
    );
}

/// Task 0470: a non-hash query used to match no pool at all, so an asset
/// code returned zero here while `/v1/liquidity-pools` returned dozens —
/// which read as the 0440 fix not working.
///
/// This does NOT re-test the matching rule: both surfaces call
/// `common::pool_asset_codes::asset_codes_predicate`, so they cannot
/// disagree, and that module's unit tests pin the pair semantics and the
/// native arm. What is only testable against a real ClickHouse is the rest
/// of this arm — that the grouped subquery parses, that `PoolRow` decodes
/// it, and that a plain asset code reaches pools at all.
#[tokio::test]
async fn search_matches_pools_by_asset_code() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping search pool asset-code smoke");
        return;
    };
    let all = IncludeFlags::all();

    // `XLM` is the needle that separates a correct predicate from a
    // plausible one: native legs carry an empty stored code, so a column
    // match without the `if(type = 0, …)` arm returns credit assets minted
    // under the code `XLM` and misses every real native pool.
    for needle in ["XLM", "USD"] {
        let hits = fetch_search(&ch, needle, &classifier::classify(needle), &all, 20)
            .await
            .expect("search decodes");
        let pools: Vec<&SearchHit> = hits
            .iter()
            .filter(|(bucket, _)| bucket == "pool")
            .map(|(_, hit)| hit)
            .collect();

        assert!(
            !pools.is_empty(),
            "search returned no pool for {needle:?} — the asset-code arm regressed to id-only"
        );
        for hit in pools {
            assert!(
                hit.label.to_uppercase().contains(needle),
                "pool {} labelled {:?} matches neither leg of {needle:?} — filter not applied",
                hit.identifier,
                hit.label,
            );
            assert!(
                hit.identifier.starts_with('L'),
                "pool identifier {:?} is not an L-strkey",
                hit.identifier,
            );
        }
    }
}

/// Task 0485: the canonical `CODE:ISSUER` (and our own `CODE-ISSUER` route
/// token) used to classify as nothing, so the asset arm hunted a 60+
/// character needle through <=12 character codes — provably empty — and the
/// most precise query a user can type answered with a blank page.
///
/// Only testable against a real corpus: what is asserted is which row
/// survives, which no fixture-free unit test can observe. The target is a
/// code carried by SEVERAL assets, addressed by the one the scan reaches
/// LAST, so a hit cannot come from the substring arm returning the first row
/// it happened to touch.
#[tokio::test]
async fn code_issuer_resolves_to_exactly_that_asset() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping CODE:ISSUER smoke");
        return;
    };

    #[derive(Debug, Row, Deserialize)]
    struct CodeRow {
        asset_code: String,
    }

    let probe = ch
        .query(
            "SELECT toString(a.asset_code) AS asset_code \
             FROM assets a FINAL \
             WHERE length(a.asset_code) > 0 AND a.issuer_id != 0 \
             GROUP BY a.asset_code \
             HAVING count() > 1 \
             LIMIT 1",
        )
        .fetch_optional::<CodeRow>()
        .await
        .expect("corpus probe must run");
    let Some(CodeRow { asset_code: code }) = probe else {
        eprintln!("no asset code shared by two issuers — skipping");
        return;
    };

    let same_code = ch
        .query(
            "SELECT a.asset_type AS asset_type, \
                    nullIf(a.asset_code, '') AS asset_code, \
                    nullIf(sc.contract_id, '') AS contract_strkey, \
                    a.issuer_id AS issuer_id \
             FROM assets a FINAL \
             LEFT JOIN ( \
                 SELECT id, any(contract_id) AS contract_id \
                 FROM soroban_contracts GROUP BY id \
             ) sc ON sc.id = a.contract_id \
             WHERE lower(toString(a.asset_code)) = lower(?) \
             LIMIT 16",
        )
        .bind(&code)
        .fetch_all::<AssetPhase1Row>()
        .await
        .expect("same-code query must run");
    let target = same_code
        .last()
        .unwrap_or_else(|| panic!("no asset row for probed code {code:?}"));

    let Some(issuer) = ch
        .query(
            "SELECT id AS id, account_id AS account_id \
             FROM accounts WHERE id = ? LIMIT 1 BY id",
        )
        .bind(target.issuer_id)
        .fetch_optional::<IssuerRow>()
        .await
        .expect("issuer resolve must run")
        .map(|r| r.account_id)
    else {
        eprintln!("probed asset has no resolvable issuer — skipping");
        return;
    };

    let want = asset_route_token(
        target.contract_strkey.as_deref(),
        target.asset_code.as_deref(),
        Some(issuer.as_str()),
        target.asset_type,
    );

    // Both separators: `:` is the canonical SEP / SDK form, `-` is what our
    // own `/assets/:id` routes emit and users paste back.
    for q in [format!("{code}:{issuer}"), format!("{code}-{issuer}")] {
        let hits = fetch_search(&ch, &q, &classifier::classify(&q), &IncludeFlags::all(), 10)
            .await
            .unwrap_or_else(|e| panic!("search failed for {q:?}: {e}"));
        let assets: Vec<&SearchHit> = hits
            .iter()
            .filter(|(bucket, _)| bucket == "asset")
            .map(|(_, hit)| hit)
            .collect();

        assert_eq!(
            assets.len(),
            1,
            "{q:?} must resolve to exactly one asset, got {assets:?}"
        );
        assert_eq!(
            assets[0].route_token, want,
            "{q:?} resolved to the wrong asset",
        );
    }
}
