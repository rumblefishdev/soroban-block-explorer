-- Endpoint:     GET /search?q=&type=transaction,contract,asset,account,nft,pool
-- Purpose:      Unified search across all entity types. The API classifies
--               the query (hash-shape, StrKey-shape, plain text) and may
--               restrict via the `type` parameter; this SQL accepts the
--               classification + (optional) type allowlist as inputs and
--               returns grouped, capped result sets per entity type.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.15
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :q                 String           the raw user query (UTF-8)
--   $2  :hash_bytes        FixedString(32)  parsed 32-byte hash if hash-shaped
--                                            (or NULL); pass via unhex(hex)
--   $3  :strkey_prefix     String           StrKey or prefix (G…/C…), NULL otherwise
--   $4  :per_group_limit   Int              cap per entity bucket
--   $5..$10                Bool             include flags per entity type
-- Indexes:      transaction_hash_dict (Dictionary, RAM-bounded cache).
--               soroban_contracts ORDER BY (id) — full scan + prefix match.
--               assets ORDER BY (id) — full scan + ILIKE-style match.
--               accounts ORDER BY (id) — full scan + prefix match.
--               nfts ORDER BY (id) — full scan + ILIKE-style match.
--               liquidity_pools ORDER BY (pool_id) — exact-hex match.
-- CH Engine:    All state tables Replacing — FINAL.
-- CH Pattern:   single UNION ALL of 6 narrow scans, each capped LIMIT $4.
--                 Tx bucket uses dictGet for the hot path (E03 pattern).
--                 No pg_trgm available — see §R3 below.
-- ADR 0044 §:   §4.9 (transaction_hash_dict Dictionary, replaces PG hash idx),
--                 §4.5 (Replacing state tables), §5.5 (Dict replaces hash idx).
-- Notes:
--   • **R3 (task 0207 risk register): pg_trgm regression.** CH has no
--     trigram GIN equivalent. The PG implementation uses `gin_trgm_ops`
--     for case-insensitive substring search on `asset_code` and `nfts.name`.
--     CH-side, we fall back to `positionCaseInsensitiveUTF8(...) > 0` which
--     is a linear scan after FINAL dedup. Cost is acceptable for the pilot
--     because assets + nfts are small relative to fact tables. If CH-side
--     free-text becomes hot, the follow-up is a `tokenbf_v1` skip index on
--     `nfts.name` and `assets.name` — out of scope for task 0207 (docs-only,
--     no schema changes).
--   • Tx bucket uses `dictGet('transaction_hash_dict', 'ledger_sequence',
--     toString(unhex($1)))` for the hash-shaped query when $2 is non-NULL.
--     This is the canonical CH hot path (§5.5). The Dict returns 0 (default
--     Int64) on miss; we filter out 0 to avoid a phantom result.
--   • Contract bucket: when $3 is non-NULL (StrKey prefix), `startsWith()`
--     gives O(scan after FINAL) which is acceptable. When $3 is NULL,
--     fall back to `positionCaseInsensitiveUTF8` on `sc.name`. CH has no
--     pre-built tsvector equivalent for `search_vector` so we don't
--     replicate PG's tsvector branch in the pilot.
--   • Account bucket: prefix match via `startsWith(account_id, $3)`.
--   • Pool bucket: exact `pool_id = unhex($1)` when input is 32-byte hex
--     (PG uses $2 BYTEA; CH receives via `unhex($1)` when shape matches).
--   • Cursor/pagination: each bucket is `LIMIT $4`, no global cursor.
--     UI policy: clicking "more in X" reuses E02/E08/E15/E18 with type
--     filter pre-applied.
--   • `asset_type_name` PG helper unavailable — project raw Int16 in
--     asset bucket's `label`; API decodes.

