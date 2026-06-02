# Database Schema Snapshot

> Snapshot of `localhost:5432/soroban_block_explorer` taken on 2026-05-08.
> Source: live `psql \d+` introspection. Do not treat as evergreen — regenerate
> from a fresh DB dump when the schema changes.

- **PostgreSQL:** 16.13 (Alpine, x86_64)
- **Schema:** `public` (18 tables — 11 regular, 7 partitioned)
- **Extensions:** `plpgsql 1.0`, `pg_trgm 1.6`
- **Latest migration:** `20260507000000_operations_appearances_application_order`

## Migration history

| Version        | Description                              |
| -------------- | ---------------------------------------- |
| 1              | extensions                               |
| 2              | identity and ledgers                     |
| 3              | transactions and operations              |
| 4              | soroban activity                         |
| 5              | tokens nfts                              |
| 6              | liquidity pools                          |
| 7              | account balances                         |
| 20260421000000 | transactions hash unique                 |
| 20260421000100 | replay safe uniques                      |
| 20260422000000 | enum label functions                     |
| 20260422000100 | contract type add nft fungible           |
| 20260424000000 | drop assets sep1 detail cols             |
| 20260427000000 | sac identity native allowance            |
| 20260428000000 | seed native asset singleton              |
| 20260428000100 | add endpoint query indexes               |
| 20260430000000 | invocations caller contract              |
| 20260505130000 | soroban contracts typed name column      |
| 20260507000000 | operations appearances application order |

## Table index

