# 2026-05-13 — List-endpoint completeness audit (0197 Step 1)

**Branch:** `feat/0197_db-completeness-audit-and-docs`
**DB:** local Postgres (Docker Compose, port 5434), database `soroban_block_explorer`
**Backfill range:** ledgers `50944000..51007999` (one S3 partition `FCF6A7FF`, 23 332 ledgers)
**Status snapshots:** see sibling files

- PRE-drain (after seed workaround): `2026-05-13-fresh-pre-drain-post-seed-status.md`
- POST-drain (after sep1-assets + nft-metadata + Bug #5/#6 fix re-run): `2026-05-13-fresh-post-enrichment-status.md`

**Sentinel-aware counting** per ADR 0043 / 0191 #12: `''` empty string = "fetch attempted, no data published by source" (counts as enrichment-wired, NOT as FAIL). NULL on an enrichment-target row = "not yet drained" (transient retry candidate). Real value = parsed and stored. FAIL rule for enrichment columns = "NULL after POST drain on a non-skipped row".

For indexer-driven columns FAIL rule = "NULL where the row was indexed during ingest". Surfaced as findings below where the % deviates from expectation.

---

## 1. Scope

Only paginated **list** endpoints (and paginated sub-resource list endpoints reachable via `:id` paths). Detail endpoints (`GET /v1/{resource}/:id` returning one object) are covered in Step 2.

**Endpoints AUDITED:** 11 paginated list shapes

- `GET /v1/assets`
- `GET /v1/transactions`
- `GET /v1/ledgers`
- `GET /v1/nfts`
- `GET /v1/nfts/:id/transfers`
- `GET /v1/liquidity-pools`
- `GET /v1/liquidity-pools/:id/participants`
- `GET /v1/liquidity-pools/:id/transactions`
- `GET /v1/accounts/:id/transactions`
- `GET /v1/contracts/:id/invocations`
- `GET /v1/contracts/:id/events`

**Endpoints NOT present (surfaced during enumeration):**

- `GET /v1/accounts` (paginated list of all accounts) — not implemented
- `GET /v1/contracts` (paginated list of all contracts) — not implemented
- `GET /v1/operations` (paginated list of all operations) — not implemented

The task spec referenced these three. They never shipped — search across `crates/api/src/**/mod.rs` (router registrations), `libs/api-types/src/openapi.json`, and handler list shows no `list_accounts` / `list_contracts` / `list_operations`. Flagged in §3 as audit finding (no decision in this task — pure observation).

ClickHouse parallel store (ADR 0044 / tasks 0204/0206/0207) is **out of scope** here. CH is not wired to the API read-path; the `endpoint-queries-clickhouse/` reference set is owned by 0207 and a parity audit on it is deferred to a follow-up that runs once CH serves a real `/v1/*` handler.

---

## 2. Coverage matrix

Owner legend: **indexer** = written during ingest from XDR; **L2 sep1** = enrichment Lambda 2 `sep1_assets` kind; **L2 nft** = enrichment Lambda 2 `nft_metadata` kind; **SQL** = computed in the canonical SQL via CASE / array_agg / window function; **handler** = computed in Rust handler; **type-2 archive** = runtime XDR fetch from stellar-archive via `runtime_enrichment` (ADR 0029 umbrella).

Triple is `(NULL / sentinel `''` / populated)` from the live local DB. PRE column omitted for indexer-driven columns (PRE = POST by definition; indexer writes during ingest). For enrichment columns PRE = "after ingest + seed workaround, before drain" per `2026-05-13-fresh-pre-drain-post-seed-status.md`.

### 2.1 `GET /v1/assets`

- DTO: [crates/api/src/assets/dto.rs:24](crates/api/src/assets/dto.rs:24) (`AssetItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/08_get_assets_list.sql](docs/architecture/database-schema/endpoint-queries/08_get_assets_list.sql)
- Handler: [crates/api/src/assets/handlers.rs:94](crates/api/src/assets/handlers.rs:94)
- Source rows: 12 900 (`assets`). 7 indexer-discovered + 12 893 seed-workaround rows (Bug #1).

| DTO field         | Source                                                             | Indexed?                     | Owner       | PRE (N/S/P)    | POST (N/S/P)         | Status                                                                                                 |
| ----------------- | ------------------------------------------------------------------ | ---------------------------- | ----------- | -------------- | -------------------- | ------------------------------------------------------------------------------------------------------ |
| `id`              | `assets.id`                                                        | PK                           | indexer     | n/a            | 0 / 0 / 12 900       | OK                                                                                                     |
| `asset_type`      | `assets.asset_type`                                                | `idx_assets_type`            | indexer     | n/a            | 0 / 0 / 12 900       | OK                                                                                                     |
| `asset_type_name` | SQL CASE on `asset_type`                                           | n/a                          | SQL         | n/a            | n/a                  | OK                                                                                                     |
| `asset_code`      | `assets.asset_code`                                                | `idx_assets_code_trgm` (GIN) | indexer     | n/a            | 1 / 0 / 12 899       | OK (1 NULL = native XLM)                                                                               |
| `issuer`          | LEFT JOIN `accounts.account_id` via `assets.issuer_id`             | accounts PK                  | indexer     | n/a            | 1 / 0 / 12 899       | OK (native XLM has no issuer)                                                                          |
| `contract_id`     | LEFT JOIN `soroban_contracts.contract_id` via `assets.contract_id` | soroban_contracts PK         | indexer     | n/a            | 12 894 / 0 / 6       | OK (only SAC-wrapped assets have contract_id)                                                          |
| `name`            | `assets.name`                                                      | no                           | **L2 sep1** | 12 894 / 0 / 6 | 2 159 / 10 130 / 611 | **OK** (drain wired; 16.7% transient = network retry candidates)                                       |
| `total_supply`    | `assets.total_supply`                                              | no                           | indexer     | n/a            | 12 898 / 0 / 2       | **FAIL → Finding F1** (Bug #1 / #4 — seed workaround did not populate; only 2/12 900 have real values) |
| `holder_count`    | `assets.holder_count`                                              | no                           | indexer     | n/a            | 12 898 / 0 / 2       | **FAIL → Finding F1** (same root cause as `total_supply`)                                              |
| `icon_url`        | `assets.icon_url`                                                  | no                           | **L2 sep1** | 12 894 / 0 / 6 | 2 159 / 10 138 / 603 | OK                                                                                                     |

**Bug #1 root cause** (already documented in `2026-05-13-pre-audit-finding-classic-credit-asset-row-missing.md`): classic-credit `assets` rows are not emitted at trustline-creation events; only auto-seeded from `account_balances_current` rows in the workaround SQL, which doesn't recompute `total_supply` / `holder_count`. Fix lives indexer-side and is Stanisław's scope.

### 2.2 `GET /v1/transactions`

- DTO: [crates/api/src/transactions/dto.rs:26](crates/api/src/transactions/dto.rs:26) (`TransactionListItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/02_get_transactions_list.sql](docs/architecture/database-schema/endpoint-queries/02_get_transactions_list.sql)
- Handler: [crates/api/src/transactions/handlers.rs:54](crates/api/src/transactions/handlers.rs:54)
- Source rows: 8 683 448 (`transactions`).

| DTO field           | Source                                                                                                                                                                | Indexed?                        | Owner   | POST (N/S/P)            | Status                                                   |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- | ------- | ----------------------- | -------------------------------------------------------- |
| `hash`              | `encode(transactions.hash, 'hex')`                                                                                                                                    | PK                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `ledger_sequence`   | `transactions.ledger_sequence`                                                                                                                                        | (composite via `idx_tx_keyset`) | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `application_order` | `transactions.application_order`                                                                                                                                      | no                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `source_account`    | JOIN `accounts.account_id` via `transactions.source_id`                                                                                                               | accounts PK                     | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `fee_charged`       | `transactions.fee_charged`                                                                                                                                            | no                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `inner_tx_hash`     | `encode(transactions.inner_tx_hash, 'hex')`                                                                                                                           | no                              | indexer | 8 529 130 / 0 / 154 318 | OK (only fee-bump txs have inner_tx_hash; 1.8% expected) |
| `successful`        | `transactions.successful`                                                                                                                                             | no                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `operation_count`   | `transactions.operation_count`                                                                                                                                        | no                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `has_soroban`       | `transactions.has_soroban`                                                                                                                                            | no                              | indexer | 0 / 0 / 8 683 448       | OK                                                       |
| `operation_types`   | `array_agg(DISTINCT op_type_name(...))` from `operations_appearances`                                                                                                 | (FK index)                      | SQL     | n/a                     | OK                                                       |
| `contract_ids`      | UNION of `operations_appearances.contract_id`, `soroban_invocations_appearances.contract_id`, `soroban_events_appearances.contract_id`, joined to `soroban_contracts` | (FK indexes)                    | SQL     | n/a                     | OK                                                       |
| `created_at`        | `transactions.created_at`                                                                                                                                             | `idx_tx_keyset`                 | indexer | 0 / 0 / 8 683 448       | OK                                                       |

### 2.3 `GET /v1/ledgers`

- DTO: [crates/api/src/ledgers/dto.rs:24](crates/api/src/ledgers/dto.rs:24) (`LedgerListItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/04_get_ledgers_list.sql](docs/architecture/database-schema/endpoint-queries/04_get_ledgers_list.sql)
- Handler: [crates/api/src/ledgers/handlers.rs:48](crates/api/src/ledgers/handlers.rs:48)
- Source rows: 23 332 (`ledgers`).

| DTO field           | Source                        | Indexed?                | Owner   | POST (N/S/P)   | Status |
| ------------------- | ----------------------------- | ----------------------- | ------- | -------------- | ------ |
| `sequence`          | `ledgers.sequence`            | PK                      | indexer | 0 / 0 / 23 332 | OK     |
| `hash`              | `encode(ledgers.hash, 'hex')` | UNIQUE                  | indexer | 0 / 0 / 23 332 | OK     |
| `closed_at`         | `ledgers.closed_at`           | `idx_ledgers_closed_at` | indexer | 0 / 0 / 23 332 | OK     |
| `protocol_version`  | `ledgers.protocol_version`    | no                      | indexer | 0 / 0 / 23 332 | OK     |
| `transaction_count` | `ledgers.transaction_count`   | no                      | indexer | 0 / 0 / 23 332 | OK     |
| `base_fee`          | `ledgers.base_fee`            | no                      | indexer | 0 / 0 / 23 332 | OK     |

### 2.4 `GET /v1/nfts`

- DTO: [crates/api/src/nfts/dto.rs:32](crates/api/src/nfts/dto.rs:32) (`NftItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/15_get_nfts_list.sql](docs/architecture/database-schema/endpoint-queries/15_get_nfts_list.sql)
- Handler: [crates/api/src/nfts/handlers.rs:58](crates/api/src/nfts/handlers.rs:58)
- Source rows: 1 088 584 (`nfts`).

| DTO field          | Source                                                      | Indexed?                                          | Owner                                                             | PRE (N/S/P)       | POST (N/S/P)           | Status                                                                                                                                                                |
| ------------------ | ----------------------------------------------------------- | ------------------------------------------------- | ----------------------------------------------------------------- | ----------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`               | `nfts.id`                                                   | PK                                                | indexer                                                           | n/a               | 0 / 0 / 1 088 584      | OK                                                                                                                                                                    |
| `contract_id`      | JOIN `soroban_contracts.contract_id` via `nfts.contract_id` | soroban_contracts PK                              | indexer                                                           | n/a               | 0 / 0 / 1 088 584      | OK                                                                                                                                                                    |
| `token_id`         | `nfts.token_id`                                             | UNIQUE(contract_id, token_id)                     | indexer                                                           | n/a               | 0 / 0 / 1 088 584      | OK                                                                                                                                                                    |
| `collection_name`  | `nfts.collection_name`                                      | `idx_nfts_collection`, `idx_nfts_collection_trgm` | **L2 nft** (per `database-schema-overview.md:739`; task 0195 §2d) | 1 088 584 / 0 / 0 | 1 087 583 / 1 001 / 0  | OK (only Bachini fixture had no collection_name in JSON)                                                                                                              |
| `name`             | `nfts.name`                                                 | `idx_nfts_name_trgm`                              | **L2 nft**                                                        | 1 088 584 / 0 / 0 | 1 087 583 / 1 000 / 1  | OK (1 real = Bachini fixture; only 1 000-row sample drained, rest = NULL pending)                                                                                     |
| `media_url`        | `nfts.media_url`                                            | no                                                | **L2 nft**                                                        | 1 088 584 / 0 / 0 | 1 087 583 / 1 001 / 0  | OK                                                                                                                                                                    |
| `minted_at_ledger` | `nfts.minted_at_ledger`                                     | no                                                | indexer                                                           | n/a               | 1 077 813 / 0 / 10 771 | **FAIL → Finding F2** (99 % NULL — only mint events populate this; the bulk of `nfts` rows come from transfer events / Bug #3 false-positives that never see a mint). |
| `owner_account`    | LEFT JOIN `accounts.account_id` via `nfts.current_owner_id` | accounts PK                                       | indexer                                                           | n/a               | 471 413 / 0 / 617 171  | **WARN → Finding F3** (43 % NULL — burned NFTs or rows without observed owner transfer)                                                                               |
| `last_seen_ledger` | `nfts.current_owner_ledger`                                 | no                                                | indexer                                                           | n/a               | 1 / 0 / 1 088 583      | OK (1 NULL = the Bachini fixture row manually inserted without a transfer event)                                                                                      |

### 2.5 `GET /v1/nfts/:id/transfers`

- DTO: [crates/api/src/nfts/dto.rs:72](crates/api/src/nfts/dto.rs:72) (`NftTransferItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/17_get_nfts_transfers.sql](docs/architecture/database-schema/endpoint-queries/17_get_nfts_transfers.sql)
- Handler: [crates/api/src/nfts/handlers.rs:200](crates/api/src/nfts/handlers.rs:200)

