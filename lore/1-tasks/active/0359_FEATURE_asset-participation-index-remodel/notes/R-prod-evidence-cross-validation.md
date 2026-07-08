---
title: 'Prod evidence + cross-validation: our ClickHouse vs stellar.expert / Horizon'
type: research
status: mature
spawned_from: notes/S-diagnosis-calibration.md
spawns: []
tags:
  [
    'clickhouse',
    'prod-evidence',
    'cross-validation',
    'stellar-expert',
    'horizon',
  ]
links: []
history:
  - date: 2026-07-08
    status: mature
    who: karolkow
    note: >
      Direct prod ClickHouse evidence via `chq`, cross-validated per-example
      against Horizon (ground truth) and stellar.expert (reference render). Every
      example carries BOTH links (ours + stellar.expert). Measured on prod
      2026-07-08; window = last 300k ledgers ending at 63,375,762.
---

# Prod evidence + cross-validation

Direct queries against prod ClickHouse (`operations_appearances`) via `chq`,
each example cross-checked against Horizon (canonical truth) and stellar.expert
(the reference explorer). Confirms the single-slot loss with real numbers and
real transactions.

## Statistics (prod, measured 2026-07-08)

- **`operations_appearances` total rows: 6,405,324,205** (~6.4 B).
- **Window = last 300k ledgers** (63,075,762 → 63,375,762), raw rows (not FINAL):
  - Total operations: **182,204,545**
  - Empty `asset_code` (no classic asset stored): **105,119,530 = 57.7%**
  - Offers (types 3/4/12): **28,059,760** — **empty: 28,059,760 = 100%**
  - Native payments (type 1, empty): **23,451,834**
  - Path-payments (types 2/13): **28,590,462** — empty (native destination):
    **10,323,827 = 36.1%**

The 57.7% empty includes op types that legitimately carry no asset (Soroban
invoke, set_options, account_merge, manage_data, …). The **clean defect** is the
offers (28 M, 100% asset-less) + native-as-empty (23.5 M) + path-payment legs
that are never stored (source leg + all hops, on every path-payment).

## Historical scope — the table covers the SOROBAN ERA only

`operations_appearances` spans ledgers **50,457,424 → 63,376,009** (~12.9 M
ledgers, ~Feb 2024 → Jul 2026 = the Soroban era). **Pre-Soroban Stellar history
(2015 – early 2024) is NOT in this table** — consistent with the product being a
_Soroban_ explorer (sorobanscan). The whole 6.4 B rows ARE the Soroban era.

**Implication for this task:** "complete backward data" is bounded to the Soroban
era (~13 M ledgers), not all of Stellar history — the backfill re-parse window is
the covered range, materially smaller than "2015-onward".

### Per-era distribution (200k-ledger samples across the covered range)

| era (ledger)        | total ops | % empty asset | offers  | offers empty   | payments | native pay | path-pay |
| ------------------- | --------- | ------------- | ------- | -------------- | -------- | ---------- | -------- |
| 51.0 M (early 2024) | 89.19 M   | 65.7%         | 27.44 M | 27.44 M (100%) | 13.98 M  | 2.26 M     | 43.94 M  |
| 54.0 M              | 78.89 M   | 58.2%         | 21.88 M | 21.88 M (100%) | 24.42 M  | 4.06 M     | 29.67 M  |
| 57.0 M              | 94.01 M   | 40.5%         | 21.10 M | 21.10 M (100%) | 50.84 M  | 1.13 M     | 12.48 M  |
| 60.0 M              | 122.10 M  | 40.1%         | 21.59 M | 21.59 M (100%) | 64.88 M  | 2.16 M     | 17.71 M  |
| 63.0 M (recent)     | 121.59 M  | 56.4%         | 19.55 M | 19.55 M (100%) | 58.25 M  | 18.41 M    | 19.26 M  |

The pattern is stable across the whole covered history: **offers are 100%
asset-less in every era** (offers == offers_empty every time); overall empty-asset
share swings 40–66% with the op mix. Extrapolating offers (~100–135 per ledger ×
12.9 M ledgers) → **~1.3–1.7 B asset-less offers table-wide**, matching the
~1.37 B figure in the earlier diagnosis. (Full-history GROUP BY was NOT run — a
6.4 B-row scan exceeds the 2 B-rows/hour read quota; these are 200k-ledger
samples per era.)

