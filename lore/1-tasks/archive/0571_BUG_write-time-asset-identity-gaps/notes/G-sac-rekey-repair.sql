-- G — One-time re-key of rows stored under a SAC contract's own id onto the
-- classic / native asset the SAC wraps (task 0571).
--
-- Mainnet rows predate the `sac_deployed` flag repair; testnet rows come from
-- the parallel backfill, which cannot see its own partition's SAC facets. New
-- writes key correctly since the flag repair (G-sac-deployed-flag-repair.sql).
--
-- The SAC → asset map is built live from `asset_sac` joined to `assets` (the
-- shape of docs/runbooks/0374_lp_legs_sac_rekey_repair.md): a SAC that wraps a
-- classic or native asset gets that asset's `assets.id`.
--
-- Written for `testnet.`; for mainnet replace every `testnet.` with `default.`
-- (mainnet has rows in `balances` only). Order: 1 preview, 2–4 writes, 1 again,
-- then 5–6 deletes (irreversible), then 1 again (all zeros).
--
-- Measured 2026-10-19:
--   testnet  balances 22,166 pairs (21,849 to insert: 296 already have a newer
--            row under the asset, 21 an identical one at the same ledger),
--            pool_movements 6,560, pool legs 501 in 403 pools, unmapped 0
--   default  balances 300 pairs (300 to insert), pools 0, unmapped 0

-- 1. Preview (read-only): rows still keyed on a SAC, and how many have no asset.
WITH (SELECT CAST(groupArray((s.sacid, a.aid)), 'Map(Int64, Int64)')
      FROM (SELECT sac_contract_id AS sacid, any(asset_type) t, any(asset_code) c, any(issuer_id) i
            FROM testnet.asset_sac WHERE sac_contract_id != 0 GROUP BY sac_contract_id) AS s
      INNER JOIN (SELECT asset_type t, asset_code c, issuer_id i, any(id) aid
                  FROM testnet.assets WHERE asset_type IN (0, 1)
                  GROUP BY asset_type, asset_code, issuer_id) AS a
        ON a.t = s.t AND a.c = s.c AND a.i = s.i) AS m
SELECT 'balances' AS t, count() AS keyed_on_sac, countIf(m[asset_id] = 0) AS unmapped
FROM (SELECT holder_id, asset_id FROM testnet.balances
      WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac)
      GROUP BY holder_id, asset_id)
UNION ALL
SELECT 'pool_movements', count(), countIf(m[asset_id] = 0)
FROM testnet.pool_movements WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac)
UNION ALL
SELECT 'pool legs', count(), countIf(m[leg] = 0)
FROM (SELECT pool_id, argMax(legs, last_updated_ledger) AS legs FROM testnet.liquidity_pools GROUP BY pool_id)
ARRAY JOIN legs AS leg WHERE leg IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);

-- 2. balances: the latest row of each SAC-keyed pair, under the asset. Skipped
--    when the asset already has a row at the same or a later ledger (RMT keeps
--    the highest `last_updated_ledger`, so that row already wins).
INSERT INTO testnet.balances (holder_id, asset_id, amount, last_updated_ledger, closed_at_ledger)
WITH (SELECT CAST(groupArray((s.sacid, a.aid)), 'Map(Int64, Int64)')
      FROM (SELECT sac_contract_id AS sacid, any(asset_type) t, any(asset_code) c, any(issuer_id) i
            FROM testnet.asset_sac WHERE sac_contract_id != 0 GROUP BY sac_contract_id) AS s
      INNER JOIN (SELECT asset_type t, asset_code c, issuer_id i, any(id) aid
                  FROM testnet.assets WHERE asset_type IN (0, 1)
                  GROUP BY asset_type, asset_code, issuer_id) AS a
        ON a.t = s.t AND a.c = s.c AND a.i = s.i) AS m,
     bad AS (SELECT holder_id, m[asset_id] AS aid,
                    argMax(amount, last_updated_ledger) AS amt,
                    max(last_updated_ledger) AS lu,
                    argMax(closed_at_ledger, last_updated_ledger) AS cl
             FROM testnet.balances
             WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac)
             GROUP BY holder_id, asset_id),
     good AS (SELECT holder_id, asset_id AS aid, max(last_updated_ledger) AS glu
              FROM testnet.balances
              WHERE (holder_id, asset_id) IN (SELECT holder_id, aid FROM bad)
              GROUP BY holder_id, asset_id)
SELECT bad.holder_id, bad.aid, bad.amt, bad.lu, bad.cl
FROM bad LEFT JOIN good USING (holder_id, aid)
WHERE good.glu < bad.lu;

-- 3. pool_movements: every SAC-keyed row again, under the asset (asset_id is
--    in the sort key, so these are new rows; none exists yet, measured).
INSERT INTO testnet.pool_movements
    (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount)
WITH (SELECT CAST(groupArray((s.sacid, a.aid)), 'Map(Int64, Int64)')
      FROM (SELECT sac_contract_id AS sacid, any(asset_type) t, any(asset_code) c, any(issuer_id) i
            FROM testnet.asset_sac WHERE sac_contract_id != 0 GROUP BY sac_contract_id) AS s
      INNER JOIN (SELECT asset_type t, asset_code c, issuer_id i, any(id) aid
                  FROM testnet.assets WHERE asset_type IN (0, 1)
                  GROUP BY asset_type, asset_code, issuer_id) AS a
        ON a.t = s.t AND a.c = s.c AND a.i = s.i) AS m
SELECT pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind,
       m[asset_id], amount
FROM testnet.pool_movements
WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);

-- 4. liquidity_pools.legs: rewritten in place (runbook 0374, step 3). A leg
--    the map does not know stays as it is.
ALTER TABLE testnet.liquidity_pools UPDATE legs = arrayMap(
    (x, y) -> if(y = 0, x, y),
    legs,
    arrayMap(x ->
        (SELECT CAST(groupArray((s.sacid, a.aid)), 'Map(Int64, Int64)')
         FROM (SELECT sac_contract_id AS sacid, any(asset_type) t, any(asset_code) c, any(issuer_id) i
               FROM testnet.asset_sac WHERE sac_contract_id != 0 GROUP BY sac_contract_id) AS s
         INNER JOIN (SELECT asset_type t, asset_code c, issuer_id i, any(id) aid
                     FROM testnet.assets WHERE asset_type IN (0, 1)
                     GROUP BY asset_type, asset_code, issuer_id) AS a
           ON a.t = s.t AND a.c = s.c AND a.i = s.i)[x],
        legs))
WHERE pool_kind = 1
SETTINGS mutations_sync = 2;

-- 5. DELETE (irreversible): the SAC-keyed balances, now copied under the asset.
DELETE FROM testnet.balances
WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);

-- 6. DELETE (irreversible): the SAC-keyed pool movements, now copied.
DELETE FROM testnet.pool_movements
WHERE asset_id IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);
