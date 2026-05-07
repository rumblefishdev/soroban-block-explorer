//! `soroban_contracts` — REMAP. FK `deployer_id → accounts(id)` is
//! remapped via `merge_remap.accounts`. Mirror of indexer upsert at
//! `write.rs:410-427`.
//!
//! **`search_vector` is GENERATED ALWAYS** (migration
//! `20260505130000_soroban_contracts_typed_name_column` recreated it
//! reading typed `name` instead of `metadata->>'name'`) — must be
//! omitted from the INSERT column list. If included, Postgres raises
//! `cannot insert a non-DEFAULT value into column "search_vector"`.
//!
//! Batched by source `id` (no clean ledger column on this table).
//! `wasm_uploaded_at_ledger`/`deployed_at_ledger` are both nullable —
//! using id keeps batching deterministic.

use sqlx::PgConnection;

use crate::batcher::{MergeStats, ledger_windowed};
use crate::error::MergeError;

pub async fn run(conn: &mut PgConnection) -> Result<MergeStats, MergeError> {
    ledger_windowed(
        conn,
        "soroban_contracts",
        "merge_source.soroban_contracts",
        "id",
        r#"
        WITH input AS (
            SELECT s.id AS src_id, s.contract_id, s.wasm_hash,
                   s.wasm_uploaded_at_ledger,
                   ra.target_id AS deployer_id_remapped,
                   s.deployed_at_ledger, s.contract_type, s.is_sac, s.name
              FROM merge_source.soroban_contracts s
              LEFT JOIN merge_remap.accounts ra ON ra.source_id = s.deployer_id
             WHERE s.id BETWEEN {lo} AND {hi}
        ),
        inserted AS (
            INSERT INTO soroban_contracts (
                contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id,
                deployed_at_ledger, contract_type, is_sac, name
            )
            SELECT contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id_remapped,
                   deployed_at_ledger, contract_type, is_sac, name
              FROM input
            ON CONFLICT (contract_id) DO UPDATE SET
                wasm_hash          = COALESCE(EXCLUDED.wasm_hash, soroban_contracts.wasm_hash),
                deployer_id        = COALESCE(EXCLUDED.deployer_id, soroban_contracts.deployer_id),
                deployed_at_ledger = COALESCE(EXCLUDED.deployed_at_ledger, soroban_contracts.deployed_at_ledger),
                contract_type      = COALESCE(EXCLUDED.contract_type, soroban_contracts.contract_type),
                is_sac             = soroban_contracts.is_sac OR EXCLUDED.is_sac,
                name               = COALESCE(EXCLUDED.name, soroban_contracts.name)
            RETURNING id, contract_id
        )
        INSERT INTO merge_remap.soroban_contracts (source_id, target_id)
        SELECT i.src_id, ins.id
          FROM input i
          JOIN inserted ins ON ins.contract_id = i.contract_id
        ON CONFLICT (source_id) DO UPDATE SET target_id = EXCLUDED.target_id
        "#,
    )
    .await
}
