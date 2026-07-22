//! Task 0392 — every read of the NFT fact tables must carry the visibility
//! predicate.
//!
//! ## Why a source-scanning test and not a database view
//!
//! `nfts` / `nft_ownership` hold NFT-shaped rows for **every** contract the
//! indexer could not rule out at ingest — the class of a `transfer` event is
//! undecidable from the event alone (an NFT and a fungible token emit the same
//! shape), and the contract's verdict can resolve after the row is written. So
//! the tables are the facts and `nfts::queries::NFT_VISIBLE` is the judgement,
//! applied at read time.
//!
//! That makes the predicate the *only* thing standing between an unclassified
//! contract and the public API — a forgotten `WHERE` shows fungible tokens as
//! NFTs, silently and plausibly. The alternative barrier (rename the table,
//! serve a same-named CH view with the filter baked in) was rejected: it
//! renames a live table the indexer writes to, hides the predicate from anyone
//! reading the query, and freezes `FINAL` for every caller — a knob worth up to
//! 19x read amplification (task 0420).
//!
//! So the barrier lives here instead: greppable, in-repo, and it names the fix
//! in its failure message.

use std::fs;
use std::path::{Path, PathBuf};

/// Fact tables whose rows are only conditionally visible. `nft_enrichment` is
/// deliberately absent — it is a side table joined onto already-filtered rows.
const GUARDED: [&str; 2] = ["FROM nfts", "FROM nft_ownership"];

/// The predicate must appear inside the SAME string literal as the `FROM` —
/// i.e. between it and the closing `"` of that SQL. A fixed character window
/// looks equivalent and is not: it reaches into the NEXT query in the file, so a
/// query that dropped its predicate passes on its neighbour's. That false
/// negative was observed, not theorised (the detail query was stripped and the
/// guard stayed green because `nft_exists` sat 400 chars below).
///
/// These SQL literals never contain an escaped `"` — ClickHouse string values
/// here are single-quoted — so the next `"` is reliably the end of the literal.
fn sql_literal_after(rest: &str) -> &str {
    match rest.find('"') {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// Escape hatch for a read that deliberately sees the unfiltered table — e.g. a
/// test asserting that a hidden row is physically present. Write it on the line
/// above the query and state why:
///
/// ```text
/// // nft-visibility-guard: counts physical rows, visibility is the assertion
/// ```
///
/// A waiver is not a loophole — it is the reason, in the diff, where a reviewer
/// sees it. `grep -rn "nft-visibility-guard:" crates/api` lists every one.
const WAIVER: &str = "nft-visibility-guard:";

#[test]
fn every_nft_table_read_carries_the_visibility_predicate() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();

    for file in rust_sources(&src) {
        let text = fs::read_to_string(&file).expect("read source");
        // Waivers live in comments, so collect them before comments are stripped.
        let waived: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains(WAIVER))
            .map(|(i, _)| i + 1)
            .collect();
        let code = strip_line_comments(&text);

        for needle in GUARDED {
            for (idx, _) in code.match_indices(needle) {
                // `FROM nfts_pending` etc. are a different table, not this one.
                let rest = &code[idx + needle.len()..];
                if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                if sql_literal_after(rest).contains("NFT_VISIBLE") {
                    continue;
                }
                // A waiver applies to the query it introduces: the 3 lines above
                // the `FROM` (macro call, opening quote, then the SQL).
                let line = code[..idx].lines().count();
                if waived
                    .iter()
                    .any(|w| line.saturating_sub(*w) <= 3 && *w <= line)
                {
                    continue;
                }
                offenders.push(format!(
                    "{}:{line} — `{needle}` with no NFT_VISIBLE predicate",
                    file.strip_prefix(&src).unwrap_or(&file).display()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "NFT fact tables read without the visibility predicate:\n  {}\n\n\
         `nfts` / `nft_ownership` contain rows for contracts that are not \
         (yet) classified as NFTs — the row is only public once its contract's \
         verdict resolves. Add `AND <alias>.contract_id IN {{NFT_VISIBLE}}` to \
         the query's WHERE (see crate::nfts::queries::NFT_VISIBLE, task 0392).",
        offenders.join("\n  ")
    );
}

/// The predicate hard-codes the discriminant because it is a SQL string, not
/// Rust. This is the link back to the enum that defines it.
#[test]
fn nft_visible_matches_the_contract_type_discriminant() {
    assert_eq!(
        domain::ContractType::Nft as i16,
        2,
        "NFT_VISIBLE filters `contract_type = 2` — update it with the enum"
    );
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

/// Drop `//` comments so prose about a query never reads as the query itself.
/// Line-based and deliberately naive — a `//` inside a string literal costs us
/// a truncated line in the scan, never a false pass (the SQL sits on its own
/// lines).
fn strip_line_comments(text: &str) -> String {
    text.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