| Table                                                               | Type                           | PK                                                           |
| ------------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------ |
| [accounts](#accounts)                                               | regular                        | `(id)`                                                       |
| [account_balances_current](#account_balances_current)               | regular                        | composite uniques (no formal PK)                             |
| [assets](#assets)                                                   | regular                        | `(id)`                                                       |
| [ledgers](#ledgers)                                                 | regular                        | `(sequence)`                                                 |
| [liquidity_pools](#liquidity_pools)                                 | regular                        | `(pool_id)`                                                  |
| [liquidity_pool_snapshots](#liquidity_pool_snapshots)               | partitioned (RANGE created_at) | `(id, created_at)`                                           |
| [lp_positions](#lp_positions)                                       | regular                        | `(pool_id, account_id)`                                      |
| [nfts](#nfts)                                                       | regular                        | `(id)`                                                       |
| [nft_ownership](#nft_ownership)                                     | partitioned (RANGE created_at) | `(nft_id, created_at, ledger_sequence, event_order)`         |
| [operations_appearances](#operations_appearances)                   | partitioned (RANGE created_at) | `(id, created_at)`                                           |
| [soroban_contracts](#soroban_contracts)                             | regular                        | `(id)`                                                       |
| [soroban_events_appearances](#soroban_events_appearances)           | partitioned (RANGE created_at) | `(contract_id, transaction_id, ledger_sequence, created_at)` |
| [soroban_invocations_appearances](#soroban_invocations_appearances) | partitioned (RANGE created_at) | `(contract_id, transaction_id, ledger_sequence, created_at)` |
| [transactions](#transactions)                                       | partitioned (RANGE created_at) | `(id, created_at)`                                           |
| [transaction_hash_index](#transaction_hash_index)                   | regular                        | `(hash)`                                                     |
| [transaction_participants](#transaction_participants)               | partitioned (RANGE created_at) | `(account_id, created_at, transaction_id)`                   |
| [wasm_interface_metadata](#wasm_interface_metadata)                 | regular                        | `(wasm_hash)`                                                |
| [\_sqlx_migrations](#_sqlx_migrations)                              | regular                        | `(version)`                                                  |

> **Partitioning note:** every partitioned table currently has **0 partitions**
> (a default partition is created and managed by the `db-partition-mgmt`
> Lambda — task 0139). The `RANGE (created_at)` partition key is shared across
> all partitioned tables so they can be co-pruned by date.

---

## accounts

Stellar accounts (G-addresses). Central reference table, FK target for almost
every other table that mentions an address.

| Column              | Type           | Nullable | Default                      | Notes             |
| ------------------- | -------------- | -------- | ---------------------------- | ----------------- |
| `id`                | `bigint`       | NOT NULL | `nextval('accounts_id_seq')` | surrogate PK      |
| `account_id`        | `varchar(56)`  | NOT NULL |                              | G-address, unique |
| `first_seen_ledger` | `bigint`       | NOT NULL |                              |                   |
| `last_seen_ledger`  | `bigint`       | NOT NULL |                              |                   |
| `sequence_number`   | `bigint`       | NOT NULL |                              |                   |
| `home_domain`       | `varchar(256)` |          |                              |                   |

**Indexes**

- `accounts_pkey` PRIMARY KEY btree (`id`)
- `accounts_account_id_key` UNIQUE btree (`account_id`)
- `idx_accounts_last_seen` btree (`last_seen_ledger DESC`)
- `idx_accounts_prefix` btree (`account_id text_pattern_ops`)

**Referenced by:** `account_balances_current.account_id`, `account_balances_current.issuer_id`, `assets.issuer_id`, `liquidity_pools.asset_a_issuer_id`, `liquidity_pools.asset_b_issuer_id`, `lp_positions.account_id`, `nft_ownership.owner_id`, `nfts.current_owner_id`, `operations_appearances.{source,destination,asset_issuer}_id`, `soroban_contracts.deployer_id`, `soroban_invocations_appearances.caller_id`, `transaction_participants.account_id`, `transactions.source_id`.

---

## account_balances_current

Current balance per (account, asset) — overwrites on every change. Does not
have a formal `PRIMARY KEY`; uniqueness is enforced via two partial indexes
(native vs credit).

| Column                | Type            | Nullable | Notes                                               |
| --------------------- | --------------- | -------- | --------------------------------------------------- |
| `account_id`          | `bigint`        | NOT NULL | FK → `accounts.id`                                  |
| `asset_type`          | `smallint`      | NOT NULL | range 0..15                                         |
| `asset_code`          | `varchar(12)`   | nullable | NULL only when `asset_type = 0`                     |
| `issuer_id`           | `bigint`        | nullable | FK → `accounts.id`, NULL only when `asset_type = 0` |
| `balance`             | `numeric(28,7)` | NOT NULL |                                                     |
| `last_updated_ledger` | `bigint`        | NOT NULL |                                                     |

**Indexes**

- `idx_abc_asset` btree (`asset_code, issuer_id`) WHERE `asset_code IS NOT NULL`
- `uidx_abc_credit` UNIQUE btree (`account_id, asset_code, issuer_id`) WHERE `asset_type <> 0`
- `uidx_abc_native` UNIQUE btree (`account_id`) WHERE `asset_type = 0`

**Check constraints**

- `ck_abc_asset_type_range`: `0 <= asset_type <= 15`
- `ck_abc_native`: native ⇒ `asset_code IS NULL AND issuer_id IS NULL`; non-native ⇒ both `NOT NULL`

**FKs:** `account_id → accounts(id)`, `issuer_id → accounts(id)`.

---

## assets

Catalog of every asset observed on-chain (native, credit, SAC-wrapped, pure
Soroban). Identity is enforced by partial unique indexes per asset_type.

| Column         | Type            | Nullable | Default                    | Notes                                     |
| -------------- | --------------- | -------- | -------------------------- | ----------------------------------------- |
| `id`           | `integer`       | NOT NULL | `nextval('assets_id_seq')` | PK                                        |
| `asset_type`   | `smallint`      | NOT NULL |                            | 0=native, 1=credit, 2=SAC, 3=pure-soroban |
| `asset_code`   | `varchar(12)`   | nullable |                            |                                           |
| `issuer_id`    | `bigint`        | nullable |                            | FK → `accounts.id`                        |
| `contract_id`  | `bigint`        | nullable |                            | FK → `soroban_contracts.id`               |
| `name`         | `varchar(256)`  | nullable |                            |                                           |
| `total_supply` | `numeric(28,7)` | nullable |                            |                                           |
| `holder_count` | `integer`       | nullable |                            |                                           |
| `icon_url`     | `varchar(1024)` | nullable |                            |                                           |

**Indexes**

- `assets_pkey` PRIMARY KEY btree (`id`)
- `idx_assets_code_trgm` gin (`asset_code gin_trgm_ops`) — fuzzy search
- `idx_assets_type` btree (`asset_type`)
- `uidx_assets_classic_asset` UNIQUE (`asset_code, issuer_id`) WHERE `asset_type IN (1, 2)`
- `uidx_assets_native` UNIQUE (`asset_type`) WHERE `asset_type = 0` — singleton native
- `uidx_assets_soroban` UNIQUE (`contract_id`) WHERE `asset_type IN (2, 3)`

**Check constraints**

- `ck_assets_asset_type_range`: `0 <= asset_type <= 15`
- `ck_assets_identity`: per-type column nullness rules (native ⇒ everything NULL; credit ⇒ code+issuer; SAC ⇒ contract + (code+issuer or both NULL); pure-soroban ⇒ contract only)

**FKs:** `contract_id → soroban_contracts(id)`, `issuer_id → accounts(id)`.

---

## ledgers

Header info for every closed ledger.

| Column              | Type          | Nullable | Notes            |
| ------------------- | ------------- | -------- | ---------------- |
| `sequence`          | `bigint`      | NOT NULL | PK               |
| `hash`              | `bytea`       | NOT NULL | unique, 32 bytes |
| `closed_at`         | `timestamptz` | NOT NULL |                  |
| `protocol_version`  | `integer`     | NOT NULL |                  |
| `transaction_count` | `integer`     | NOT NULL |                  |
| `base_fee`          | `bigint`      | NOT NULL |                  |

**Indexes**

- `ledgers_pkey` PRIMARY KEY btree (`sequence`)
- `idx_ledgers_closed_at` btree (`closed_at DESC`)
- `ledgers_hash_key` UNIQUE btree (`hash`)

**Check:** `ck_ledgers_hash_len`: `octet_length(hash) = 32`

---

## liquidity_pools

Static metadata for each Stellar AMM pool (constant product or stable). Pool
state lives in `liquidity_pool_snapshots`.

| Column              | Type          | Nullable | Notes              |
| ------------------- | ------------- | -------- | ------------------ |
| `pool_id`           | `bytea`       | NOT NULL | PK, 32 bytes       |
| `asset_a_type`      | `smallint`    | NOT NULL | range 0..15        |
| `asset_a_code`      | `varchar(12)` | nullable |                    |
| `asset_a_issuer_id` | `bigint`      | nullable | FK → `accounts.id` |
| `asset_b_type`      | `smallint`    | NOT NULL | range 0..15        |
| `asset_b_code`      | `varchar(12)` | nullable |                    |
| `asset_b_issuer_id` | `bigint`      | nullable | FK → `accounts.id` |
| `fee_bps`           | `integer`     | NOT NULL |                    |
| `created_at_ledger` | `bigint`      | NOT NULL |                    |

**Indexes**

- `liquidity_pools_pkey` PRIMARY KEY btree (`pool_id`)
- `idx_pools_asset_a` btree (`asset_a_code, asset_a_issuer_id`)
- `idx_pools_asset_b` btree (`asset_b_code, asset_b_issuer_id`)
- `idx_pools_created_at_ledger` btree (`created_at_ledger DESC, pool_id DESC`)

**Checks:** asset-type ranges + `octet_length(pool_id) = 32`

**Referenced by:** `operations_appearances.pool_id`, `liquidity_pool_snapshots.pool_id`, `lp_positions.pool_id`.

---

## liquidity_pool_snapshots

Per-ledger snapshot of pool state. Partitioned by `created_at`.

| Column            | Type            | Nullable | Default                                      | Notes                                    |
| ----------------- | --------------- | -------- | -------------------------------------------- | ---------------------------------------- |
| `id`              | `bigint`        | NOT NULL | `nextval('liquidity_pool_snapshots_id_seq')` |                                          |
| `pool_id`         | `bytea`         | NOT NULL |                                              | FK → `liquidity_pools.pool_id`, 32 bytes |
| `ledger_sequence` | `bigint`        | NOT NULL |                                              |                                          |
| `reserve_a`       | `numeric(28,7)` | NOT NULL |                                              |                                          |
| `reserve_b`       | `numeric(28,7)` | NOT NULL |                                              |                                          |
| `total_shares`    | `numeric(28,7)` | NOT NULL |                                              |                                          |
| `tvl`             | `numeric(28,7)` | nullable |                                              |                                          |
| `volume`          | `numeric(28,7)` | nullable |                                              |                                          |
| `fee_revenue`     | `numeric(28,7)` | nullable |                                              |                                          |
| `created_at`      | `timestamptz`   | NOT NULL |                                              | partition key                            |

**Partitioning:** RANGE (`created_at`), 0 partitions currently provisioned.

**Indexes**

- `liquidity_pool_snapshots_pkey` PRIMARY KEY (`id, created_at`)
- `idx_lps_pool` btree (`pool_id, created_at DESC`)
- `idx_lps_tvl` btree (`tvl DESC`) WHERE `tvl IS NOT NULL`
- `uq_lp_snapshots_pool_ledger` UNIQUE (`pool_id, ledger_sequence, created_at`)

**Check:** `octet_length(pool_id) = 32`. **FK:** `pool_id → liquidity_pools(pool_id)`.

---

## lp_positions

Per-account holdings of LP shares.

| Column                 | Type            | Nullable | Notes                                    |
| ---------------------- | --------------- | -------- | ---------------------------------------- |
| `pool_id`              | `bytea`         | NOT NULL | FK → `liquidity_pools.pool_id`, 32 bytes |
| `account_id`           | `bigint`        | NOT NULL | FK → `accounts.id`                       |
| `shares`               | `numeric(28,7)` | NOT NULL |                                          |
| `first_deposit_ledger` | `bigint`        | NOT NULL |                                          |
| `last_updated_ledger`  | `bigint`        | NOT NULL |                                          |

**Indexes**

- `lp_positions_pkey` PRIMARY KEY btree (`pool_id, account_id`)
- `idx_lpp_shares` btree (`pool_id, shares DESC`) WHERE `shares > 0`

**Check:** `octet_length(pool_id) = 32`.

---

## nfts

NFT catalog — one row per (contract, token_id) pair.

| Column                 | Type           | Nullable | Default                  | Notes                       |
| ---------------------- | -------------- | -------- | ------------------------ | --------------------------- |
| `id`                   | `integer`      | NOT NULL | `nextval('nfts_id_seq')` | PK                          |
| `contract_id`          | `bigint`       | NOT NULL |                          | FK → `soroban_contracts.id` |
| `token_id`             | `varchar(256)` | NOT NULL |                          |                             |
| `collection_name`      | `varchar(256)` | nullable |                          |                             |
| `name`                 | `varchar(256)` | nullable |                          |                             |
| `media_url`            | `text`         | nullable |                          |                             |
| `metadata`             | `jsonb`        | nullable |                          |                             |
| `minted_at_ledger`     | `bigint`       | nullable |                          |                             |
| `current_owner_id`     | `bigint`       | nullable |                          | FK → `accounts.id`          |
| `current_owner_ledger` | `bigint`       | nullable |                          |                             |

**Indexes**

- `nfts_pkey` PRIMARY KEY btree (`id`)
- `idx_nfts_collection` btree (`collection_name`)
- `idx_nfts_collection_trgm` gin (`collection_name gin_trgm_ops`)
- `idx_nfts_name_trgm` gin (`name gin_trgm_ops`)
- `idx_nfts_owner` btree (`current_owner_id`)
- `nfts_contract_id_token_id_key` UNIQUE (`contract_id, token_id`)

---

## nft_ownership

Append-only history of NFT mint/transfer/burn events. Partitioned by `created_at`.

| Column            | Type          | Nullable | Notes                                                 |
| ----------------- | ------------- | -------- | ----------------------------------------------------- |
| `nft_id`          | `integer`     | NOT NULL | FK → `nfts.id` ON DELETE CASCADE                      |
| `transaction_id`  | `bigint`      | NOT NULL | FK → `transactions(id, created_at)` ON DELETE CASCADE |
| `owner_id`        | `bigint`      | nullable | FK → `accounts.id` (NULL on burn)                     |
| `event_type`      | `smallint`    | NOT NULL | range 0..15                                           |
| `ledger_sequence` | `bigint`      | NOT NULL |                                                       |
| `event_order`     | `smallint`    | NOT NULL |                                                       |
| `created_at`      | `timestamptz` | NOT NULL | partition key                                         |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `nft_ownership_pkey` PRIMARY KEY (`nft_id, created_at, ledger_sequence, event_order`)

**Check:** `0 <= event_type <= 15`.

---

## operations_appearances

Folded operation index — one row per (transaction, role, asset/contract/pool)
appearance. Drives `/accounts/{id}/operations` and `/contracts/{id}/operations`.
Partitioned by `created_at`.

| Column              | Type          | Nullable | Default                                    | Notes                                                 |
| ------------------- | ------------- | -------- | ------------------------------------------ | ----------------------------------------------------- |
| `id`                | `bigint`      | NOT NULL | `nextval('operations_appearances_id_seq')` |                                                       |
| `transaction_id`    | `bigint`      | NOT NULL |                                            | FK → `transactions(id, created_at)` ON DELETE CASCADE |
| `type`              | `smallint`    | NOT NULL |                                            | range 0..127                                          |
| `source_id`         | `bigint`      | nullable |                                            | FK → `accounts.id`                                    |
| `destination_id`    | `bigint`      | nullable |                                            | FK → `accounts.id`                                    |
| `contract_id`       | `bigint`      | nullable |                                            | FK → `soroban_contracts.id`                           |
| `asset_code`        | `varchar(12)` | nullable |                                            |                                                       |
| `asset_issuer_id`   | `bigint`      | nullable |                                            | FK → `accounts.id`                                    |
| `pool_id`           | `bytea`       | nullable |                                            | FK → `liquidity_pools.pool_id`, 32 bytes              |
| `amount`            | `bigint`      | NOT NULL |                                            | > 0                                                   |
| `ledger_sequence`   | `bigint`      | NOT NULL |                                            |                                                       |
| `created_at`        | `timestamptz` | NOT NULL |                                            | partition key                                         |
| `application_order` | `smallint`    | nullable |                                            | range 1..32767                                        |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `operations_appearances_pkey` PRIMARY KEY (`id, created_at`)
- `idx_ops_app_asset` btree (`asset_code, asset_issuer_id, created_at DESC`) WHERE `asset_code IS NOT NULL`
- `idx_ops_app_contract` btree (`contract_id, created_at DESC`) WHERE `contract_id IS NOT NULL`
- `idx_ops_app_destination` btree (`destination_id, created_at DESC`) WHERE `destination_id IS NOT NULL`
- `idx_ops_app_pool` btree (`pool_id, created_at DESC`) WHERE `pool_id IS NOT NULL`
- `idx_ops_app_source` btree (`source_id, created_at DESC`) WHERE `source_id IS NOT NULL`
- `idx_ops_app_type` btree (`type, created_at DESC`)
- `uq_ops_app_identity` UNIQUE (`transaction_id, type, source_id, destination_id, contract_id, asset_code, asset_issuer_id, pool_id, ledger_sequence, created_at`) NULLS NOT DISTINCT

**Checks**

- `amount > 0`
- `application_order IS NULL OR 1 <= application_order <= 32767`
- `pool_id IS NULL OR octet_length(pool_id) = 32`
- `0 <= type <= 127`

---

## soroban_contracts

One row per Soroban contract — instance + (optional) WASM linkage + classification.

| Column                    | Type           | Nullable | Default                               | Notes                                                                |
| ------------------------- | -------------- | -------- | ------------------------------------- | -------------------------------------------------------------------- |
| `id`                      | `bigint`       | NOT NULL | `nextval('soroban_contracts_id_seq')` | PK                                                                   |
| `contract_id`             | `varchar(56)`  | NOT NULL |                                       | C-address, unique                                                    |
| `wasm_hash`               | `bytea`        | nullable |                                       | FK → `wasm_interface_metadata.wasm_hash`, 32 bytes                   |
| `wasm_uploaded_at_ledger` | `bigint`       | nullable |                                       |                                                                      |
| `deployer_id`             | `bigint`       | nullable |                                       | FK → `accounts.id`                                                   |
| `deployed_at_ledger`      | `bigint`       | nullable |                                       |                                                                      |
| `contract_type`           | `smallint`     | nullable |                                       | range 0..15                                                          |
| `is_sac`                  | `boolean`      | NOT NULL | `false`                               |                                                                      |
| `name`                    | `varchar(256)` | nullable |                                       |                                                                      |
| `search_vector`           | `tsvector`     | nullable | generated                             | `to_tsvector('simple', COALESCE(name,'') \|\| ' ' \|\| contract_id)` |

**Indexes**

- `soroban_contracts_pkey` PRIMARY KEY btree (`id`)
- `idx_contracts_prefix` btree (`contract_id text_pattern_ops`)
- `idx_contracts_search` gin (`search_vector`)
- `idx_contracts_type` btree (`contract_type`)
- `idx_contracts_wasm` btree (`wasm_hash`) WHERE `wasm_hash IS NOT NULL`
- `soroban_contracts_contract_id_key` UNIQUE (`contract_id`)

**Checks**

- `contract_type IS NULL OR 0 <= contract_type <= 15`
- `wasm_hash IS NULL OR octet_length(wasm_hash) = 32`

---

## soroban_events_appearances

Folded Soroban event index — `(contract_id, transaction_id, ledger_sequence, amount)`.
Currently powers `/contracts/{id}/events` (which round-trips to S3 for the
event payload — see ADR 0033 / abandoned task 0203 for the full-content
replacement design). Partitioned by `created_at`.

| Column            | Type          | Nullable | Notes                                                 |
| ----------------- | ------------- | -------- | ----------------------------------------------------- |
| `contract_id`     | `bigint`      | NOT NULL | FK → `soroban_contracts.id`                           |
| `transaction_id`  | `bigint`      | NOT NULL | FK → `transactions(id, created_at)` ON DELETE CASCADE |
| `ledger_sequence` | `bigint`      | NOT NULL |                                                       |
| `amount`          | `bigint`      | NOT NULL |                                                       |
| `created_at`      | `timestamptz` | NOT NULL | partition key                                         |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `soroban_events_appearances_pkey` PRIMARY KEY (`contract_id, transaction_id, ledger_sequence, created_at`)
- `idx_sea_contract_keyset` btree (`contract_id, created_at DESC, transaction_id DESC`)
- `idx_sea_contract_ledger` btree (`contract_id, ledger_sequence DESC, created_at DESC`)
- `idx_sea_transaction` btree (`transaction_id, created_at DESC`)

---

## soroban_invocations_appearances

Folded Soroban contract invocation index. Partitioned by `created_at`.

| Column               | Type          | Nullable | Notes                                                 |
| -------------------- | ------------- | -------- | ----------------------------------------------------- |
| `contract_id`        | `bigint`      | NOT NULL | FK → `soroban_contracts.id`                           |
| `transaction_id`     | `bigint`      | NOT NULL | FK → `transactions(id, created_at)` ON DELETE CASCADE |
| `ledger_sequence`    | `bigint`      | NOT NULL |                                                       |
| `caller_id`          | `bigint`      | nullable | FK → `accounts.id` (XOR with `caller_contract_id`)    |
| `amount`             | `integer`     | NOT NULL |                                                       |
| `created_at`         | `timestamptz` | NOT NULL | partition key                                         |
| `caller_contract_id` | `bigint`      | nullable | FK → `soroban_contracts.id` (XOR with `caller_id`)    |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `soroban_invocations_appearances_pkey` PRIMARY KEY (`contract_id, transaction_id, ledger_sequence, created_at`)
- `idx_sia_contract_keyset` btree (`contract_id, created_at DESC, transaction_id DESC`)
- `idx_sia_contract_ledger` btree (`contract_id, ledger_sequence DESC`)
- `idx_sia_transaction` btree (`transaction_id`)

**Check:** `ck_sia_caller_xor`: `caller_id IS NULL OR caller_contract_id IS NULL` (at most one set).

---

## transactions

Top-level transaction table. Partitioned by `created_at`. PK `(id, created_at)`
because `created_at` must appear in the PK on partitioned tables.

| Column              | Type          | Nullable | Default                          | Notes              |
| ------------------- | ------------- | -------- | -------------------------------- | ------------------ |
| `id`                | `bigint`      | NOT NULL | `nextval('transactions_id_seq')` |                    |
| `hash`              | `bytea`       | NOT NULL |                                  | 32 bytes           |
| `ledger_sequence`   | `bigint`      | NOT NULL |                                  |                    |
| `application_order` | `smallint`    | NOT NULL |                                  |                    |
| `source_id`         | `bigint`      | NOT NULL |                                  | FK → `accounts.id` |
| `fee_charged`       | `bigint`      | NOT NULL |                                  |                    |
| `inner_tx_hash`     | `bytea`       | nullable |                                  | 32 bytes when set  |
| `successful`        | `boolean`     | NOT NULL |                                  |                    |
| `operation_count`   | `smallint`    | NOT NULL |                                  |                    |
| `has_soroban`       | `boolean`     | NOT NULL | `false`                          |                    |
| `parse_error`       | `boolean`     | NOT NULL | `false`                          |                    |
| `created_at`        | `timestamptz` | NOT NULL |                                  | partition key      |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `transactions_pkey` PRIMARY KEY (`id, created_at`)
- `idx_tx_has_soroban` btree (`created_at DESC`) WHERE `has_soroban`
- `idx_tx_keyset` btree (`created_at DESC, id DESC`)
- `idx_tx_ledger` btree (`ledger_sequence`)
- `idx_tx_source_created` btree (`source_id, created_at DESC`)
- `uq_transactions_hash_created_at` UNIQUE (`hash, created_at`)

**Checks**

- `octet_length(hash) = 32`
- `inner_tx_hash IS NULL OR octet_length(inner_tx_hash) = 32`

**Referenced by (composite FK on `(transaction_id, created_at)`, all `ON DELETE CASCADE`):** `nft_ownership`, `operations_appearances`, `soroban_events_appearances`, `soroban_invocations_appearances`, `transaction_participants`.

---

## transaction_hash_index

Lookup-only mapping `hash → (ledger_sequence, created_at)` so the API can
locate a transaction's partition without scanning all of `transactions`.

| Column            | Type          | Nullable | Notes        |
| ----------------- | ------------- | -------- | ------------ |
| `hash`            | `bytea`       | NOT NULL | PK, 32 bytes |
| `ledger_sequence` | `bigint`      | NOT NULL |              |
| `created_at`      | `timestamptz` | NOT NULL |              |

**Indexes:** `transaction_hash_index_pkey` PRIMARY KEY btree (`hash`).

**Check:** `octet_length(hash) = 32`.

---

## transaction_participants

Many-to-many between transactions and the accounts that participated in any
operation. Partitioned by `created_at`.

| Column           | Type          | Nullable | Notes                                                 |
| ---------------- | ------------- | -------- | ----------------------------------------------------- |
| `transaction_id` | `bigint`      | NOT NULL | FK → `transactions(id, created_at)` ON DELETE CASCADE |
| `account_id`     | `bigint`      | NOT NULL | FK → `accounts.id`                                    |
| `created_at`     | `timestamptz` | NOT NULL | partition key                                         |

**Partitioning:** RANGE (`created_at`), 0 partitions.

**Indexes**

- `transaction_participants_pkey` PRIMARY KEY (`account_id, created_at, transaction_id`)
- `idx_tp_tx` btree (`transaction_id`)

---

## wasm_interface_metadata

Parsed Soroban contract interface (functions, events, types) — keyed on
WASM hash so multiple deployed contracts that share a WASM share metadata.

| Column      | Type    | Nullable | Notes        |
| ----------- | ------- | -------- | ------------ |
| `wasm_hash` | `bytea` | NOT NULL | PK, 32 bytes |
| `metadata`  | `jsonb` | NOT NULL |              |

**Indexes:** `wasm_interface_metadata_pkey` PRIMARY KEY btree (`wasm_hash`).

**Check:** `octet_length(wasm_hash) = 32`.

**Referenced by:** `soroban_contracts.wasm_hash`.

---

## \_sqlx_migrations

Internal table maintained by `sqlx` migrate. Listed for completeness — see
`migration history` table at the top.

| Column           | Type          | Nullable | Default |
| ---------------- | ------------- | -------- | ------- |
| `version`        | `bigint`      | NOT NULL |         |
| `description`    | `text`        | NOT NULL |         |
| `installed_on`   | `timestamptz` | NOT NULL | `now()` |
| `success`        | `boolean`     | NOT NULL |         |
| `checksum`       | `bytea`       | NOT NULL |         |
| `execution_time` | `bigint`      | NOT NULL |         |

**PK:** `(version)`.