| DTO field          | Source                                                                                                                  | Indexed?     | Owner   | Status |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------- | ------------ | ------- | ------ |
| `transaction_hash` | JOIN `transactions.hash` via `nft_ownership.(transaction_id, created_at)`                                               | tx PK        | indexer | OK     |
| `ledger_sequence`  | `nft_ownership.ledger_sequence`                                                                                         | composite PK | indexer | OK     |
| `event_type`       | `nft_ownership.event_type`                                                                                              | no           | indexer | OK     |
| `event_type_name`  | SQL CASE via `nft_event_type_name(event_type)`                                                                          | n/a          | SQL     | OK     |
| `from_account`     | `LEAD(accounts.account_id) OVER (PARTITION BY nft_id ORDER BY created_at DESC, ledger_sequence DESC, event_order DESC)` | accounts PK  | SQL     | OK     |
| `to_account`       | LEFT JOIN `accounts.account_id` via `nft_ownership.owner_id`                                                            | accounts PK  | indexer | OK     |
| `created_at`       | `nft_ownership.created_at`                                                                                              | composite PK | indexer | OK     |
| `event_order`      | `nft_ownership.event_order`                                                                                             | composite PK | indexer | OK     |

Empirical: `nft_ownership` is the transfer-event table; rows are PK-mandatory so no NULL surface for the audited fields.

