# Milestone 2 — Captured API Responses

Real responses from the deployed API, for the entities referenced in
`milestone-2-evidence.pdf`.

- **API base:** `https://api-sorobanscan.rumblefishdev.com/v1`
- **Auth:** `x-api-key` header (key provided to reviewers on request)
- **Read path:** our own indexed mainnet data in ClickHouse — no third-party
  chain API on the read path.

Responses are shown as returned by the live API. Where noted, raw XDR blobs,
transaction signatures, and per-operation diagnostic core-metrics are trimmed so
the decoded fields stay readable — nothing else is altered. `null` fields
(`name` / `symbol` / `icon_url` on assets, `tvl` / `volume` on pools) are
optional off-chain **enrichment** values (Milestone 3 scope), not missing data.

---

## 1. Ledger — `GET /v1/ledgers/63300000`

```json
{
  "sequence": 63300000,
  "hash": "99edd7a94468775367fb767404094c3b75f2293ceddda8f38b45cde26327a7db",
  "closed_at": "2026-07-02T20:36:34Z",
  "protocol_version": 26,
  "transaction_count": 302,
  "base_fee": 100,
  "prev_sequence": 63299999,
  "next_sequence": 63300001,
  "transactions": {
    "data": [
      {
        "hash": "d291099d7e4e84acf9f272be5961122f8aca3a403cf1ea93e03e03808fb52bf3",
        "application_order": 302,
        "source_account": "GDI5WYUHPUACVZOFCXX65N6AEH2HSQALFORXANXF266JNJIJSVQZPTX3",
        "fee_charged": 34290,
        "successful": true,
        "operation_count": 1,
        "has_soroban": true,
        "operation_types": ["INVOKE_HOST_FUNCTION"],
        "contract_ids": [
          "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA"
        ],
        "created_at": "2026-07-02T20:36:34Z"
      }
    ]
  }
}
```

_The embedded `transactions.data` array carries all 302 ledger transactions
(paginated); one row shown._

---

## 2. Transaction — decoded invocation + CAP-67 events — `GET /v1/transactions/e584ff2d…687b65c9`

Full hash: `e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`
(a KALE-farm `plant` call). This one response backs both the decoded-invocation
and the decoded-events criteria.

```json
{
  "hash": "e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9",
  "ledger_sequence": 63310022,
  "source_account": "GBME3YR3P3AG4BHZ4AZZQIVFZCSQYVVJIUZO6ZXI2OEH2VMDFTK6Y7AX",
  "successful": true,
  "operation_count": 1,
  "has_soroban": true,
  "created_at": "2026-07-03T12:42:31Z",
  "heavy": {
    "operations": [
      {
        "op_type": "INVOKE_HOST_FUNCTION",
        "details": {
          "contractId": "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA",
          "functionName": "plant",
          "hostFunctionType": "invokeContract",
          "functionArgs": [
            {
              "type": "address",
              "value": "GBME3YR3P3AG4BHZ4AZZQIVFZCSQYVVJIUZO6ZXI2OEH2VMDFTK6Y7AX"
            },
            { "type": "i128", "value": "0" }
          ],
          "returnValue": { "type": "void", "value": null }
        }
      }
    ],
    "contract_events": [
      {
        "event_type": "contract",
        "contract_id": "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        "topics": [
          { "type": "sym", "value": "fee" },
          {
            "type": "address",
            "value": "GA2JRQOF6EA3HQWDCEDBPPMLYPJCFLDDGYZLEQGMS5SOBQIB3BAFHVAW"
          }
        ],
        "data": { "type": "i128", "value": "288790" }
      }
    ],
    "heavy_fields_status": "ok"
  }
}
```

- **Decoded invocation** (`heavy.operations[].details`): function name `plant`,
  typed arguments (`address`, `i128`), and `returnValue` — not raw XDR.
- **Decoded CAP-67 event** (`heavy.contract_events[]`): a `fee` charge from the
  native-XLM Stellar Asset Contract, topics decoded to `sym` + `address`, data
  to `i128`.
- _Trimmed:_ `envelope_xdr` / `result_xdr` / `result_meta_xdr`, signatures, and
  the per-operation `diagnostic_events` core-metrics array.

---

## 3. Account — `GET /v1/accounts/GA6G524Y…QSIAJ3O`

Full ID: `GA6G524YPEHXE7M5ICIDKBHPXM4HO7TKNAZYMLCRLB33CUNLQKSIAJ3O`

```json
{
  "account_id": "GA6G524YPEHXE7M5ICIDKBHPXM4HO7TKNAZYMLCRLB33CUNLQKSIAJ3O",
  "sequence_number": 261917452286525440,
  "balances": [
    {
      "asset_type_name": "native",
      "type": 0,
      "asset_code": null,
      "asset_issuer": null,
      "balance": "20072472",
      "decimals": 7,
      "last_updated_ledger": 63308589
    }
  ],
  "home_domain": null,
  "first_seen_ledger": 63308589,
  "last_seen_ledger": 63308589,
  "deleted": false
}
```

---

## 4. Contract — KALE farm — `GET /v1/contracts/CDL74RF5…VMFAIGWA`

Full ID: `CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA`

```json
{
  "contract_id": "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA",
  "wasm_hash": "db2c14290d4964e3805f2527dd132939ba5fb3fccac56b30bfab8fd091011627",
  "wasm_uploaded_at_ledger": 56150212,
  "deployer": "GBDVX4VELCDSQ54KQJYTNHXAHFLBCA77ZY2USQBM4CSHTTV7DME7KALE",
  "deployed_at_ledger": 54347093,
  "contract_type_name": "other",
  "contract_type": 1,
  "is_sac": false,
  "upgradeable": true,
  "stats": {
    "recent_invocations": 16984366,
    "recent_unique_callers": 6500,
    "recent_events": 0,
    "stats_window": "7 days"
  }
}
```

