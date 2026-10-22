use super::*;

fn toml_with(code: &str, issuer: &str, image: Option<&str>, name: Option<&str>) -> Sep1TomlParsed {
    Sep1TomlParsed {
        currencies: vec![Sep1Currency {
            code: Some(code.to_owned()),
            issuer: Some(issuer.to_owned()),
            desc: None,
            image: image.map(str::to_owned),
            name: name.map(str::to_owned),
        }],
        documentation: None,
    }
}

fn entry<'a>(p: &'a Sep1TomlParsed, code: &str, issuer: &str) -> Option<&'a Sep1Currency> {
    find_currency(p, Some(code), Some(issuer))
}

#[test]
fn find_currency_matches_by_code_and_issuer() {
    let p = toml_with("USDC", "GA1", Some("https://x/u.png"), Some("USD Coin"));
    assert!(find_currency(&p, Some("USDC"), Some("GA1")).is_some());
    assert!(find_currency(&p, Some("EURC"), Some("GA1")).is_none());
    assert!(find_currency(&p, Some("USDC"), Some("GA2")).is_none());
    assert!(find_currency(&p, None, None).is_none());
}

#[test]
fn resolve_icon_returns_url_on_match() {
    let p = toml_with("USDC", "GA1", Some("https://example.com/u.png"), None);
    assert_eq!(
        resolve_icon(entry(&p, "USDC", "GA1")),
        "https://example.com/u.png"
    );
}

#[test]
fn resolve_icon_returns_sentinel_when_missing_or_empty() {
    let none = toml_with("USDC", "GA1", None, None);
    let empty = toml_with("USDC", "GA1", Some(""), None);
    assert_eq!(resolve_icon(entry(&none, "USDC", "GA1")), "");
    assert_eq!(resolve_icon(entry(&empty, "USDC", "GA1")), "");
    assert_eq!(resolve_icon(None), "");
}

#[test]
fn resolve_icon_rejects_unsafe_scheme() {
    let p = toml_with("USDC", "GA1", Some("javascript:alert(1)"), None);
    assert_eq!(resolve_icon(entry(&p, "USDC", "GA1")), "");
}

#[test]
fn resolve_icon_rejects_too_long() {
    let url = format!("https://example.com/{}", "x".repeat(MAX_ICON_URL_BYTES));
    let p = toml_with("USDC", "GA1", Some(&url), None);
    assert_eq!(resolve_icon(entry(&p, "USDC", "GA1")), "");
}

#[test]
fn resolve_name_skips_native_and_soroban() {
    let p = toml_with("USDC", "GA1", None, Some("USD Coin"));
    assert!(resolve_name(0, entry(&p, "USDC", "GA1")).is_none());
    assert!(resolve_name(3, entry(&p, "USDC", "GA1")).is_none());
}

#[test]
fn resolve_name_fills_for_classic_and_sac() {
    let p = toml_with("USDC", "GA1", None, Some("USD Coin"));
    for at in [1, 2] {
        assert_eq!(
            resolve_name(at, entry(&p, "USDC", "GA1")).as_deref(),
            Some("USD Coin")
        );
    }
}

#[test]
fn resolve_name_trims_whitespace() {
    let p = toml_with("USDC", "GA1", None, Some("  USD Coin  "));
    assert_eq!(
        resolve_name(1, entry(&p, "USDC", "GA1")).as_deref(),
        Some("USD Coin")
    );
}

#[test]
fn resolve_name_sentinel_when_missing_or_blank() {
    let none = toml_with("USDC", "GA1", None, None);
    let empty = toml_with("USDC", "GA1", None, Some(""));
    let ws = toml_with("USDC", "GA1", None, Some("   "));
    for p in [&none, &empty, &ws] {
        assert_eq!(
            resolve_name(1, entry(p, "USDC", "GA1")).as_deref(),
            Some("")
        );
    }
    assert_eq!(resolve_name(1, None).as_deref(), Some(""));
}

#[test]
fn resolve_name_sentinel_when_too_long() {
    let too_long = "x".repeat(MAX_NAME_BYTES + 1);
    let p = toml_with("USDC", "GA1", None, Some(&too_long));
    assert_eq!(
        resolve_name(1, entry(&p, "USDC", "GA1")).as_deref(),
        Some("")
    );
}

#[test]
fn sentinel_name_only_for_classic_and_sac() {
    assert!(sentinel_name(0).is_none());
    assert_eq!(sentinel_name(1).as_deref(), Some(""));
    assert_eq!(sentinel_name(2).as_deref(), Some(""));
    assert!(sentinel_name(3).is_none());
}

/// Smoke (task 0231 step 5): hit a REAL issuer `stellar.toml` over the
/// network and run the actual fetch + resolve path — catches real-world
/// TOML quirks the mocked unit tests can't. `#[ignore]` (manual / network,
/// not CI). Run:
/// `cargo test -p enrichment-shared smoke_real_sep1 -- --ignored --nocapture`
#[tokio::test]
#[ignore = "network: fetches a live issuer stellar.toml"]
async fn smoke_real_sep1_resolves_icon_and_name() {
    // USDC (Circle) — a stable, well-formed SEP-1 issuer.
    let home_domain = "centre.io";
    let code = "USDC";
    let issuer = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let fetcher = Sep1Fetcher::new().expect("build fetcher");
    let parsed = fetcher.fetch(home_domain).await.expect("fetch USDC toml");
    let (icon, name) = resolve_currency_outcome(1, Some(code), Some(issuer), &parsed);
    eprintln!("USDC → icon={icon:?} name={name:?}");
    assert!(!icon.is_empty(), "expected a real icon URL (got sentinel)");
    assert!(
        name.as_deref().is_some_and(|n| !n.is_empty()),
        "expected a real name (got sentinel/None)"
    );
}