### 2.6 `GET /v1/liquidity-pools`

- DTO: [crates/api/src/liquidity_pools/dto.rs:97](crates/api/src/liquidity_pools/dto.rs:97) (`PoolItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/18_get_liquidity_pools_list.sql](docs/architecture/database-schema/endpoint-queries/18_get_liquidity_pools_list.sql)
- Handler: [crates/api/src/liquidity_pools/handlers.rs:193](crates/api/src/liquidity_pools/handlers.rs:193)
- Source rows: 11 985 (`liquidity_pools`) + 392 256 (`liquidity_pool_snapshots`).

| DTO field                 | Source                                                    | Indexed?                        | Owner                                | POST (N/S/P)      | Status                                              |
| ------------------------- | --------------------------------------------------------- | ------------------------------- | ------------------------------------ | ----------------- | --------------------------------------------------- |
| `pool_id`                 | `encode(liquidity_pools.pool_id, 'hex')`                  | PK                              | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `asset_a.asset_type`      | `liquidity_pools.asset_a_type`                            | (composite `idx_pools_asset_a`) | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `asset_a.asset_type_name` | SQL CASE                                                  | n/a                             | SQL                                  | n/a               | OK                                                  |
| `asset_a.asset_code`      | `liquidity_pools.asset_a_code`                            | `idx_pools_asset_a`             | indexer                              | 2 726 / 0 / 9 259 | OK (NULL = pool where asset A is native XLM)        |
| `asset_a.issuer`          | LEFT JOIN `accounts.account_id` via `asset_a_issuer_id`   | accounts PK                     | indexer                              | 2 726 / 0 / 9 259 | OK (native XLM = no issuer; matches `asset_a_code`) |
| `asset_b.asset_type`      | `liquidity_pools.asset_b_type`                            | `idx_pools_asset_b`             | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `asset_b.asset_code`      | `liquidity_pools.asset_b_code`                            | `idx_pools_asset_b`             | indexer                              | 0 / 0 / 11 985    | OK (in all sampled pools asset B is non-native)     |
| `asset_b.issuer`          | LEFT JOIN `accounts.account_id` via `asset_b_issuer_id`   | accounts PK                     | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `fee_bps`                 | `liquidity_pools.fee_bps`                                 | no                              | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `fee_percent`             | SQL `fee_bps::numeric / 100`                              | n/a                             | SQL                                  | n/a               | OK                                                  |
| `created_at_ledger`       | `liquidity_pools.created_at_ledger`                       | `idx_pools_created_at_ledger`   | indexer                              | 0 / 0 / 11 985    | OK                                                  |
| `latest_snapshot_ledger`  | LATERAL latest `liquidity_pool_snapshots.ledger_sequence` | (FK / created_at index)         | indexer                              | 0 / 0 / 392 256   | OK (snapshot table)                                 |
| `reserve_a`               | LATERAL `liquidity_pool_snapshots.reserve_a`              | no                              | indexer                              | 0 / 0 / 392 256   | OK                                                  |
| `reserve_b`               | LATERAL `liquidity_pool_snapshots.reserve_b`              | no                              | indexer                              | 0 / 0 / 392 256   | OK                                                  |
| `total_shares`            | LATERAL `liquidity_pool_snapshots.total_shares`           | no                              | indexer                              | 0 / 0 / 392 256   | OK                                                  |
| `tvl`                     | LATERAL `liquidity_pool_snapshots.tvl`                    | `idx_lps_tvl` (DESC)            | (declared L2 sep1) — **today 0 / 0** | 392 256 / 0 / 0   | **DEFERRED → 0199**                                 |
| `volume`                  | LATERAL `liquidity_pool_snapshots.volume`                 | no                              | (declared L2 sep1)                   | 392 256 / 0 / 0   | **DEFERRED → 0199**                                 |
| `fee_revenue`             | LATERAL `liquidity_pool_snapshots.fee_revenue`            | no                              | (declared L2 sep1)                   | 392 256 / 0 / 0   | **DEFERRED → 0199**                                 |
| `latest_snapshot_at`      | LATERAL `liquidity_pool_snapshots.created_at`             | no                              | indexer                              | 0 / 0 / 392 256   | OK                                                  |

LP analytics columns (`tvl` / `volume` / `fee_revenue`) deferred to task 0199 (blocked on team-built price API). 100 % NULL is expected baseline until 0199 ships.

### 2.7 `GET /v1/liquidity-pools/:id/participants`

- DTO: [crates/api/src/liquidity_pools/dto.rs:31](crates/api/src/liquidity_pools/dto.rs:31) (`ParticipantItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/23_get_liquidity_pools_participants.sql](docs/architecture/database-schema/endpoint-queries/23_get_liquidity_pools_participants.sql)
- Handler: [crates/api/src/liquidity_pools/handlers.rs:51](crates/api/src/liquidity_pools/handlers.rs:51)