## Per-type distribution (same window, raw rows)

`empty_asset`: 1 = `asset_code` is empty, 0 = has a code.

| type | op                          | empty=0 (has code) | empty=1 (no code)       |
| ---- | --------------------------- | ------------------ | ----------------------- |
| 1    | payment                     | 56,860,969         | 23,406,466 (native XLM) |
| 2    | path_payment_strict_receive | 6,280,219          | 3,473,033 (native dest) |
| 3    | manage_sell_offer           | 0                  | 16,910,464 (**all**)    |
| 4    | create_passive_sell_offer   | 0                  | 7,251 (**all**)         |
| 6    | change_trust                | 1,208,839          | 3,196                   |
| 12   | manage_buy_offer            | 0                  | 11,130,605 (**all**)    |
| 13   | path_payment_strict_send    | 11,977,721         | 6,846,659 (native dest) |

Offers (3/4/12): **zero** rows carry an asset. Path-payments (2/13): even the
"has code" rows store only the destination leg — the source asset and every path
hop are dropped.

## Cross-validation examples (our DB ↔ Horizon ↔ stellar.expert)

### 1. Offer — the cleanest core-bug case (asset 100% lost)

`3c2aa9b72c3da98082735a7f6c5143478b8e4e0cc438d613ee8545112bebc8df` (type 3, manage_sell_offer)