SELECT entity_type, identifier, label, surrogate_id FROM (
    -- Transactions: dictGet hot path (§5.5). Use $2 (parsed hash bytes)
    -- — the API classifier accepts BOTH hex AND base64 inputs for the
    -- raw query $1, so `unhex($1)` would throw on a base64-shaped q.
    -- $2 carries the pre-parsed bytes regardless of the input encoding.
    -- Review feedback (Copilot PR #174).
    SELECT
        'transaction'                                                                       AS entity_type,
        lower(hex($2))                                                                      AS identifier,
        concat('ledger ', toString(dictGet('transaction_hash_dict', 'ledger_sequence', toString($2)))) AS label,
        CAST(NULL AS Nullable(Int64))                                                       AS surrogate_id
    WHERE $5 = true
      AND $2 IS NOT NULL
      AND dictGet('transaction_hash_dict', 'ledger_sequence', toString($2)) > 0
    LIMIT $4
) UNION ALL
SELECT entity_type, identifier, label, surrogate_id FROM (
    -- Contracts: StrKey prefix when $3 set, otherwise substring on name
    SELECT
        'contract'                                                                          AS entity_type,
        sc.contract_id                                                                      AS identifier,
        coalesce(sc.name, '')                                                               AS label,
        CAST(sc.id AS Nullable(Int64))                                                      AS surrogate_id
    FROM soroban_contracts sc FINAL
    WHERE $6 = true
      AND (
              ($3 IS NOT NULL AND startsWith(sc.contract_id, $3))
           OR ($3 IS NULL     AND positionCaseInsensitiveUTF8(coalesce(sc.name, ''), $1) > 0)
          )
    LIMIT $4
) UNION ALL
SELECT entity_type, identifier, label, surrogate_id FROM (
    -- Assets: substring on asset_code (CH no pg_trgm — linear scan).
    -- PR #175 dropped `assets.id` surrogate; project a synthetic
    -- cityHash64 over the natural 4-tuple as opaque routing key.
    SELECT
        'asset'                                                                             AS entity_type,
        if(length(a.asset_code) > 0, a.asset_code, 'XLM')                                   AS identifier,
        toString(a.asset_type)                                                              AS label,
        toInt64(cityHash64(toString(a.asset_type), a.asset_code,
                           toString(a.issuer_id), toString(a.contract_id)))                 AS surrogate_id
    FROM assets a FINAL
    WHERE $7 = true
      AND (
              (length(a.asset_code) > 0 AND positionCaseInsensitiveUTF8(a.asset_code, $1) > 0)
           OR (a.asset_type = 0 AND (lower($1) = 'xlm' OR lower($1) = 'native'))
          )
    LIMIT $4
) UNION ALL
SELECT entity_type, identifier, label, surrogate_id FROM (
    -- Accounts: StrKey prefix only
    SELECT
        'account'                                                                           AS entity_type,
        a.account_id                                                                        AS identifier,
        coalesce(a.home_domain, '')                                                         AS label,
        CAST(a.id AS Nullable(Int64))                                                       AS surrogate_id
    FROM accounts a FINAL
    WHERE $8 = true
      AND $3 IS NOT NULL
      AND startsWith(a.account_id, $3)
    LIMIT $4
) UNION ALL
SELECT entity_type, identifier, label, surrogate_id FROM (
    -- NFTs: substring on name (CH no pg_trgm — linear scan).
    -- PR #175 dropped `nfts.id` surrogate; project synthetic
    -- cityHash64(contract_id, token_id) as opaque routing key.
    SELECT
        'nft'                                                                               AS entity_type,
        coalesce(n.name, '')                                                                AS identifier,
        coalesce(n.collection_name, '')                                                     AS label,
        toInt64(cityHash64(toString(n.contract_id), n.token_id))                            AS surrogate_id
    FROM nfts n FINAL
    WHERE $9 = true
      AND n.name IS NOT NULL
      AND positionCaseInsensitiveUTF8(n.name, $1) > 0
    LIMIT $4
) UNION ALL
SELECT entity_type, identifier, label, surrogate_id FROM (
    -- Pools: exact-hex match on pool_id (32-byte BYTEA via $2)
    SELECT
        'pool'                                                                              AS entity_type,
        lower(hex(lp.pool_id))                                                              AS identifier,
        concat(coalesce(lp.asset_a_code, 'XLM'), ' / ', coalesce(lp.asset_b_code, 'XLM'))   AS label,
        CAST(NULL AS Nullable(Int64))                                                       AS surrogate_id
    FROM liquidity_pools lp
    WHERE $10 = true
      AND $2 IS NOT NULL
      AND lp.pool_id = $2
    LIMIT $4
);