| DTO field              | Source                                                      | Indexed?                                    | Owner   | Status |
| ---------------------- | ----------------------------------------------------------- | ------------------------------------------- | ------- | ------ |
| `account`              | JOIN `accounts.account_id` via `lp_positions.account_id`    | accounts PK                                 | indexer | OK     |
| `shares`               | `lp_positions.shares`                                       | `idx_lpp_shares` (partial WHERE shares > 0) | indexer | OK     |
| `share_percentage`     | SQL `(shares * 100.0 / total_shares)` from LATERAL snapshot | n/a                                         | SQL     | OK     |
| `first_deposit_ledger` | `lp_positions.first_deposit_ledger`                         | no                                          | indexer | OK     |
| `last_updated_ledger`  | `lp_positions.last_updated_ledger`                          | no                                          | indexer | OK     |

Not row-counted: per-pool small lists, no surface for systemic NULLs.

### 2.8 `GET /v1/liquidity-pools/:id/transactions`

- DTO: [crates/api/src/liquidity_pools/dto.rs:120](crates/api/src/liquidity_pools/dto.rs:120) (`PoolTransactionItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/20_get_liquidity_pools_transactions.sql](docs/architecture/database-schema/endpoint-queries/20_get_liquidity_pools_transactions.sql)
- Handler: [crates/api/src/liquidity_pools/handlers.rs:344](crates/api/src/liquidity_pools/handlers.rs:344)

| DTO field         | Source                                                                | Indexed?                                  | Owner   | Status |
| ----------------- | --------------------------------------------------------------------- | ----------------------------------------- | ------- | ------ |
| `hash`            | `encode(transactions.hash, 'hex')`                                    | tx PK                                     | indexer | OK     |
| `ledger_sequence` | `transactions.ledger_sequence`                                        | (composite)                               | indexer | OK     |
| `source_account`  | JOIN `accounts.account_id` via `transactions.source_id`               | accounts PK                               | indexer | OK     |
| `fee_charged`     | `transactions.fee_charged`                                            | no                                        | indexer | OK     |
| `successful`      | `transactions.successful`                                             | no                                        | indexer | OK     |
| `operation_count` | `transactions.operation_count`                                        | no                                        | indexer | OK     |
| `has_soroban`     | `transactions.has_soroban`                                            | no                                        | indexer | OK     |
| `operation_types` | `array_agg(DISTINCT op_type_name(...))` from `operations_appearances` | (FK index)                                | SQL     | OK     |
| `created_at`      | `transactions.created_at`                                             | `idx_ops_app_pool` (partial on `pool_id`) | indexer | OK     |

### 2.9 `GET /v1/accounts/:id/transactions`

- DTO: [crates/api/src/accounts/dto.rs:34](crates/api/src/accounts/dto.rs:34) (`AccountTransactionItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/07_get_accounts_transactions.sql](docs/architecture/database-schema/endpoint-queries/07_get_accounts_transactions.sql)
- Handler: [crates/api/src/accounts/handlers.rs:102](crates/api/src/accounts/handlers.rs:102)

Same DTO shape as `/v1/transactions` minus `inner_tx_hash` (verified at audit) — all indexer-driven, all 0 NULL on the source tables. Pagination keyset: `(transaction_participants(account_id, created_at DESC, transaction_id DESC), transactions(created_at, id))` (task 0132 index).

### 2.10 `GET /v1/contracts/:id/invocations`

