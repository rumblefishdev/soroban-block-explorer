use super::CONTRACT_METADATA;

/// Every read of the table goes through [`CONTRACT_METADATA`]: a second copy
/// is how three dedup styles, one of them wrong, grew in the first place.
#[test]
fn only_this_module_reads_the_metadata_table() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.to_string_lossy().contains("common/contract_metadata")
            {
                let text = std::fs::read_to_string(&path).expect("read source");
                for (n, line) in text.lines().enumerate() {
                    if line.contains("FROM soroban_contract_metadata")
                        || line.contains("JOIN soroban_contract_metadata")
                    {
                        hits.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "read it through CONTRACT_METADATA: {hits:#?}"
    );
}

/// A newest row without a field reads as unpublished, not as the older value.
/// Merges are stopped so both versions stay in separate parts — a merge would
/// hide the difference by keeping only the newer row.
#[tokio::test]
async fn newest_row_wins_nulls_included() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping contract metadata check");
        return;
    };
    const DB: &str = "api_test_0584_contract_metadata";
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    ch.query("SYSTEM STOP MERGES soroban_contract_metadata")
        .execute()
        .await
        .expect("stop metadata merges");
    // Separate inserts: one block would collapse the versions on write.
    for sql in [
        "INSERT INTO soroban_contract_metadata VALUES ('A', 'Old', 'OLD', 7, 1), ('B', 'Bee', 'B', 6, 1)",
        "INSERT INTO soroban_contract_metadata VALUES ('A', NULL, NULL, NULL, 2)",
    ] {
        ch.query(sql).execute().await.expect("seed rows");
    }

    type Row = (String, Option<String>, Option<String>, Option<u32>);
    let rows: Vec<Row> = ch
        .query(&format!(
            "SELECT contract_id, name, symbol, decimals FROM {CONTRACT_METADATA} ORDER BY contract_id"
        ))
        .fetch_all()
        .await
        .expect("metadata read runs");
    assert_eq!(
        rows,
        vec![
            ("A".into(), None, None, None),
            ("B".into(), Some("Bee".into()), Some("B".into()), Some(6)),
        ]
    );

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