- **Reality (Horizon):** sell **729.2591422 XLM** for **AQUA** at 515 AQUA/XLM — two assets.
- **Our DB (`chq`):** `asset_code=''`, `asset_issuer_id=NULL`, `pool_ids=[]` — **no asset at all**.
- **Effect:** invisible on BOTH the XLM and the AQUA asset page (the query filters `asset_code='XLM'`, never matches empty).
- Ours: https://sorobanscan.rumblefish.dev/transactions/3c2aa9b72c3da98082735a7f6c5143478b8e4e0cc438d613ee8545112bebc8df
- stellar.expert: https://stellar.expert/explorer/public/tx/3c2aa9b72c3da98082735a7f6c5143478b8e4e0cc438d613ee8545112bebc8df
- Asset pages — AQUA: [ours](https://sorobanscan.rumblefish.dev/assets/AQUA-GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA) · [stellar.expert](https://stellar.expert/explorer/public/asset/AQUA-GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA)

### 2. Path payment — 10 ops, 12 distinct assets

`3d95452f58071cf3e05aa33db5f3b3038d61f2b43507150e91f8c509d08626ea` (10× type 13)

- **Reality (Horizon):** 10 path-payment ops touching **12 distinct assets**:
  `T43, FiaT, native, U, USDC, LOVE, 4ALL, R3FL3cT, SHX, EITG, XRPketchup, VELO`.
- **Our DB:** one row per op, storing only each op's **destination** leg
  (USDC / native / FiaT). ~9 of 12 assets (sources + all hops) dropped.
- Ours: https://sorobanscan.rumblefish.dev/transactions/3d95452f58071cf3e05aa33db5f3b3038d61f2b43507150e91f8c509d08626ea
- stellar.expert: https://stellar.expert/explorer/public/tx/3d95452f58071cf3e05aa33db5f3b3038d61f2b43507150e91f8c509d08626ea

### 3. Liquidity-pool deposit — 2 assets, only the pool id stored

`8f8532e7a3350c2f050bdf8a8e114706384b45a89003767a0405221a49fd499b` (ledger 63,375,507; change_trust + LP deposit)

- **Reality (Horizon + stellar.expert render):** established trustline to the
  `1018`/`bubba` pool, then **deposited 0.0000003 `1018` and 86 `bubba`** to the
  pool (price range 3.453–3.523). Two assets, with amounts.
- **Our DB (`chq`):**
  - op 1 (type 6, change_trust to the pool share): `asset_code=''`, 0 pools.
  - op 2 (type 22, LP deposit): `asset_code=''`, **1 pool id** (raw binary), the
    two deposited assets `1018`/`bubba` NOT stored.
- **Effect:** invisible on the `1018` and `bubba` asset pages (the asset query
  does not join through `pool_ids`).
- Ours: https://sorobanscan.rumblefish.dev/transactions/8f8532e7a3350c2f050bdf8a8e114706384b45a89003767a0405221a49fd499b
- stellar.expert: https://stellar.expert/explorer/public/tx/8f8532e7a3350c2f050bdf8a8e114706384b45a89003767a0405221a49fd499b
- Asset pages — bubba: [ours](https://sorobanscan.rumblefish.dev/assets/bubba-GDKS7XTNEVCPGVUT2ZPPOU5CHHF3NH6NX7P3ROSFSDAO6NQDS3C74Y6C) · [stellar.expert](https://stellar.expert/explorer/public/asset/bubba-GDKS7XTNEVCPGVUT2ZPPOU5CHHF3NH6NX7P3ROSFSDAO6NQDS3C74Y6C) — 1018: [ours](https://sorobanscan.rumblefish.dev/assets/1018-GAVVNJKEM4XFXBPYITFCDVKOZRI3PAXJHDH666MDQIXJAJ4H7HO3722C) · [stellar.expert](https://stellar.expert/explorer/public/asset/1018-GAVVNJKEM4XFXBPYITFCDVKOZRI3PAXJHDH666MDQIXJAJ4H7HO3722C)

stellar.expert render (pasted by karolkow, 2026-07-08): _"established trustline
to 1018/bubba … deposited liquidity 0.0000003 1018 and 86 bubba to the pool
1018/bubba (price range 3.453 - 3.523)"_ — both assets shown with amounts.

### 4. Native payment — stored as absence

`af9cbbed692ca7d7ecf5c70d542ef885f5beb55553ed9d240339949e46d2527d` (type 1, native)

- **Reality:** a native XLM payment.
- **Our DB:** `asset_code=''` — native modelled as absence, not a key.
- **Effect:** `/assets/native/transactions` is documented out-of-scope and
  returns `{"data":[]}` (canonical query note, `10_get_assets_transactions.sql:61`).
- Ours: https://sorobanscan.rumblefish.dev/transactions/af9cbbed692ca7d7ecf5c70d542ef885f5beb55553ed9d240339949e46d2527d
- stellar.expert: https://stellar.expert/explorer/public/tx/af9cbbed692ca7d7ecf5c70d542ef885f5beb55553ed9d240339949e46d2527d
- XLM asset page — [ours (empty)](https://sorobanscan.rumblefish.dev/assets/native) · [stellar.expert (full)](https://stellar.expert/explorer/public/asset/XLM)

## Inferred stellar.expert schema (from behaviour — closed source)

stellar.expert renders, per asset: each operation with the specific asset's
amount + role + path hops, a separate **Trades** tab, Holders, and Markets. That
is not renderable without a per-asset row (re-parsing per request would be too
slow at their scale). Inferred tables:

1. **Asset registry** — `asset(id, code, issuer, type)`, native given a real
   numeric id (not empty).
2. **Per-asset participation** (= our proposed fan-out) —
   `asset_operations(asset_id, ledger, tx, op_index, role, amount, counterparty)`,
   keyed by `asset_id` for fast per-asset paging.
3. **Trades** — `trades(base_asset, counter_asset, base_amount, counter_amount,
price, seller, buyer, pool_or_offer_id, ledger)`, one row per crossed offer
   (ClaimAtom), keyed by asset.
4. **Balances/holders + markets** aggregates for the Holders/Markets tabs.

**Conclusion:** stellar.expert almost certainly already uses the per-(operation,
asset) participation index + a separate trades table that this task proposes —
their per-asset render is explainable by little else. Strong external validation
that the fan-out is the industry-standard shape, not an experiment. (Inference
from observable behaviour, not their source.)

## Method / reproducibility

`chq "<SQL>"` against prod CH (read-only; quotas 2 B rows / 100 GB per
server-hour → all queries are `ledger_sequence`-windowed to prune). Horizon:
`horizon.stellar.org/transactions/<hash>/operations`. Numbers are raw
ReplacingMergeTree rows (not FINAL) → treat as ±a few %. See
[[reference_chq_clickhouse_cli]], [[reference_sorobanscan_hosts]].
