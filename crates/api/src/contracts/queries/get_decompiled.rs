//! `GET /v1/contracts/:id/decompiled` — the program bytes the Code tab
//! decompiles, read from `wasm_programs.code` (task 0620).

use clickhouse::Row;
use serde::Deserialize;

#[derive(Debug, Row, Deserialize)]
struct ProgramCodeRow {
    #[serde(with = "serde_bytes")]
    code: Vec<u8>,
}

/// The stored bytes of the program with this hash, or `None` when they are
/// not indexed yet (the program predates the indexer storing bytes and the
/// one-off fill has not reached it).
///
/// No FINAL: a program's bytes are fixed by its hash, so every duplicate row
/// carries the same `code` and the first one will do.
pub async fn fetch_program_code(
    client: &clickhouse::Client,
    wasm_hash_hex: &str,
) -> Result<Option<Vec<u8>>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT code FROM wasm_programs \
             WHERE wasm_hash = unhex(?) AND code != '' \
             LIMIT 1",
        )
        .bind(wasm_hash_hex)
        .fetch_optional::<ProgramCodeRow>()
        .await?;
    Ok(row.map(|r| r.code))
}
