# G — Parser-output → ClickHouse column coverage mapping

> Cross-check artefact for [task 0206](../README.md). For every field on every
> `Extracted*` type emitted by `crates/xdr-parser/src/types.rs`, this doc names
> the CH target column from `crates/db-clickhouse/schema/init.sql` (task 0204)
> — or explicitly marks the field as "consumed by staging, not directly
> stored" or "not stored on either side, matches PG behaviour".
>
> Authoritative content anchor: the PG persist path
> (`crates/indexer/src/handler/persist/{staging,write}.rs`). The CH writer
> must read every parser field PG reads, with the documented divergence:
> `soroban_events` is unfolded per ADR 0044 §Decision §4a (one CH row per
> `ExtractedEvent`, instead of folding into per-trio
> `soroban_events_appearances`).

Legend in the **CH target** column:

- `T.col` — stored verbatim in the named CH `init.sql` table.column.
- _staged_ — consumed by writer staging (StrKey universe, dedup keys,
  participant union, JSON unpack) and surfaces indirectly through other
  columns (`*_id` FKs, typed op-identity columns, etc.).
- _N/A — matches PG_ — neither PG nor CH stores the field; documented as
  derivative data that the API re-extracts from XDR at read time.

Every account / contract `Int64` FK in CH is the **same**
`cityhash64_strkey(StrKey)` value used to derive `accounts.id` and
`soroban_contracts.id` respectively (see [§Surrogate ID derivation](#surrogate-id-derivation)).

---

## ExtractedLedger → `ledgers`

| Field               | CH target                   | Notes                                          |
| ------------------- | --------------------------- | ---------------------------------------------- |
| `sequence`          | `ledgers.sequence`          | `u32 → i64`. Used as partition key everywhere. |
| `hash`              | `ledgers.hash`              | hex decoded → `FixedString(32)`.               |
| `closed_at`         | `ledgers.closed_at`         | Unix seconds → `DateTime64(3, 'UTC')`.         |
| `protocol_version`  | `ledgers.protocol_version`  | `u32 → i32`.                                   |
| `transaction_count` | `ledgers.transaction_count` | `u32 → i32`.                                   |
| `base_fee`          | `ledgers.base_fee`          | `u32 → i64`.                                   |

`ledgers` is the **partition commit marker**: the writer holds its row(s)
in a `Vec<LedgerRow>` until every other table's `Insert::end()` ack'd at
partition close, then opens the final `Insert<LedgerRow>` and ends it.
Mid-partition failure ⇒ no `ledgers` rows ⇒ resume re-does the whole
partition.

## ExtractedTransaction → `transactions` + `transaction_hash_index`

| Field             | CH target                                                                 | Notes                                                                             |
| ----------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `hash`            | `transactions.hash`, `transaction_hash_index.hash`                        | hex → `FixedString(32)`. Also derives `transactions.id = cityhash64(hash_bytes)`. |
| `inner_tx_hash`   | `transactions.inner_tx_hash`                                              | hex → `Nullable(FixedString(32))`. Fee-bump only.                                 |
| `ledger_sequence` | `transactions.ledger_sequence` + `transaction_hash_index.ledger_sequence` | `u32 → i64`.                                                                      |
| `source_account`  | `transactions.source_id`                                                  | `cityhash64_strkey(source_account)`. Also fills participant + accounts universe.  |
| `fee_charged`     | `transactions.fee_charged`                                                | `i64` direct.                                                                     |
| `successful`      | `transactions.successful`                                                 | bool direct.                                                                      |
| `result_code`     | _N/A — matches PG_                                                        | PG drops it; CH has no column either.                                             |
| `envelope_xdr`    | _N/A — matches PG_                                                        | PG defers to S3 archive (ADR 0029).                                               |
| `result_xdr`      | _N/A — matches PG_                                                        | Same.                                                                             |
| `result_meta_xdr` | _N/A — matches PG_                                                        | Same.                                                                             |
| `operation_tree`  | _N/A — matches PG_                                                        | API re-extracts at read time.                                                     |
| `memo_type`       | _N/A — matches PG_                                                        | PG schema dropped memo columns post-ADR 0027 cleanups; CH ditto.                  |
| `memo`            | _N/A — matches PG_                                                        | Same.                                                                             |
| `created_at`      | _N/A — derivable_                                                         | ADR 0044 §Decision §4b: `created_at` lives only on `ledgers`; recover via JOIN.   |
| `parse_error`     | `transactions.parse_error`                                                | bool direct.                                                                      |

Derived columns that have no parser counterpart and are computed during
staging:

- `transactions.application_order` — `1 + position_in_transactions_slice`
  (ADR 0028 1-based).
- `transactions.operation_count` — `operations.len()` for the tx's hash key.
- `transactions.has_soroban` — strict `op_type ∈ {InvokeHostFunction,
ExtendFootprintTtl, RestoreFootprint}` (matches PG, see staging.rs).

## ExtractedOperation → `operations_appearances`

Aggregation: identity tuple identical to PG (`tx_hash`, `op_type`,
`source`, `destination`, `contract_id`, `asset_code`, `asset_issuer`,
`pool_id`, `ledger_sequence`) folds into one row with
`amount = COUNT(*)` and `application_order = MIN(operation_index)`.

| Field              | CH target                                                                   | Notes                                                                                                                                                                                                                                                                                                       |
| ------------------ | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `transaction_hash` | `operations_appearances.transaction_id`                                     | `cityhash64(hash_bytes)` — same value as the parent `transactions.id` derivation.                                                                                                                                                                                                                           |
| `operation_index`  | `operations_appearances.application_order` (after `MIN()` fold)             | 1-based per ADR 0028 / task 0172.                                                                                                                                                                                                                                                                           |
| `op_type`          | `operations_appearances.type`                                               | `OperationType as i16`.                                                                                                                                                                                                                                                                                     |
| `source_account`   | `operations_appearances.source_id`                                          | `Option<StrKey> → Option<cityhash64_strkey>`.                                                                                                                                                                                                                                                               |
| `details` JSON     | `destination_id`, `contract_id`, `asset_code`, `asset_issuer_id`, `pool_id` | Unpacked per op type; mirrors PG `OpTyped::from_details` byte-for-byte (CreateAccount, Payment, PathPayment\*, AccountMerge, Clawback, LP Deposit/Withdraw, InvokeHostFunction, ChangeTrust, SetTrustLineFlags, AllowTrust, BeginSponsoringFutureReserves). Other op types carry nullable identity columns. |

Derived:

- `operations_appearances.id` — `cityhash64((tx_hash_bytes,
application_order_after_fold))`. Deterministic so replays dedup under
  ReplacingMergeTree.
- `operations_appearances.amount` — folded count.
- `operations_appearances.ledger_sequence` — from parent.

## ExtractedEvent → `soroban_events` (ADR 0044 §Decision §4a unfold)

CH **unfolds** events: one row per `ExtractedEvent` (vs PG, which folds
into `soroban_events_appearances` at one row per `(contract, tx,
ledger)` trio per ADR 0033). The full-content design is the v3-spike
reverted on the columnar side because columnar compression makes
per-row event detail cheap.

Filter: drop `EventSource::Diagnostic` before staging — CAP-67 byte-
identical Contract-typed copies of per-op consensus events would
double-count otherwise (matches PG `staging.rs` rule).

| Field              | CH target                        | Notes                                                                                                                                                                                                                                                                                                      |
| ------------------ | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `transaction_hash` | `soroban_events.transaction_id`  | `cityhash64(hash_bytes)`.                                                                                                                                                                                                                                                                                  |
| `event_type`       | `soroban_events.event_type`      | `ContractEventType as i16`.                                                                                                                                                                                                                                                                                |
| `source`           | _staged (filter)_                | `Diagnostic` rows dropped before any insert.                                                                                                                                                                                                                                                               |
| `contract_id`      | `soroban_events.contract_id`     | `Option<C-StrKey> → cityhash64_strkey`. **Required column on the CH side** (init.sql lists `Int64`, not `Nullable`) — rows with `None` are dropped with a single aggregate `tracing::debug!`. PG's appearance row is also keyed by contract; events without contract have nowhere to land in either store. |
| `topics`           | `soroban_events.topics_xdr`      | `serde_json::to_string(&value)` → CH `String`. Column name is a historical artefact (parser no longer emits raw XDR); ADR 0044 amendment if rename desired.                                                                                                                                                |
| `data`             | `soroban_events.data_xdr`        | Same: `serde_json::to_string(&value)` → CH `String`.                                                                                                                                                                                                                                                       |
| `event_index`      | `soroban_events.event_index`     | `u32 → i16` (Stellar caps tx event count well below i16 max).                                                                                                                                                                                                                                              |
| `ledger_sequence`  | `soroban_events.ledger_sequence` | `u32 → i64`.                                                                                                                                                                                                                                                                                               |
| `created_at`       | _N/A — derivable_                | See ADR 0044 §Decision §4b.                                                                                                                                                                                                                                                                                |

Derived:

- `soroban_events.signature` — `Nullable(String)`. Populated by
  `stage::extract_event_signature(topics)` which reads `topics[0]` and
  returns its inner `value` when the first topic is a Symbol ScVal
  (`{"type":"sym","value":"<name>"}`). Stellar SAC and most Soroban
  contract events follow this convention (e.g. `"transfer"`, `"mint"`,
  `"burn"`, `"fee"`). Events whose first topic is a non-Symbol ScVal
  (e.g. an Address) leave the column NULL. Very low cardinality so
  the `Nullable(String)` column compresses to near-zero footprint
  under CH defaults; no codec override needed here.

## ExtractedInvocation → `soroban_invocations_appearances`

Aggregation: same trio as PG ADR 0034 (`(contract, tx, ledger)`).
First non-NULL caller (account or contract) wins per trio.

| Field              | CH target                                                 | Notes                                                                                         |
| ------------------ | --------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `transaction_hash` | `soroban_invocations_appearances.transaction_id`          | `cityhash64(hash_bytes)`.                                                                     |
| `contract_id`      | `soroban_invocations_appearances.contract_id`             | `Option<C-StrKey> → cityhash64_strkey`. Rows without contract are dropped (no PG row either). |
| `caller_account`   | `caller_id` (G-prefix) or `caller_contract_id` (C-prefix) | Split by StrKey prefix, matches PG `ck_sia_caller_xor`.                                       |
| `function_name`    | _N/A — matches PG_                                        | API re-extracts at read time.                                                                 |
| `function_args`    | _N/A — matches PG_                                        | Same.                                                                                         |
| `return_value`     | _N/A — matches PG_                                        | Same.                                                                                         |
| `successful`       | _N/A — matches PG_                                        | Derived from parent tx success at read time.                                                  |
| `invocation_index` | _staged (root detection)_                                 | Used to determine "first node = root" for caller selection.                                   |
| `depth`            | _N/A — matches PG_                                        | API re-derives from XDR.                                                                      |
| `ledger_sequence`  | `soroban_invocations_appearances.ledger_sequence`         | `u32 → i64`.                                                                                  |
| `created_at`       | _N/A — derivable_                                         | ADR 0044 §Decision §4b.                                                                       |

Derived:

- `soroban_invocations_appearances.amount` — folded count, like PG.

## ExtractedContractInterface → `wasm_interface_metadata`

| Field           | CH target                           | Notes                                                                                      |
| --------------- | ----------------------------------- | ------------------------------------------------------------------------------------------ |
| `wasm_hash`     | `wasm_interface_metadata.wasm_hash` | hex → `FixedString(32)`. Dedup across the partition before insert.                         |
| `functions`     | `wasm_interface_metadata.metadata`  | `serde_json::to_string({ functions, wasm_byte_len })`. CH stores as `String`, PG as JSONB. |
| `wasm_byte_len` | _embedded in `metadata`_            | Folded into the JSON shape PG already writes.                                              |

## ExtractedContractDeployment → `soroban_contracts`

| Field                | CH target                                                           | Notes                                                                                             |
| -------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `contract_id`        | `soroban_contracts.contract_id`                                     | String column. Also derives `soroban_contracts.id = cityhash64_strkey(contract_id)`.              |
| `wasm_hash`          | `soroban_contracts.wasm_hash`                                       | hex → `Nullable(FixedString(32))`.                                                                |
| `deployer_account`   | `soroban_contracts.deployer_id`                                     | `cityhash64_strkey`.                                                                              |
| `deployed_at_ledger` | `soroban_contracts.deployed_at_ledger` + `.wasm_uploaded_at_ledger` | `u32 → i64`. `wasm_uploaded_at_ledger` is the `ReplacingMergeTree` version slot (Int64 DFLT 0).   |
| `contract_type`      | `soroban_contracts.contract_type`                                   | `ContractType as i16`.                                                                            |
| `is_sac`             | `soroban_contracts.is_sac`                                          | bool direct.                                                                                      |
| `name`               | `soroban_contracts.name`                                            | `Nullable(String)`.                                                                               |
| `sac_asset`          | _staged → feeds `assets`_                                           | `SacAssetIdentity::{Native,Credit}` rolls into asset emission (asset.contract_id ↔ SAC contract). |

Per ADR 0042 / task 0156, `contract_name_writes: &[(String, String)]` is
the retroactive UPDATE feed: each pair re-stamps `soroban_contracts.name`
on a later ledger. CH stages these as `(id = cityhash64_strkey(contract_id),
name)` and writes them through the same `soroban_contracts` insert —
ReplacingMergeTree dedup (`ORDER BY id`, version
`wasm_uploaded_at_ledger`) keeps the row count correct because the
retroactive write reuses the same `wasm_uploaded_at_ledger` watermark
as the original deploy row.

### Stub-rows for referenced-only contracts (PG parity)

CH staging emits a **stub `soroban_contracts` row** for every contract
referenced by `op_rows` / `event_rows` / `invocation_rows` /
`asset_rows` / `nft_rows` / `nft_events` that wasn't deployed in this
ledger (i.e. not in `contract_seen` and not in `contract_name_writes`).
Mirrors PG `write::upsert_contracts_returning_id` Pass 2
(`crates/indexer/src/handler/persist/write.rs:446–502`). Without it,
mid-stream backfill ranges would leave `soroban_contracts` empty even
though every `*.contract_id` FK column points at it.

Stub shape: `id = cityhash64_strkey(contract_id)`, `contract_id =
<C-StrKey>`, `is_sac = false`, everything else `NULL` or default.
`wasm_uploaded_at_ledger = 0` (version sentinel) — a future ledger
that contains the real deploy will write the row with a non-zero
version and ReplacingMergeTree's background merge keeps the
authoritative one.

## ExtractedAccountState → `accounts` + `account_balances_current`

| Field                | CH target                            | Notes                                                                                                                                                                                                                                                  |
| -------------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `account_id`         | `accounts.account_id`                | String column. Derives `accounts.id = cityhash64_strkey(account_id)`.                                                                                                                                                                                  |
| `first_seen_ledger`  | `accounts.first_seen_ledger`         | `Option<u32> → i64` (default 0 if creation not seen this batch).                                                                                                                                                                                       |
| `last_seen_ledger`   | `accounts.last_seen_ledger`          | `u32 → i64`. Version slot for ReplacingMergeTree.                                                                                                                                                                                                      |
| `sequence_number`    | `accounts.sequence_number`           | i64 direct. Trustline-only updates emit sentinel `-1`; CH writer applies the merge rule from PG `merge_account_state_overrides` and stages `0` when no real seq has been seen yet (CH has no Nullable column here).                                    |
| `balances` JSON      | `account_balances_current` rows      | Same split as PG: native ⇒ `(asset_code=NULL, issuer_id=NULL)`, credit ⇒ both NOT NULL.                                                                                                                                                                |
| `removed_trustlines` | `account_balances_current` mutations | Cross-tx re-add check, otherwise `ALTER … DELETE` against the credit row. **Out of scope for v1 of this writer** — the PG path issues a real DELETE; CH replays the trustline-removal as a soft cue. See §"Known trade-off: trustline removals" below. |
| `home_domain`        | `accounts.home_domain`               | `Nullable(String)`. Latest non-NULL wins per PG merge.                                                                                                                                                                                                 |
| `created_at`         | _N/A — derivable_                    |                                                                                                                                                                                                                                                        |

### Known trade-off: trustline removals

PG's `account_balances_current` write path issues `DELETE FROM
account_balances_current WHERE ...` for every entry in
`ExtractedAccountState.removed_trustlines` that is not re-added later in
the same ledger. CH's ReplacingMergeTree has no "delete row" verb in a
streaming insert — the only options are `ALTER TABLE … DELETE` (a heavy
mutation; not suitable for per-ledger streaming) or writing a tombstone
row (which would require `CollapsingMergeTree` / `VersionedCollapsingMergeTree`,
neither of which the 0204 schema chose).

For v1, the CH writer **does not delete** trustline rows. The credit
row will stay in `account_balances_current` with whatever balance it
had at last write; reads should filter `WHERE balance > 0` (or treat
zero-balance as "trustline-only").

Flagged in `docs/architecture/database-schema/clickhouse-pilot.md` as a
known divergence; revisit if a follow-up ADR moves
`account_balances_current` to a tombstone-capable engine.

## ExtractedLiquidityPool → `liquidity_pools`

> **Resolved by production-schema refactor (task 0208 folded inline).** > `liquidity_pools` engine is now `ReplacingMergeTree(last_updated_ledger)`
> with natural key ordering, deduplicating to 1 row per pool. PG's
> `created_at_ledger` column dropped on the CH side — derive at
> read time via `MIN(ledger_sequence) FROM liquidity_pool_snapshots
GROUP BY pool_id` if needed. Mapping below reflects production
> schema; see ADR 0044 history entry "Production-grade schema refactor"
> (2026-05-12) for the full rationale.

| Field                 | CH target                                                                  | Notes                                                                              |
| --------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `pool_id`             | `liquidity_pools.pool_id`                                                  | hex → `FixedString(32)`.                                                           |
| `asset_a` / `asset_b` | `asset_a_type` / `asset_a_code` / `asset_a_issuer_id` (and `_b` analogues) | Same JSON unpacking as PG `split_pool_asset`. Asset issuer → `cityhash64_strkey`.  |
| `fee_bps`             | `liquidity_pools.fee_bps`                                                  | `i32` direct.                                                                      |
| `reserves`            | _staged → feeds `liquidity_pool_snapshots`_                                | Not stored on the pool row itself.                                                 |
| `total_shares`        | _staged → feeds `liquidity_pool_snapshots`_                                | Same.                                                                              |
| `tvl`                 | _staged → feeds `liquidity_pool_snapshots`_                                | Same.                                                                              |
| `created_at_ledger`   | `liquidity_pools.created_at_ledger`                                        | `Option<u32> → i64` (earliest non-NULL across same-ledger duplicates).             |
| `last_updated_ledger` | _staged_                                                                   | The schema does not carry this on `liquidity_pools`; used for pool-row dedup only. |
| `created_at`          | _N/A — derivable_                                                          |                                                                                    |

## ExtractedLiquidityPoolSnapshot → `liquidity_pool_snapshots`

| Field             | CH target                                  | Notes                                                                                                      |
| ----------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `pool_id`         | `liquidity_pool_snapshots.pool_id`         | hex → `FixedString(32)`.                                                                                   |
| `ledger_sequence` | `liquidity_pool_snapshots.ledger_sequence` | `u32 → i64`.                                                                                               |
| `created_at`      | _N/A — derivable_                          |                                                                                                            |
| `reserves`        | `reserve_a` / `reserve_b`                  | JSON `{a, b}` → `Decimal128(7)` each (stroops → decimal string, matches PG `format_stroops`).              |
| `total_shares`    | `liquidity_pool_snapshots.total_shares`    | String → `Decimal128(7)`. Accept both already-formatted and raw stroops (matches PG `format_stroops_str`). |
| `tvl`             | `liquidity_pool_snapshots.tvl`             | `Option<String> → Option<Decimal128(7)>`.                                                                  |
| `volume`          | `liquidity_pool_snapshots.volume`          | Same.                                                                                                      |
| `fee_revenue`     | `liquidity_pool_snapshots.fee_revenue`     | Same.                                                                                                      |

Derived:

- `liquidity_pool_snapshots.id` — `cityhash64((pool_id_bytes, ledger_sequence_i64))`.

## ExtractedAsset → `assets`

| Field            | CH target             | Notes                                                              |
| ---------------- | --------------------- | ------------------------------------------------------------------ |
| `asset_type`     | `assets.asset_type`   | `TokenAssetType as i16`.                                           |
| `asset_code`     | `assets.asset_code`   | `Option<String>`. NULL for native, classic-issuer-NULL is invalid. |
| `issuer_address` | `assets.issuer_id`    | `Option<StrKey> → Option<cityhash64_strkey>`.                      |
| `contract_id`    | `assets.contract_id`  | `Option<C-StrKey> → Option<cityhash64_strkey>`.                    |
| `name`           | `assets.name`         | `Option<String>`.                                                  |
| `total_supply`   | `assets.total_supply` | `Option<String> → Option<Decimal128(7)>`.                          |
| `holder_count`   | `assets.holder_count` | `Option<i32>`.                                                     |

Dedup identity: same as PG (`native` singleton, classic by
`(code, issuer)`, soroban/sac by `contract_id`). The CH writer dedups
the **emitted set** before staging — duplicates across ledgers within
the same partition are folded by ReplacingMergeTree on the merge side.

Derived:

- `assets.id` — `cityhash64((asset_type_i16, asset_code_or_empty,
issuer_id_i64, contract_id_i64)) as i32` (Int32 column). 32-bit
  collision posture documented in `docs/architecture/database-schema/clickhouse-pilot.md`.
- `assets.icon_url` — always `NULL` from this writer; populated by the
  enrichment lambda (out of scope here, matches PG behaviour).

### Native XLM singleton

The `(asset_type=Native, name='Stellar Lumen')` row is **not** produced
by `xdr_parser::state::detect_assets` on either side. PG seeds it via
a one-shot sqlx migration
(`crates/db/migrations/20260428000000_seed_native_asset_singleton.up.sql`,
task 0161). CH has no migration ladder analogue (`init.sql` is pure
DDL), so the writer stages the row each ledger. ReplacingMergeTree
dedups by `ORDER BY id` on the deterministic `asset_id(Native, None,
None, None)` hash — net steady-state after the background merger
runs is exactly one Native row, regardless of backfill width.

## ExtractedNft → `nfts`

| Field              | CH target                   | Notes                                                                               |
| ------------------ | --------------------------- | ----------------------------------------------------------------------------------- |
| `contract_id`      | `nfts.contract_id`          | C-StrKey → `cityhash64_strkey` (Int64).                                             |
| `token_id`         | `nfts.token_id`             | String direct.                                                                      |
| `collection_name`  | `nfts.collection_name`      | `Nullable(String)`.                                                                 |
| `owner_account`    | `nfts.current_owner_id`     | `Option<G-StrKey> → Option<cityhash64_strkey>`.                                     |
| `name`             | `nfts.name`                 | `Nullable(String)`.                                                                 |
| `media_url`        | `nfts.media_url`            | `Nullable(String)`.                                                                 |
| `minted_at_ledger` | `nfts.minted_at_ledger`     | `Option<u32> → Option<i64>`.                                                        |
| `last_seen_ledger` | `nfts.current_owner_ledger` | `u32 → i64`. Version slot for ReplacingMergeTree (NOT NULL DEFAULT 0 per init.sql). |
| `created_at`       | _N/A — derivable_           |                                                                                     |

Per ADR 0044 §Decision §4c: **no `metadata` column on CH** (PG keeps it).

Derived:

- `nfts.id` — `cityhash64((contract_id_strkey, token_id)) as i32`.

## ExtractedNftEvent → `nft_ownership`

Empty slice until task 0202 lands. Writer treats `[]` as `Ok(())` —
no inserts opened.

| Field              | CH target                       | Notes                                                      |
| ------------------ | ------------------------------- | ---------------------------------------------------------- |
| `transaction_hash` | `nft_ownership.transaction_id`  | `cityhash64(hash_bytes)`.                                  |
| `contract_id`      | _staged_                        | Combined with `token_id` to derive `nft_ownership.nft_id`. |
| `token_id`         | _staged_                        | Same.                                                      |
| `event_type`       | `nft_ownership.event_type`      | `NftEventType as i16`.                                     |
| `owner_account`    | `nft_ownership.owner_id`        | `Option<G-StrKey> → Option<cityhash64_strkey>`.            |
| `event_order`      | `nft_ownership.event_order`     | `u16 → i16`.                                               |
| `ledger_sequence`  | `nft_ownership.ledger_sequence` | `u32 → i64`.                                               |
| `created_at`       | _N/A — derivable_               |                                                            |

Derived:

- `nft_ownership.nft_id` — `cityhash64((contract_id_strkey, token_id)) as i32`
  (matches `nfts.id`).

## ExtractedLpPosition → `lp_positions`

Empty slice until task 0126 lands. Writer treats `[]` as `Ok(())`.

| Field                  | CH target                           | Notes                                                                      |
| ---------------------- | ----------------------------------- | -------------------------------------------------------------------------- |
| `pool_id`              | `lp_positions.pool_id`              | hex → `FixedString(32)`.                                                   |
| `account_id`           | `lp_positions.account_id`           | G-StrKey → `cityhash64_strkey`.                                            |
| `shares`               | `lp_positions.shares`               | String → `Decimal128(7)`.                                                  |
| `first_deposit_ledger` | `lp_positions.first_deposit_ledger` | `Option<u32> → i64` (default `last_updated_ledger` when None, matches PG). |
| `last_updated_ledger`  | `lp_positions.last_updated_ledger`  | `u32 → i64`. Version slot.                                                 |

## ExtractedLedgerEntryChange

_N/A — matches PG._ The parser feeds these into downstream typed
extractions (`account_states`, `contract_deployments`,
`liquidity_pools`, `assets`, `nfts`). Neither store keeps raw
`ExtractedLedgerEntryChange` rows.

## Cross-cutting staging-derived rows

### `transaction_participants`

PG builds a union per tx: `source ∪ op destinations ∪ tx-level event
participants (G-only) ∪ invocation callers (G-only) ∪ NFT owners ∪
asset issuers (issuer-of-issuer). CH mirrors this byte-for-byte. Output
shape: `(account_id, ledger_sequence, transaction_id)` with surrogate
IDs derived as above.

### `accounts` universe

Same union as PG's `account_keys_set` — every G-StrKey referenced
anywhere in the ledger (tx source / op destinations / op asset issuers /
event transfer participants / invocation callers / contract deployer /
account_states / pool asset issuers / asset issuers / nft owners / nft
event owners / lp position accounts). Defense-in-depth filter on
`len ≤ 56 && starts_with('G')` per PG behaviour.

Watermark-guarded fields (`sequence_number`, `first_seen_ledger`,
`home_domain`) come from `ExtractedAccountState`; pure-reference
accounts get the parser's defaults (`sequence_number = 0`,
`first_seen_ledger = last_seen_ledger`, `home_domain = NULL`).

---

## Hybrid surrogate-id / natural-key design

After empirical measurement (10 k-ledger smoke: ~500 MB extra on-disk
vs hash-i64 baseline, +10 ms write/ledger), the production design
combines:

- **Surrogate `id Int64`** on three central FK hubs: `accounts`,
  `soroban_contracts`, `transactions`. Derived via
  `cityhash64(natural_key)` (deterministic; see
  `crates/db-clickhouse/src/persist/ids.rs`). FK columns referencing
  these are `Int64` — cheap integer joins, ~7× smaller on-disk than
  StrKey FK columns.
- **Natural / composite keys** on everything else (`assets`, `nfts`,
  `liquidity_pools`, `lp_positions`, `liquidity_pool_snapshots`,
  `operations_appearances`, `transaction_participants`,
  `nft_ownership`) — composite ORDER BYs over already-cheap-shape
  columns work without a hash layer.

| Table                             | ORDER BY                                                      | Has surrogate `id`? |
| --------------------------------- | ------------------------------------------------------------- | ------------------- |
| `accounts`                        | `account_id` (StrKey G…)                                      | **yes** — Int64     |
| `soroban_contracts`               | `contract_id` (StrKey C…)                                     | **yes** — Int64     |
| `transactions`                    | `(ledger_sequence, application_order)`                        | **yes** — Int64     |
| `assets`                          | `(asset_type, asset_code, issuer_id, contract_id)`            | no                  |
| `account_balances_current`        | `(account_id, asset_type, asset_code, issuer_id)`             | no                  |
| `nfts`                            | `(contract_id, token_id)`                                     | no                  |
| `liquidity_pools`                 | `pool_id`                                                     | no                  |
| `lp_positions`                    | `(pool_id, account_id)`                                       | no                  |
| `transaction_hash_index`          | `hash`                                                        | no                  |
| `operations_appearances`          | `(ledger_sequence, transaction_id, application_order)`        | no                  |
| `transaction_participants`        | `(account_id, ledger_sequence, transaction_id)`               | no                  |
| `soroban_events`                  | `(contract_id, ledger_sequence, transaction_id, event_index)` | no                  |
| `soroban_invocations_appearances` | `(contract_id, ledger_sequence, transaction_id)`              | no                  |
| `nft_ownership`                   | `(contract_id, token_id, ledger_sequence, event_order)`       | no                  |
| `liquidity_pool_snapshots`        | `(pool_id, ledger_sequence)`                                  | no                  |

Cross-table FK consistency by Int64 equality on the three hub tables
(`accounts.id`, `soroban_contracts.id`, `transactions.id`) and by
natural key elsewhere. All `_id` FK columns derived from
`cityhash64(natural_key)` via `super::ids`. Verified by
`persist/ids.rs::tests::fk_consistency_account_id`.

Detailed rationale in
`docs/architecture/database-schema/clickhouse-pilot.md` §"Hybrid
surrogate / natural keys".
