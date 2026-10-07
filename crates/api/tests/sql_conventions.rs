//! SQL conventions the API queries must keep. Pure text checks, no ClickHouse.
//!
//! A `ReplacingMergeTree` table holds unmerged duplicate rows until a merge
//! collapses them, so every read dedups it. ClickHouse up to 26.6 applies the
//! `FINAL` of the left-most table to every table joined to it; 26.7 stops
//! doing that (task 0602). A query written as
//! `FROM transactions t FINAL JOIN ledgers l` then returns each transaction
//! twice whenever its ledger row has an unmerged duplicate — silently. So a
//! replacing table joined next to a `FINAL` read carries its own `FINAL`.

use std::fs;
use std::path::{Path, PathBuf};

use db_clickhouse::INIT_SQL;

/// Names of the `ReplacingMergeTree` tables in `init.sql`.
fn replacing_tables() -> Vec<String> {
    let stripped: String = INIT_SQL
        .lines()
        .map(|line| line.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    stripped
        .split(';')
        .filter_map(|stmt| {
            let start = stmt.find("CREATE TABLE IF NOT EXISTS ")?;
            let rest = &stmt[start + "CREATE TABLE IF NOT EXISTS ".len()..];
            let name = rest.split_whitespace().next()?;
            stmt.contains("ENGINE = ReplacingMergeTree")
                .then(|| name.to_string())
        })
        .collect()
}

/// Every `.rs` file under `dir`, test files included — test SQL follows the
/// same rule.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The string literals of a Rust source file, comments skipped. Handles plain
/// and raw strings, char literals and lifetimes — enough for our SQL, which
/// is always a plain or raw string (often a `format!` template).
fn string_literals(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            'r' if matches!(chars.get(i + 1), Some('"') | Some('#')) => {
                let mut hashes = 0;
                let mut j = i + 1;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) != Some(&'"') {
                    i += 1;
                    continue;
                }
                let body_start = j + 1;
                let mut k = body_start;
                while k < chars.len() {
                    if chars[k] == '"' && (1..=hashes).all(|h| chars.get(k + h) == Some(&'#')) {
                        break;
                    }
                    k += 1;
                }
                out.push(chars[body_start..k.min(chars.len())].iter().collect());
                i = k + 1 + hashes;
            }
            '"' => {
                let mut k = i + 1;
                let mut body = String::new();
                while k < chars.len() && chars[k] != '"' {
                    if chars[k] == '\\' {
                        k += 1;
                    }
                    if let Some(&c) = chars.get(k) {
                        body.push(c);
                    }
                    k += 1;
                }
                out.push(body);
                i = k + 1;
            }
            '\'' => {
                // A char literal ('x', '\n', '"') is skipped whole; a lifetime
                // ('a) is one character.
                if chars.get(i + 1) == Some(&'\\') {
                    i += 2;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// `JOIN <replacing table> [AS] [alias]` without its own `FINAL`, in a query
/// whose FROM reads a replacing table with `FINAL`.
fn joins_relying_on_final_propagation(sql: &str, replacing: &[String]) -> Vec<String> {
    let tokens: Vec<&str> = sql.split_whitespace().filter(|t| *t != "\\").collect();
    let is_replacing = |t: &str| replacing.iter().any(|r| r == t);
    let final_within =
        |from: usize, span: usize| tokens[from..tokens.len().min(from + span)].contains(&"FINAL");

    let reads_with_final = tokens.windows(2).enumerate().any(|(i, w)| {
        w[0] == "FROM"
            && is_replacing(w[1])
            && final_within(i + 2, 2)
            && !tokens[i + 2..tokens.len().min(i + 4)].contains(&"ON")
    });
    if !reads_with_final {
        return Vec::new();
    }

    let mut found = Vec::new();
    for (i, w) in tokens.windows(2).enumerate() {
        if !(w[0] == "JOIN" && is_replacing(w[1])) {
            continue;
        }
        // `JOIN t FINAL`, `JOIN t x FINAL`, `JOIN t AS x FINAL`.
        let mut j = i + 2;
        if tokens.get(j) == Some(&"AS") {
            j += 1;
        }
        let has_own_final = tokens.get(j) == Some(&"FINAL")
            || (tokens.get(j) != Some(&"ON") && tokens.get(j + 1) == Some(&"FINAL"));
        if !has_own_final {
            found.push(tokens[i..tokens.len().min(i + 6)].join(" "));
        }
    }
    found
}

#[test]
fn a_replacing_table_joined_to_a_final_read_carries_its_own_final() {
    let replacing = replacing_tables();
    assert!(
        replacing.len() > 20,
        "parser found only {} replacing tables",
        replacing.len()
    );
    for name in ["ledgers", "transactions", "soroban_contracts"] {
        assert!(replacing.iter().any(|r| r == name), "`{name}` not found");
    }

    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 50, "found only {} source files", files.len());

    let mut violations = Vec::new();
    for file in &files {
        let src = fs::read_to_string(file).unwrap();
        for sql in string_literals(&src) {
            for join in joins_relying_on_final_propagation(&sql, &replacing) {
                violations.push(format!("{}: {join}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "A replacing table is joined to a `FINAL` read without its own `FINAL`. \
         It is deduped today only because ClickHouse up to 26.6 propagates the \
         left-most table's `FINAL`; 26.7 stops, and the join then repeats rows \
         (task 0602). Add `FINAL` to the joined table:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_check_catches_a_bare_join_and_accepts_its_own_final() {
    let replacing = vec!["transactions".to_string(), "ledgers".to_string()];
    let bare = "SELECT 1 FROM transactions t FINAL \
                INNER JOIN ledgers l ON l.sequence = t.ledger_sequence";
    let own = "SELECT 1 FROM transactions t FINAL \
               INNER JOIN ledgers l FINAL ON l.sequence = t.ledger_sequence";
    let own_as = "SELECT 1 FROM transactions AS t FINAL \
                  JOIN ledgers AS l FINAL ON l.sequence = t.ledger_sequence";
    let no_final_left = "SELECT 1 FROM transactions t \
                         JOIN ledgers l ON l.sequence = t.ledger_sequence";
    assert_eq!(
        joins_relying_on_final_propagation(bare, &replacing).len(),
        1
    );
    assert!(joins_relying_on_final_propagation(own, &replacing).is_empty());
    assert!(joins_relying_on_final_propagation(own_as, &replacing).is_empty());
    assert!(joins_relying_on_final_propagation(no_final_left, &replacing).is_empty());
}

/// `FROM` / `JOIN soroban_contract_metadata` in a query: a read of the table
/// that bypasses `CONTRACT_METADATA`. Writes (`INSERT INTO`) do not count.
fn direct_metadata_reads(sql: &str) -> usize {
    let tokens: Vec<&str> = sql.split_whitespace().filter(|t| *t != "\\").collect();
    tokens
        .windows(2)
        .filter(|pair| {
            let keyword = pair[0].to_uppercase();
            let table = pair[1].trim_start_matches("default.").trim_end_matches(')');
            (keyword == "FROM" || keyword == "JOIN") && table == "soroban_contract_metadata"
        })
        .count()
}

/// Contract metadata is read in one place, `common/contract_metadata.rs`, so
/// every endpoint shows the same name, symbol and decimals for a contract —
/// the newest row's, a missing field included.
#[test]
fn contract_metadata_is_read_only_through_its_one_definition() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut violations = Vec::new();
    for file in &files {
        if file.ends_with("common/contract_metadata.rs") {
            continue;
        }
        let src = fs::read_to_string(file).unwrap();
        for sql in string_literals(&src) {
            if direct_metadata_reads(&sql) > 0 {
                violations.push(file.display().to_string());
            }
        }
    }
    assert!(
        violations.is_empty(),
        "Read `soroban_contract_metadata` through `CONTRACT_METADATA` \
         (`common/contract_metadata.rs`):\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_metadata_check_catches_every_spelling_of_a_direct_read() {
    assert_eq!(
        direct_metadata_reads("SELECT name FROM soroban_contract_metadata FINAL"),
        1
    );
    assert_eq!(
        direct_metadata_reads("LEFT JOIN default.soroban_contract_metadata m ON 1"),
        1
    );
    assert_eq!(
        direct_metadata_reads("select name from\n     soroban_contract_metadata)"),
        1
    );
    assert_eq!(
        direct_metadata_reads("INSERT INTO soroban_contract_metadata VALUES (1)"),
        0
    );
    assert_eq!(
        direct_metadata_reads("SELECT name FROM {CONTRACT_METADATA} m"),
        0
    );
}
