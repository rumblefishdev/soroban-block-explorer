# G — Orphan composition split queries (ClickHouse)

Ready-to-run when the DB is reachable. Columns verified against
`crates/db-clickhouse/schema/init.sql`. `soroban_contracts` is a
ReplacingMergeTree → use `FINAL`.

**Canonical orphan predicate:** `is_sac=false AND deployed_at_ledger IS NULL AND wasm_hash IS NULL`.

```sql
-- Q0  population + pending impact
WITH orphans AS (
  SELECT id FROM soroban_contracts FINAL
  WHERE is_sac = false AND deployed_at_ledger IS NULL AND wasm_hash IS NULL)
SELECT
  (SELECT count() FROM orphans)                                       AS orphan_contracts,
  (SELECT uniqExact(contract_id) FROM nfts_pending
     WHERE contract_id IN (SELECT id FROM orphans))                   AS orphans_with_pending,
  (SELECT count() FROM nfts_pending
     WHERE contract_id IN (SELECT id FROM orphans))                   AS pending_rows_held;
```

```sql
-- Q1  event-signature profile (no XDR decode; `signature` = event symbol)
SELECT e.signature AS event_sig, count() AS events, uniqExact(e.contract_id) AS distinct_orphans
FROM soroban_events e
WHERE e.contract_id IN (
  SELECT id FROM soroban_contracts FINAL
  WHERE is_sac = false AND deployed_at_ledger IS NULL AND wasm_hash IS NULL)
GROUP BY event_sig ORDER BY events DESC LIMIT 30;
```

```sql
-- Q2  phantom-caller detection (caller-only = bucket b)
WITH orphans AS (
  SELECT id FROM soroban_contracts FINAL
  WHERE is_sac = false AND deployed_at_ledger IS NULL AND wasm_hash IS NULL)
SELECT
  countIf(id IN (SELECT caller_contract_id FROM soroban_invocations_appearances)) AS as_caller,
  countIf(id IN (SELECT contract_id FROM soroban_events))                         AS as_emitter,
  countIf(id IN (SELECT caller_contract_id FROM soroban_invocations_appearances)
          AND id NOT IN (SELECT contract_id FROM soroban_events))                 AS caller_only_phantom
FROM orphans;
```

```sql
-- Q3  top offenders (is CDP5RUMSC7YJ… a caller or an emitter?)
SELECT sc.contract_id, count() AS pending_rows,
       sc.id IN (SELECT contract_id FROM soroban_events)                          AS is_emitter,
       sc.id IN (SELECT caller_contract_id FROM soroban_invocations_appearances)  AS is_caller
FROM nfts_pending np
INNER JOIN soroban_contracts sc FINAL ON sc.id = np.contract_id
WHERE sc.is_sac = false AND sc.deployed_at_ledger IS NULL AND sc.wasm_hash IS NULL
GROUP BY sc.contract_id, sc.id ORDER BY pending_rows DESC LIMIT 20;
```

```sql
-- Q4  sanity: orphans already in `assets` as SAC/soroban (should be ~0 = proves the derivation gap)
SELECT count() FROM soroban_contracts sc FINAL
WHERE sc.is_sac = false AND sc.deployed_at_ledger IS NULL AND sc.wasm_hash IS NULL
  AND sc.id IN (SELECT contract_id FROM assets WHERE asset_type IN (2, 3));
```

```sql
-- Q5  EXPORT for the cryptographic SAC match (CH can't SHA256/XDR)
--     then external Rust/JS: decode topics_xdr → Asset(code,issuer)
--     → derive_sac_contract_id(passphrase) == orphan_strkey ?
SELECT sc.contract_id AS orphan_strkey,
       any(e.signature)  AS sample_sig,
       any(e.topics_xdr) AS sample_topics_xdr
FROM soroban_contracts sc FINAL
INNER JOIN soroban_events e ON e.contract_id = sc.id
WHERE sc.is_sac = false AND sc.deployed_at_ledger IS NULL AND sc.wasm_hash IS NULL
GROUP BY sc.contract_id
INTO OUTFILE 'orphans_events.tsv' FORMAT TSV;
```

## Companion — G5 name verification (off-ledger names)

```sql
-- expected name_nonempty = 0 → confirms names are off-ledger (enrichment job, task 0297)
SELECT count() AS total,
       countIf(name IS NOT NULL)                  AS name_not_null,
       countIf(name IS NOT NULL AND name != '')   AS name_nonempty,
       countIf(NOT is_sac AND name IS NOT NULL AND name != '') AS nonempty_non_sac
FROM soroban_contracts FINAL;
```