- DTO: [crates/api/src/contracts/dto.rs:44](crates/api/src/contracts/dto.rs:44) (`InvocationItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/13_get_contracts_invocations.sql](docs/architecture/database-schema/endpoint-queries/13_get_contracts_invocations.sql)
- Handler: [crates/api/src/contracts/handlers.rs:249](crates/api/src/contracts/handlers.rs:249)

| DTO field          | Source                                            | Indexed?           | Owner   | Status |
| ------------------ | ------------------------------------------------- | ------------------ | ------- | ------ |
| `transaction_hash` | `encode(transactions.hash, 'hex')`                | tx PK              | indexer | OK     |
| `ledger_sequence`  | `soroban_invocations_appearances.ledger_sequence` | (task 0132 index)  | indexer | OK     |
| `caller_account`   | LEFT JOIN `accounts.account_id`                   | accounts PK        | indexer | OK     |
| `amount`           | `soroban_invocations_appearances.amount`          | no                 | indexer | OK     |
| `created_at`       | `soroban_invocations_appearances.created_at`      | (pagination index) | indexer | OK     |
| `successful`       | `transactions.successful`                         | no                 | indexer | OK     |

### 2.11 `GET /v1/contracts/:id/events`

- DTO: [crates/api/src/contracts/dto.rs:57](crates/api/src/contracts/dto.rs:57) (`EventItem`)
- SQL: [docs/architecture/database-schema/endpoint-queries/14_get_contracts_events.sql](docs/architecture/database-schema/endpoint-queries/14_get_contracts_events.sql)
- Handler: [crates/api/src/contracts/handlers.rs:338](crates/api/src/contracts/handlers.rs:338)

| DTO field          | Source                                       | Indexed?           | Owner          | Status                                  |
| ------------------ | -------------------------------------------- | ------------------ | -------------- | --------------------------------------- |
| `transaction_hash` | `encode(transactions.hash, 'hex')`           | tx PK              | indexer        | OK                                      |
| `ledger_sequence`  | `soroban_events_appearances.ledger_sequence` | (task 0132 index)  | indexer        | OK                                      |
| `successful`       | `transactions.successful`                    | no                 | indexer        | OK                                      |
| `amount`           | `soroban_events_appearances.amount`          | no                 | indexer        | OK                                      |
| `created_at`       | `soroban_events_appearances.created_at`      | (pagination index) | indexer        | OK                                      |
| `event_type`       | type-2 archive (XDR fetch per appearance)    | n/a                | type-2 archive | OK (computed in handler, not persisted) |
| `topics`           | type-2 archive                               | n/a                | type-2 archive | OK                                      |
| `data`             | type-2 archive                               | n/a                | type-2 archive | OK                                      |

`event_type` / `topics` / `data` deliberately not persisted — ADR 0029 / `runtime_enrichment::stellar_archive`. Per-appearance one-shot fetch, cached at API process level.

---

## 3. Findings

| #   | Severity               | Endpoint(s) impacted  | Description                                                                                                                                                                                                                                                                                                                                                                                                                             | Suggested owner          | Spawn?                                                                                              |
| --- | ---------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ | --------------------------------------------------------------------------------------------------- |
| F1  | High                   | `/v1/assets`          | `assets.total_supply` and `assets.holder_count` ~100 % NULL on classic-credit rows. Root cause: Bug #1 seed workaround populated `assets` from `account_balances_current` without recomputing aggregates. Indexer fix (Stanisław) restores both fields on natural ingest of trustline/payment events.                                                                                                                                   | indexer                  | already covered by Bug #1 + #4 fix paths (Stanisław); record as "verified, not regressed by audit". |
| F2  | High                   | `/v1/nfts`            | `nfts.minted_at_ledger` 99 % NULL. Indexer only writes this on mint events; rows created via transfer-event observation (incl. Bug #3 false positives) never see a mint. UI may show "minted at —" for the majority of listed NFTs.                                                                                                                                                                                                     | indexer / classifier     | candidate follow-up — flag in §4.                                                                   |
| F3  | Medium                 | `/v1/nfts`            | `nfts.current_owner_id` 43 % NULL. Either real (burned tokens) or coverage gap from missing observed transfers in window. Needs decision: is "no owner" a legitimate render state, or always-bug?                                                                                                                                                                                                                                       | classifier / FE contract | candidate follow-up — flag in §4.                                                                   |
| F4  | ~~Doc-only~~ Retracted | `/v1/nfts`            | ~~`collection_name` declared indexer-owned in the schema migration comment, but in practice it's only written by L2 nft enrichment.~~ Re-checked during Step 3: `database-schema-overview.md:739` already attributes `collection_name` to "task 0195 §2d (Lambda 2)" — correct. The original migration `0005_tokens_nfts.sql` carries no ownership comment (neutral DDL), so there is nothing to fix. Step 1 finding was a false alarm. | n/a                      | no fix needed                                                                                       |
| F5  | Info                   | n/a                   | Three list endpoints referenced in task spec do not exist: `/v1/accounts`, `/v1/contracts`, `/v1/operations`. Their per-resource sub-list endpoints (`/:id/transactions`, `/:id/invocations`, `/:id/events`) cover the use cases. No FAIL — pure observation.                                                                                                                                                                           | n/a                      | none.                                                                                               |
| F6  | Info                   | `/v1/liquidity-pools` | `tvl` / `volume` / `fee_revenue` 100 % NULL — owned by task 0199, blocked on team-built price API. Re-verify after 0199 ships.                                                                                                                                                                                                                                                                                                          | 0199                     | none (already tracked).                                                                             |

---

## 4. Follow-up tasks (recommendation only, not spawned by Step 1)

Per task 0197 plan, FAILs spawn follow-up tasks per the 0210 pattern. Step 1 produces the recommendation list; the spawn is done in Step 6.

- **F2** — BUG candidate: `nfts.minted_at_ledger` populated only on mint events; ~99 % of NFT rows have it NULL because they originated from transfer-event observation (incl. false positives flagged by Bug #3). Decide whether the column is "best-effort indexer when mint observed, else NULL" (documented behaviour, fix is doc + UI fallback) or "must always be set, derive from earliest observed `nft_ownership` row" (fix is migration + backfill).
- **F3** — RESEARCH or BUG: decide whether `nfts.current_owner_id IS NULL` is a real state (burned / never had an owner observed) or a coverage gap.
- **F4** — RETRACTED, no action required. Schema doc already correctly attributes `nfts.collection_name` to "task 0195 §2d (Lambda 2)" (`database-schema-overview.md:739`); original migration `0005_tokens_nfts.sql` is neutral DDL. Step 1 finding was a false alarm.

These are reviewed with Stanisław before spawning, given that Bugs #1-#4 are already in his queue and may overlap.

---

## 5. One-time live-smoke checks

Per task spec Step 1.8, a one-shot live verification of the two enrichment kinds. Persistent regression suite is owned by task 0212.

### 5.1 `sep1_assets` live-smoke

Sampled real enriched rows: `2026-05-13-fresh-sample-real-sep1-enrichments.txt` (first 20 of 603). Spot-check on `Asset asset_code='USDC', issuer=GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`:

| field      | value                                                        |
| ---------- | ------------------------------------------------------------ |
| `name`     | `USDC`                                                       |
| `icon_url` | `https://www.centre.io/images/usdc/usdc-icon-86074d9d49.png` |

Verified manually that the issuer's `stellar.toml` at `https://www.centre.io/.well-known/stellar.toml` returns matching `[[CURRENCIES]]` block. Live RPC + HTTP + parse + DB write path exercised end-to-end.

### 5.2 `nft_metadata` live-smoke

Bachini `SorobanNFT` contract `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`, token_id=1:

| field             | value                                      |
| ----------------- | ------------------------------------------ |
| `name`            | `SorobanNFT`                               |
| `media_url`       | (NULL — fixture JSON has no `image` field) |
| `collection_name` | (NULL — fixture JSON has no `collection`)  |

Verified end-to-end: Soroban RPC `simulateTransaction` → `token_uri()` zero-arg (SEP-39 fallback after Bug #5 fix) → IPFS gateway fetch → JSON parse → DB write. See `2026-05-13-pre-audit-finding-token-uri-signature-mismatch.md` for the full PRE/POST table.

Both enrichment kinds confirmed wired end-to-end against live external sources.

---

## 6. Schema cross-references

Indexes spot-checked at audit time against `crates/db/migrations/*.sql`:

| Table                      | Index                                                                   | Defined in                                              |
| -------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------- |
| `assets`                   | `idx_assets_type`                                                       | `crates/db/migrations/20251005_assets_initial.up.sql`   |
| `assets`                   | `idx_assets_code_trgm` (GIN)                                            | `crates/db/migrations/20260104_assets_code_trgm.up.sql` |
| `nfts`                     | `idx_nfts_collection`, `idx_nfts_collection_trgm`                       | per task 0195 §2d                                       |
| `nfts`                     | `idx_nfts_name_trgm` (GIN)                                              | per task 0195                                           |
| `nfts`                     | `idx_nfts_owner`                                                        | per task 0195                                           |
| `liquidity_pools`          | `idx_pools_asset_a`, `idx_pools_asset_b`, `idx_pools_created_at_ledger` | per task 0181                                           |
| `liquidity_pool_snapshots` | `idx_lps_tvl` (DESC)                                                    | per task 0199 prep                                      |
| `transactions`             | `idx_tx_keyset` (created_at, id)                                        | per task 0132                                           |
| `operations_appearances`   | `idx_ops_app_pool` (partial)                                            | per task 0181                                           |
| `ledgers`                  | `idx_ledgers_closed_at`                                                 | `crates/db/migrations/20251001_ledgers_initial.up.sql`  |
| `lp_positions`             | `idx_lpp_shares` (partial WHERE shares > 0)                             | per task 0181                                           |

No missing index for any list endpoint's primary sort key.

---

## 7. Step 2 — Detail-endpoint anti-pattern sweep

Detail endpoints under `crates/api/src/{module}/handlers.rs` exposing `GET /v1/<resource>/:id` (single object). For each, audit:

- (a) **unique-to-detail fields** — present in detail DTO but not in list DTO
- (b) anti-pattern: stored DB column that is read only by the detail endpoint (drop candidate)
- (c) correct runtime-fetch pattern: detail-only field served via `runtime_enrichment` (ADR 0029 umbrella) — no DB column

### 7.1 Detail endpoints inventory

| Endpoint                                   | DTO                                                                                                            | SQL                                | Handler                                                                                          |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------ |
| `GET /v1/assets/:id`                       | [crates/api/src/assets/dto.rs:49](crates/api/src/assets/dto.rs:49) `AssetDetailResponse`                       | `09_get_assets_by_id.sql`          | [crates/api/src/assets/handlers.rs:152](crates/api/src/assets/handlers.rs:152)                   |
| `GET /v1/transactions/:hash`               | [crates/api/src/transactions/dto.rs:65](crates/api/src/transactions/dto.rs:65) `TransactionDetailLight`        | `03_get_transactions_by_hash.sql`  | [crates/api/src/transactions/handlers.rs:161](crates/api/src/transactions/handlers.rs:161)       |
| `GET /v1/ledgers/:sequence`                | [crates/api/src/ledgers/dto.rs:45](crates/api/src/ledgers/dto.rs:45) `LedgerDetailResponse`                    | `05_get_ledgers_by_sequence.sql`   | [crates/api/src/ledgers/handlers.rs:115](crates/api/src/ledgers/handlers.rs:115)                 |
| `GET /v1/accounts/:account_id`             | [crates/api/src/accounts/dto.rs:23](crates/api/src/accounts/dto.rs:23) `AccountDetailResponse`                 | `06_get_accounts_by_id.sql`        | [crates/api/src/accounts/handlers.rs:40](crates/api/src/accounts/handlers.rs:40)                 |
| `GET /v1/contracts/:contract_id`           | [crates/api/src/contracts/dto.rs:16](crates/api/src/contracts/dto.rs:16) `ContractDetailResponse`              | `11_get_contracts_by_id.sql`       | [crates/api/src/contracts/handlers.rs:129](crates/api/src/contracts/handlers.rs:129)             |
| `GET /v1/contracts/:contract_id/interface` | [crates/api/src/contracts/dto.rs:37](crates/api/src/contracts/dto.rs:37) `InterfaceResponse`                   | `12_get_contracts_interface.sql`   | `crates/api/src/contracts/handlers.rs:get_interface`                                             |
| `GET /v1/nfts/:id`                         | [crates/api/src/nfts/dto.rs:58](crates/api/src/nfts/dto.rs:58) `NftDetailResponse`                             | `16_get_nfts_by_id.sql`            | [crates/api/src/nfts/handlers.rs:111](crates/api/src/nfts/handlers.rs:111)                       |
| `GET /v1/liquidity-pools/:id`              | [crates/api/src/liquidity_pools/dto.rs:97](crates/api/src/liquidity_pools/dto.rs:97) `PoolItem` (same as list) | `19_get_liquidity_pools_by_id.sql` | [crates/api/src/liquidity_pools/handlers.rs:149](crates/api/src/liquidity_pools/handlers.rs:149) |
| `GET /v1/liquidity-pools/:id/chart`        | [crates/api/src/liquidity_pools/dto.rs:184](crates/api/src/liquidity_pools/dto.rs:184) `ChartResponse`         | `21_get_liquidity_pools_chart.sql` | `crates/api/src/liquidity_pools/handlers.rs:get_pool_chart`                                      |
| `GET /v1/search`                           | [crates/api/src/search/dto.rs:18](crates/api/src/search/dto.rs:18) `SearchResponse`                            | `22_get_search.sql`                | `crates/api/src/search/handlers.rs:get_search`                                                   |

### 7.2 Unique-to-detail fields + verdict

#### `/v1/assets/:id`

| Detail-only field    | DB column?                                                             | Implementation                                                                                                    | Verdict                                                                                                 |
| -------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `deployed_at_ledger` | `soroban_contracts.deployed_at_ledger` (JOIN via `assets.contract_id`) | SQL JOIN                                                                                                          | OK — on-chain field, indexer-driven, NULL for non-SAC. Used by `/v1/search` too — legitimately indexed. |
| `description`        | **DROPPED** (`20260424000000_drop_assets_sep1_detail_cols.up.sql`)     | Runtime SEP-1 fetch via `runtime_enrichment::sep1` ([handlers.rs:185-198](crates/api/src/assets/handlers.rs:185)) | OK ✓ — ADR 0043 carve-out (task 0188).                                                                  |
| `home_page`          | **DROPPED** (same migration)                                           | Same                                                                                                              | OK ✓                                                                                                    |

#### `/v1/transactions/:hash`

| Detail-only field                            | DB column?                                                                       | Implementation                       | Verdict                                                                                                                               |
| -------------------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `parse_error`                                | `transactions.parse_error` BOOLEAN (migration 0003)                              | indexer-written on XDR parse failure | OK — parser metadata, not off-chain enrichment. Tested in [task 0190](lore/1-tasks/active/0190_FEATURE_parse-error-test-coverage.md). |
| `operations[]`                               | `operations_appearances` (indexed appearance rows) + archive XDR for full detail | SQL fetch + type-2 archive merge     | OK — appearance rows are list-level index, archive provides heavy fields (ADR 0029). Not a drop candidate.                            |
| `participants[]`                             | DB fallback + archive (heavy)                                                    | type-2 archive merge                 | OK — fallback only when archive unavailable.                                                                                          |
| `soroban_events[]` / `soroban_invocations[]` | DB appearance rows + archive (ADR 0033 / 0034)                                   | type-2 archive merge                 | OK                                                                                                                                    |

#### `/v1/ledgers/:sequence`

| Detail-only field                 | DB column?                           | Implementation      | Verdict                                                   |
| --------------------------------- | ------------------------------------ | ------------------- | --------------------------------------------------------- |
| `prev_sequence` / `next_sequence` | computed via LATERAL on `ledgers` PK | SQL                 | OK — no storage, free navigation field.                   |
| `transactions[]`                  | embedded paginated sub-list          | SQL via statement B | OK — same shape as `/v1/transactions` filtered by ledger. |

#### `/v1/accounts/:account_id`

| Detail-only field | DB column?                                           | Implementation  | Verdict                                                                      |
| ----------------- | ---------------------------------------------------- | --------------- | ---------------------------------------------------------------------------- |
| `sequence_number` | `accounts.sequence_number`                           | indexer-written | OK — on-chain field, can't be runtime-fetched cheaply (Horizon dependency).  |
| `home_domain`     | `accounts.home_domain`                               | indexer-written | OK — also used by `/v1/assets/:id` SEP-1 fetch as lookup key. Multi-purpose. |
| `balances[]`      | `account_balances_current` (materialized per ledger) | SQL fetch       | OK — list-shape but per-account, served only via detail.                     |

#### `/v1/contracts/:contract_id`

| Detail-only field                                                        | DB column?                                                                              | Implementation                 | Verdict                                                                                          |
| ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------ |
| `wasm_hash`, `wasm_uploaded_at_ledger`, `deployer`, `deployed_at_ledger` | `soroban_contracts.*`                                                                   | indexer-written at deploy time | OK — written once, immutable. Runtime fetch would re-read archive every request. Cheap to store. |
| `contract_type` / `contract_type_name`, `is_sac`                         | `soroban_contracts.contract_type` / `is_sac` (CASE for `_name`)                         | indexer (WASM classifier)      | OK                                                                                               |
| `stats` (recent invocations + unique callers)                            | computed by statement B aggregating `soroban_invocations_appearances` over 7-day window | SQL                            | OK — computed, not stored.                                                                       |

#### `/v1/contracts/:contract_id/interface`

| Detail-only field            | DB column?                         | Implementation             | Verdict                                                                                                                                                                                                                                                   |
| ---------------------------- | ---------------------------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `interface_metadata` (JSONB) | `wasm_interface_metadata.metadata` | indexer-written (per WASM) | OK — see F7 below. NOT detail-only despite single API consumer today: shared lookup table (NFT classifier has planned future use per [enrichment-shared/nft_token_uri/client.rs:258-275](crates/enrichment-shared/src/nft_token_uri/client.rs:258) TODO). |

#### `/v1/nfts/:id`

| Detail-only field  | DB column?                                               | Implementation                                                                                                                  | Verdict                                                                                       |
| ------------------ | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `metadata` (JSONB) | **DROPPED** (`20260507120000_drop_nfts_metadata.up.sql`) | Runtime Soroban RPC + IPFS via `runtime_enrichment::nft_token_uri` ([handlers.rs:142-169](crates/api/src/nfts/handlers.rs:142)) | OK ✓ — ADR 0043 carve-out (task 0195). 3s wall-clock timeout + 24h LRU cache, fail-soft NULL. |

#### `/v1/liquidity-pools/:id`

No unique-to-detail fields — `PoolItem` reused. Chart endpoint (§7.2 next) handles time-series.

#### `/v1/liquidity-pools/:id/chart`

| Detail-only field                       | DB column?                                          | Implementation         | Verdict                                             |
| --------------------------------------- | --------------------------------------------------- | ---------------------- | --------------------------------------------------- |
| `data[].tvl` / `volume` / `fee_revenue` | `liquidity_pool_snapshots.*` (currently 100 % NULL) | SQL bucket aggregation | DEFERRED → 0199 (same Finding F6 as list endpoint). |

#### `/v1/search`

Cross-resource discriminated response. Per-bucket fields are minimal (`entity_type`, `identifier`, `label`, `surrogate_id`). No detail-only DB columns. OK.

### 7.3 UI-fallback contracts (per §2a 0195 + §6 0188 patterns)

These are **frontend rendering contracts** the API doesn't enforce: when the API returns NULL or `''` on a field, the FE has a specific fallback. Documented here so the contract stays visible at the API↔FE boundary.

| Endpoint                       | Field                            | API surface                             | Frontend fallback                                                       |
| ------------------------------ | -------------------------------- | --------------------------------------- | ----------------------------------------------------------------------- |
| `/v1/assets`, `/v1/assets/:id` | `name`                           | NULL (pending) / `''` (sentinel) / real | Render `asset_code` when NULL or `''` (0195 §2a)                        |
| `/v1/assets`, `/v1/assets/:id` | `icon_url`                       | NULL / `''` / real URL                  | Render generic placeholder icon when NULL or `''`                       |
| `/v1/assets/:id`               | `description`                    | NULL / real                             | Hide section when NULL (0188)                                           |
| `/v1/assets/:id`               | `home_page`                      | NULL / real                             | Hide link when NULL (0188)                                              |
| `/v1/nfts`, `/v1/nfts/:id`     | `name`                           | NULL / `''` / real                      | Render `"#{token_id}"` or contract StrKey snippet when NULL or `''`     |
| `/v1/nfts/:id`                 | `media_url`                      | NULL / `''` / real                      | Render placeholder image when NULL or `''`                              |
| `/v1/nfts/:id`                 | `metadata`                       | NULL / JSON object                      | Hide trait/attribute section when NULL (timeout / IPFS gateway failure) |
| `/v1/liquidity-pools`          | `tvl` / `volume` / `fee_revenue` | NULL today (0199 not shipped)           | Render `—` placeholder until 0199 ships                                 |

### 7.4 Findings (Step 2)

| #   | Severity | Endpoint                        | Description                                                                                                                                                                                                                                                                                                                                                                          | Verdict                                                     |
| --- | -------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| F7  | Low      | `/v1/contracts/:id/interface`   | `wasm_interface_metadata.metadata` JSONB exposed by single API consumer today (`/v1/contracts/:id/interface`). **Not** detail-only in system terms: shared lookup table that the NFT classifier has planned future use for (per [client.rs:258-275](crates/enrichment-shared/src/nft_token_uri/client.rs:258) TODO — pre-determine `token_uri` arity from interface spec).           | KEEP — multi-consumer (planned)                             |
| F8  | Medium   | `wasm_interface_metadata` table | Local DB: 10 rows total (10 distinct wasm_hashes from `soroban_contracts`), of which 4 have `{}` empty metadata and 6 have real `functions` array + `wasm_byte_len`. Empty `{}` rows = parser produced no spec from WASM bytecode. Means the future NFT-classifier dispatch is dead-on-arrival for 40 % of WASM-bearing contracts unless parser is improved.                         | docs (record limitation) + xdr-parser follow-up (Stanisław) |
| F9  | Medium   | `soroban_contracts.wasm_hash`   | 11 915 / 11 926 rows (99.9 %) have NULL `wasm_hash`. Mostly real (SAC contracts have no WASM by design — 6 SAC rows confirmed), but partly indexer gap: non-SAC contracts whose deploy event landed outside the audit window or were created from invocation observation without a deploy event seen. Same root cause class as Bug #4 (SAC detection misses pre-existing contracts). | indexer (Stanisław, Bug #4 fix path)                        |
| F10 | Low      | code comment                    | TODO in [client.rs:260-261](crates/enrichment-shared/src/nft_token_uri/client.rs:260) claims `wasm_interface_metadata.metadata` is "today **always NULL** per ~100 % of rows." Locally that's not accurate: 10/10 wasm-bearing-contract rows have a row (4 are `{}`, 6 have real specs). The bottleneck is upstream: `soroban_contracts.wasm_hash` is NULL for 99.9 % of contracts.  | docs (update comment in Step 3 docs refresh)                |

No drop candidates surfaced. The four already-dropped detail columns (`assets.description`, `assets.home_page`, `nfts.metadata`, and the older `assets.sep1_*` set) confirm the ADR 0043 carve-out has been applied consistently. Remaining detail-only DB columns (`transactions.parse_error`, `accounts.sequence_number`, `accounts.home_domain`, `soroban_contracts.wasm_*` family) are either parser metadata, on-chain protocol fields with no cheap runtime source, or written-once-immutable values where storage is cheaper than re-reading the archive per request.

### 7.5 Cross-check: list↔detail DTO consistency

For each resource, the detail DTO either **extends** the list DTO (`AssetDetailResponse` flattens `AssetItem`, `NftDetailResponse` flattens `NftItem`) or reuses it directly (`PoolItem` for `/v1/liquidity-pools/:id`). No detail endpoint **shrinks** below list shape (would be a UX bug). No detail endpoint adds a stored column that should be in list per ADR 0043.

---

## 8. Step 4 — ADR cross-check

Per task spec, four ADRs cross-checked against audit findings.

### 8.1 ADR 0043 — field allocation rule

**Verdict:** RE-AFFIRMED without amendment.

- Decision body (§Decision, §Rationale) is current and matches the empirical Step 1 mapping (11/11 list endpoints' field allocation traces correctly to indexer / Lambda 2 / SQL / type-2).
- Per-kind allocation matrix at §"Per-kind allocation matrix (informative)" (lines 169-187 of the ADR) lists every column-class encountered in Step 1 / 2. No entries need adding or correcting from the audit.
- Notes §"ADR 0029 boundary" already articulates the sibling boundary between this ADR (write-side type-1 / type-2) and 0029 (read-time XDR fetch). Audit found no docs / code confusion that would warrant re-stating.
- 0194 §1b / §1c population logic for `assets.total_supply` / `assets.holder_count` confirmed mapping to "List endpoint + on-chain → indexer" (Step 1 Finding F1 is a write-side gap, not an allocation-rule violation).

### 8.2 ADR 0029 — abandon parsed-ledger S3 artifacts, read-time XDR fetch

**Verdict:** NO AMENDMENT required.

Rationale (per task spec "no silent skip"): the umbrella view of `runtime_enrichment` (covering both `stellar_archive`, the original 0029 scope, and `sep1` / `nft_token_uri`, the off-chain detail-only siblings added by tasks 0188 / 0195) is already captured in two evergreen places:

1. **ADR 0043 Notes** explicitly draws the sibling boundary: "ADR 0029 covers the _read-time XDR fetch_ path (E3 / E14 heavy fields from S3). It is a sibling of this ADR's runtime type-2 case, sharing the in-process LRU + fail-soft pattern. ADR 0029 does not need amendment for type-1 write-side concerns; this ADR is the home for those."
2. **`backend-overview.md` §4.1** lists all three submodules (`stellar_archive`, `sep1`, `nft_token_uri`) under a single "they share the architectural shape (per-request, fail-soft, in-process LRU-cached)" framing — refreshed in this audit (Step 3) to add the `nft_token_uri` submodule and clarify that the API does Soroban RPC at detail time.

The umbrella concept is therefore canonically described at the architecture-doc layer (where any reader looking for the cross-component picture will land first) and pointed to from the field-allocation ADR. Amending 0029 itself would either duplicate that text or stretch the ADR's scope ("abandon parsed-ledger artifacts") into a general type-2 framework, neither of which improves clarity. 0188's original "Out of Scope" deferral ("until a unified description across both submodules is worth writing") is therefore answered: written, but in `backend-overview.md` + ADR 0043, not in 0029.

### 8.3 ADR 0037 — current schema snapshot

**Verdict:** History entry added recording post-anchor deltas (snapshot body unchanged per the 0038 / 0039 frozen-with-delta pattern).

Audit findings reconciled against the 0037 schema dump:

- 0194 introduced **no new columns**: `assets.total_supply` and `assets.holder_count` already exist in `0005_tokens_nfts.sql` (the original schema). 0194 only added the `recompute_asset_aggregates` population logic. No schema-snapshot amendment required.
- 0195 §2d **dropped** `nfts.metadata JSONB` via migration `20260507120000_drop_nfts_metadata.up.sql`. ADR 0037 §12 (line 388 in this snapshot) still shows the column — recorded as a post-anchor delta in the new history entry rather than rewriting the snapshot body (consistent with the 0038 / 0039 deltas-not-rewrites pattern fmazur left open).
- 0132 indexes (`20260428000100_add_endpoint_query_indexes.up.sql`) already covered by ADR 0039 delta.

History entry added at 2026-05-13 inside ADR 0037 frontmatter naming both deltas.

### 8.4 ADR 0044 — ClickHouse pilot, parallel store

**Verdict:** NO AMENDMENT required.

Audit scope is **Postgres-only** (Step 1 preamble; CH not wired to the API read-path). The ClickHouse parallel reference set under `endpoint-queries-clickhouse/` (task 0207) was acknowledged in Step 3 via a signpost in `database-schema-overview.md §7.2`. A CH-side equivalence audit is explicitly deferred to a follow-up once at least one `/v1/*` handler routes through ClickHouse; running it now would only verify the 0207 reference SQL against itself.

ADR 0044's "no indexer dual-write, no API read-path changes" framing is intact. No audit-surfaced fact would change its decision.

- **Indexer columns:** 11/11 endpoints pass with three findings (F1, F2, F3) where empirical NULL % deviates from "always populated"; F1 is a known Bug #1 artifact, F2 and F3 are new and recommended for spawn in Step 6.
- **L2 enrichment columns:** 4/4 wired (`assets.name`, `assets.icon_url`, `nfts.name`, `nfts.media_url`; `nfts.collection_name` is also L2 nft, already correctly attributed in the schema doc). All flip NULL → sentinel|populated on drain.
- **SQL-computed columns:** 6 fields (`asset_type_name`, `fee_percent`, `operation_types`, `contract_ids`, `from_account` LEAD, `share_percentage`) — all pass.
- **Type-2 archive columns:** 3 fields on `/v1/contracts/:id/events` (`event_type` / `topics` / `data`) — declared non-persisted, verified via handler trace.
- **Detail-endpoint anti-patterns:** 0 drop candidates. F7 (`wasm_interface_metadata.metadata`) flagged as "keep for now, type-2 archive option recorded." Four already-dropped columns (`assets.description`, `assets.home_page`, `nfts.metadata`, `assets.sep1_*` family) confirm ADR 0043 carve-out applied consistently.
