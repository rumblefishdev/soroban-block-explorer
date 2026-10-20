-- G — One-time repair of `asset_sac.sac_deployed` for SACs deployed by another
-- contract (task 0571). Their deploy left the flag at 0, so the SAC → asset map
-- (`fetch_sac_classic_map`, `HAVING max(sac_deployed) = 1`) skipped them. The
-- parser fix reads the identity from the instance entry, so new deploys set the
-- flag; this fixes the ones already stored.
--
-- `asset_sac` is an AggregatingMergeTree with `max` columns: inserting 1 raises
-- the flag and nothing can lower it, so the statement is safe to repeat.
-- Testnet first, then replace `testnet.` with `default.`.
--
-- Preview (read-only). Measured 2026-10-19: testnet 73, default 148. After: 0.
SELECT count()
FROM (SELECT asset_type, asset_code, issuer_id, contract_id,
             max(sac_contract_id) AS sid, max(sac_deployed) AS dep
      FROM testnet.asset_sac
      GROUP BY asset_type, asset_code, issuer_id, contract_id)
WHERE dep = 0 AND sid IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);

-- Write (owner, `chw`):
INSERT INTO testnet.asset_sac
    (asset_type, asset_code, issuer_id, contract_id, sac_contract_id, sac_deployed)
SELECT asset_type, asset_code, issuer_id, contract_id, sid, 1
FROM (SELECT asset_type, asset_code, issuer_id, contract_id,
             max(sac_contract_id) AS sid, max(sac_deployed) AS dep
      FROM testnet.asset_sac
      GROUP BY asset_type, asset_code, issuer_id, contract_id)
WHERE dep = 0 AND sid IN (SELECT id FROM testnet.soroban_contracts WHERE is_sac);