---

## 5. Contract invocations index — `GET /v1/contracts/CDL74RF5…VMFAIGWA/invocations`

The Invocations tab is the appearance index — each call links to its
transaction (tx hash / caller / ledger / time). First rows shown.

```json
{
  "data": [
    {
      "transaction_hash": "bc67f27fc1752d002523f927d536df153639fec45df450b552451474ae7b687b",
      "ledger_sequence": 63308653,
      "caller_account": "GBRJDSIOT4OODYJDQMZHDAMRUNMLZTVA3UWUOWXIVRKBUAQBBDHXSSKT",
      "amount": 1,
      "created_at": "2026-07-03T10:29:49Z",
      "successful": true
    },
    {
      "transaction_hash": "97073c3121861dfc457bdee1353f69b864d265aef8a6e01026f64fb12f6ab84e",
      "ledger_sequence": 63308653,
      "caller_account": "GDTAIB3ZUJXO36Z6C47CPHPM7GFUJ3WGYDWHFRT25DSQVL66VT274MEG",
      "amount": 1,
      "created_at": "2026-07-03T10:29:49Z",
      "successful": true
    },
    {
      "transaction_hash": "e5f56501b97cad1c350648ab3867f1fd19e78d4f6c649ff64388acc23f039f4d",
      "ledger_sequence": 63308653,
      "caller_account": "GC2JF6TVTMEMS3LC5LQTJDGMMTDH3LYW52NAZF4VFRTE6I5O4SBHA57K",
      "amount": 1,
      "created_at": "2026-07-03T10:29:49Z",
      "successful": true
    }
  ]
}
```

---

## 6. Asset — EURC — `GET /v1/assets/EURC-GDHU6WRG…ITNPP2`

Full ID: `EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2`
(asset id format is `CODE-ISSUER`).

```json
{
  "id": "EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
  "asset_type_name": "classic_credit",
  "asset_type": 1,
  "asset_code": "EURC",
  "issuer": "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
  "contract_id": null,
  "sac_contract_id": "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
  "sac_deployed": true,
  "name": null,
  "symbol": null,
  "decimals": 7,
  "total_supply": "41000089363667",
  "holder_count": 4441,
  "icon_url": null,
  "deployed_at_ledger": 50599892,
  "description": null,
  "home_page": null
}
```

---

## 7. Liquidity pool — XLM / USDC — `GET /v1/liquidity-pools/LCSGRVA5…PMAIQQUG`

Full ID: `LCSGRVA5R2NY6PDSBFSRMCFXJN63PLEZKLOK4DG7ESDR2HM4PMAIQQUG`
(the API requires the `L`-StrKey pool id, not the raw hex).

```json
{
  "pool_id": "LCSGRVA5R2NY6PDSBFSRMCFXJN63PLEZKLOK4DG7ESDR2HM4PMAIQQUG",
  "asset_a": {
    "asset_type_name": "native",
    "asset_type": 0,
    "asset_code": null,
    "issuer": null
  },
  "asset_b": {
    "asset_type_name": "credit_alphanum4",
    "asset_type": 1,
    "asset_code": "USDC",
    "issuer": "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
    "contract_id": "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
  },
  "fee_bps": 30,
  "fee_percent": "0.3",
  "created_at_ledger": 50457433,
  "participant_count": 330,
  "latest_snapshot_ledger": 63308847,
  "reserve_a": "11865343.1320615",
  "reserve_b": "2381566.8329882",
  "total_shares": "3997972.8712451",
  "tvl": null,
  "volume": null,
  "fee_revenue": null,
  "latest_snapshot_at": "2026-07-03T10:48:44Z"
}
```

---

## 8. NFT — PIKO PASS — `GET /v1/nfts/CCAOCYAL…7B2YPKS/8`

Full ID: `CCAOCYALAGXC2NQMSB5TMNE56KMONZHR2U7DLOOXE7J4WJNPA7B2YPKS` / token `8`.
`name`, `media_url`, and `metadata` are decoded from the on-chain
`token_uri` — an enriched NFT.

```json
{
  "contract_id": "CCAOCYALAGXC2NQMSB5TMNE56KMONZHR2U7DLOOXE7J4WJNPA7B2YPKS",
  "token_id": "8",
  "collection_name": null,
  "name": "PIKO",
  "media_url": "https://kpop.rocks/images/piko-pass-nft.jpg",
  "minted_at_ledger": 62891186,
  "owner_account": "GB56DE2RGWQT4E4PETY6W5GSVQFQU6XMQIOMDDFKLQ7HQGV3NTPWRNLO",
  "last_seen_ledger": 62891186,
  "metadata": {
    "name": "PIKO",
    "description": "PIKO PASS - Your Pass to KPOP.ROCKS ! 🎶\nToken-Gated Access to the Arcade, the Shop, Future Drops, & Pops. 🍭",
    "image": "https://kpop.rocks/images/piko-pass-nft.jpg",
    "external_url": "https://kpop.rocks",
    "attributes": [
      { "trait_type": "Collection", "value": "PIKO PASS" },
      { "trait_type": "Edition", "value": "GENESIS" },
      { "trait_type": "Type", "value": "Community Access Pass" },
      { "trait_type": "Supply", "value": "Unlimited" }
    ]
  }
}
```