/// CH-backed end-to-end smoke (task 0231 step 5): the full sep1 write path
/// against a live local ClickHouse — issuer `accounts` lookup → live TOML
/// fetch → resolve → INSERT → read back. Covers the **real** path (USDC) and
/// the **sentinel** path (issuer absent from `accounts`). `#[ignore]` (needs
/// CH + network). Run:
/// `CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
///  CLICKHOUSE_PASSWORD=clickhouse cargo test -p enrichment-shared \
///  smoke_ch_sep1 -- --ignored --nocapture`
#[tokio::test]
#[ignore = "needs live local ClickHouse + network (centre.io TOML)"]
async fn smoke_ch_sep1_real_and_sentinel() {
    #[derive(Row, Deserialize)]
    struct Readback {
        icon_url: Option<String>,
        name: Option<String>,
    }
    async fn read(client: &Client, k: &AssetKey) -> (Option<String>, Option<String>) {
        let r = client
            .query(
                "SELECT icon_url, name FROM asset_enrichment FINAL \
                 WHERE asset_type = ? AND asset_code = ? AND issuer_id = ? AND contract_id = ?",
            )
            .bind(k.asset_type)
            .bind(&k.asset_code)
            .bind(k.issuer_id)
            .bind(k.contract_id)
            .fetch_one::<Readback>()
            .await
            .expect("read asset_enrichment");
        (r.icon_url, r.name)
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    let fetcher = Sep1Fetcher::new().expect("build fetcher");

    // --- REAL: seed the USDC issuer account (home_domain → centre.io) ---
    let issuer = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let issuer_id = db_clickhouse::persist::ids::account_id(issuer);
    client
        .query(
            "INSERT INTO accounts \
             (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain) \
             VALUES (?, ?, 0, 0, 0, 'centre.io')",
        )
        .bind(issuer_id)
        .bind(issuer)
        .execute()
        .await
        .expect("seed issuer account");

    let usdc = AssetKey {
        asset_type: 1,
        asset_code: "USDC".into(),
        issuer_id,
        contract_id: 0,
    };
    enrich_asset_from_sep1(&client, usdc.clone(), &fetcher)
        .await
        .expect("enrich USDC");
    let (icon, name) = read(&client, &usdc).await;
    eprintln!("REAL USDC -> icon={icon:?} name={name:?}");
    assert!(
        icon.as_deref().is_some_and(|s| !s.is_empty()),
        "real path: non-empty icon"
    );
    assert!(
        name.as_deref().is_some_and(|s| !s.is_empty()),
        "real path: non-empty name"
    );

    // --- SENTINEL: a classic asset whose issuer is absent from `accounts` ---
    let ghost = AssetKey {
        asset_type: 1,
        asset_code: "GHOST".into(),
        issuer_id: 909_001,
        contract_id: 0,
    };
    enrich_asset_from_sep1(&client, ghost.clone(), &fetcher)
        .await
        .expect("enrich ghost");
    let (g_icon, g_name) = read(&client, &ghost).await;
    assert_eq!(g_icon.as_deref(), Some(""), "sentinel: icon = ''");
    assert_eq!(g_name.as_deref(), Some(""), "sentinel: name = ''");

    // --- cleanup (dev CH is otherwise empty) ---
    client
        .query("ALTER TABLE accounts DELETE WHERE id = ?")
        .bind(issuer_id)
        .execute()
        .await
        .expect("cleanup accounts");
    client
        .query(
            "ALTER TABLE asset_enrichment DELETE WHERE asset_code = 'USDC' OR asset_code = 'GHOST'",
        )
        .execute()
        .await
        .expect("cleanup asset_enrichment");
}

// ---- task 0655: the detail page's SEP-1 text is stored with icon and name ----

#[test]
fn resolve_detail_text_takes_desc_and_org_url_from_the_same_read() {
    let mut p = toml_with("USDC", "GA1", None, Some("USD Coin"));
    p.currencies[0].desc = Some("A fully reserved stablecoin".to_owned());
    p.documentation = Some(crate::sep1::dto::Sep1Documentation {
        org_url: Some("https://www.circle.com".to_owned()),
    });
    assert_eq!(
        resolve_detail_text(Some("USDC"), Some("GA1"), &p),
        (
            Some("A fully reserved stablecoin".to_owned()),
            Some("https://www.circle.com".to_owned())
        )
    );
    // No currency match: the organisation's page still applies, the description does not.
    assert_eq!(
        resolve_detail_text(Some("EURC"), Some("GA1"), &p),
        (None, Some("https://www.circle.com".to_owned()))
    );
    // No [DOCUMENTATION] table.
    p.documentation = None;
    assert_eq!(
        resolve_detail_text(Some("USDC"), Some("GA1"), &p),
        (Some("A fully reserved stablecoin".to_owned()), None)
    );
}
