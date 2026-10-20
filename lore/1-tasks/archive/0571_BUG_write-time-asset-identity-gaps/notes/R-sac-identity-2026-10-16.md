# R: SAC identity — where the SAC-keyed rows come from (2026-10-16)

Read-only measurements on `default` (mainnet) and `testnet`, plus Horizon and
stellar.expert for the deploy transactions.

## SACs by facet state

|                                        | mainnet | testnet |
| -------------------------------------- | ------- | ------- |
| SACs in `soroban_contracts` (`is_sac`) | 4,065   | 15,368  |
| facet `sac_deployed = 1`               | 3,915   | 15,148  |
| facet `sac_deployed = 0`               | **148** | 73      |
| no facet                               | 2       | 147     |

## Rows keyed on a SAC contract

| table                            | mainnet                                      | testnet                                               |
| -------------------------------- | -------------------------------------------- | ----------------------------------------------------- |
| `balances` (holder, asset) pairs | 299 (297 non-zero), 144 SACs, all on facet 0 | 22,166: 22,141 on facet 1 (6,167 SACs), 25 on facet 0 |
| `liquidity_pools.legs`, pools    | 0 of 54,687                                  | 403 of 5,384                                          |
| `pool_movements` rows            | 0 of 12,885,411                              | 6,560 of 622,652, all on facet 1                      |
| `pool_operation_amounts` rows    | 0                                            | 0                                                     |

Testnet rows on facet-1 SACs: last written at ledger ≤ 4,991,999 (balances)
and ≤ 4,991,994 (pool movements); none from 5,000,000, where the live indexer
took over. Of the 22,141 balance pairs, 9,686 sit in the same 64,000-ledger
partition as the SAC's deploy and 12,455 in another.

## How each facet-0 SAC was deployed (mainnet)

Method: for each SAC, the creator account from stellar.expert
(`/explorer/public/contract/{C…}`), that account's transactions in the deploy
ledger from Horizon (`/ledgers/{n}/transactions`), envelopes decoded with
`stellar xdr decode --type TransactionEnvelope`. Control: 100 facet-1 SACs
drawn by `cityHash64` order.

| group                 | checked    | result                                                                                                                       |
| --------------------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------- |
| facet 0 (148)         | 147        | 147 deployed by another contract: `coin_it` 80, `coin_them` 56, `create` 8, `create_stellar_asset_contract` 3; no auth entry |
| no facet (2)          | 1          | 1 via `create_stellar_asset_contract`                                                                                        |
| facet 1 (100 sampled) | 53         | 53 top-level `CreateContract` from an asset                                                                                  |
| not checkable         | 1 + 1 + 47 | deploy ledger below ~58.75M, beyond Horizon's retained history (~1 year)                                                     |

What it cannot show: SACs deployed before Horizon's retention window. The
split is clean on everything checkable — no contract-deployed SAC with facet
1, no direct deploy with facet 0.

Example: `CAIYL7WXJJCAYYUE4O2ZG4SPHBCYU6FCVFOMALMSBCUL4E6MLXIPPQA7`, ledger
64,951,791, transaction
`7ac7c9477b808751eef5f9ce749176ee9b0cd68cb5ba8f815d54d1d9951c440e`: one
`invoke_contract` on `CBFUPD6RNBYGW6UNMIP26TOXM2FSMCBI7XFY4LATRKFKHTIHPRG2W7LJ`
`create_stellar_asset_contract(<asset XDR>)`, zero auth entries.

## The instance entry carries the identity

`contract_instances.data_xdr` of a SAC, decoded as `LedgerEntryData`:
executable `stellar_asset`, storage `METADATA` (`decimal`, `name =
"CODE:ISSUER"`, `symbol`) and `AssetInfo` (`AlphaNum4`/`AlphaNum12`/`Native`
with `asset_code` and `issuer` bytes).

|                             | mainnet | testnet                                  |
| --------------------------- | ------- | ---------------------------------------- |
| SACs with a stored instance | 4,043   | 15,301                                   |
| — carrying `AssetInfo`      | 4,043   | 15,215                                   |
| — without (storage empty)   | 0       | 86, all with no facet, ledgers ≤ 845,183 |

The 86 are SACs the current code knows nothing about either, so taking the
identity from the entry loses none.

## Queries

```sql
-- facet state per SAC (used for both networks)
WITH sc AS (SELECT id FROM soroban_contracts GROUP BY id HAVING max(is_sac) = 1),
     fa AS (SELECT max(sac_contract_id) sid, max(sac_deployed) dep, toUInt8(1) found
            FROM asset_sac GROUP BY asset_type, asset_code, issuer_id, contract_id
            HAVING sid != 0)
SELECT count(), countIf(found = 0), countIf(found = 1 AND dep = 1),
       countIf(found = 1 AND dep = 0)
FROM sc LEFT JOIN fa ON fa.sid = sc.id;

-- SAC instances carrying AssetInfo
WITH sc AS (SELECT DISTINCT substring(base32Decode(contract_id), 2, 32) AS raw
            FROM soroban_contracts WHERE is_sac),
     ci AS (SELECT contract, argMax(data_xdr, ledger) x FROM contract_instances
            WHERE contract IN (SELECT raw FROM sc) GROUP BY contract)
SELECT count(), countIf(position(x, 'AssetInfo') > 0) FROM ci;
```
