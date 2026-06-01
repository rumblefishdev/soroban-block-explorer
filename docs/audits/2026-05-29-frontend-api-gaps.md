# FE → API gaps audit (2026-05-29)

> **Status update (2026-06-01).** 3 of the 7 gaps are already implemented on
> branch `feat/0274-0275_api-gaps-and-contracts-list` (commit `c6bec5ee`),
> pending merge — do **not** redo them:
>
> - ✅ `order` param on `GET /v1/ledgers` (§2) — wired in `c6bec5ee`, but the
>   asc path was broken (reversed order + dead forward pagination); fixed
>   correctly in `08279072` with a behaviour test.
> - ✅ `recent_events` on `ContractStats` (§2) — verified genuinely correct.
> - ✅ typed `interface_metadata` schema (§3) — `08279072` also makes a
>   decode failure return 500 instead of silently `null` (re-index legacy
>   rows before deploy; fresh-data parse-success not yet verified e2e).
>
> Still open: `GET /v1/accounts` list (§1), per-op LP amounts (§2),
> `PoolAssetLeg.icon_url` (§2). Pool chart nulls (§2) stay with task 0199.
>
> **Task-number corrections:** the "0249" and "0250" cites below are **wrong**
> — 0249 is an archived AWS-teardown task, 0250 is ClickHouse quota
> enforcement. Neither tracks the FE follow-ups implied here; those tasks do
> not exist yet. 0247 (LP per-tx amounts research) and 0199 (LP analytics)
> are correct.

## 1. Missing endpoints

### `GET /v1/accounts` — Accounts list

Blocking the Accounts page. Currently mocked in-memory at
[`web/src/api/hooks/useAccountsList.ts`](../../web/src/api/hooks/useAccountsList.ts)
with 80 synthesized rows.

```ts
// Query
type Query = {
  limit?: number; // 1..100, default 20
  cursor?: string; // opaque
  sort?: 'xlm_desc' | 'last_seen_desc' | 'first_seen_desc';
  'filter[q]'?: string; // substring on account_id
  'filter[with_domain]'?: boolean;
};

// Response
interface AccountListItem {
  account_id: string; // G-strkey
  xlm_balance: string; // NUMERIC(28,7), e.g. "4107709533.0000000"
  xlm_supply_percent: number; // 0..100, 2 decimals
  first_seen_ledger: number;
  last_seen_ledger: number;
  home_domain: string | null;
}

interface AccountListResponse {
  data: AccountListItem[];
  page: {
    limit: number;
    next_cursor: string | null;
    prev_cursor: string | null;
  };
}
```

Each `sort` mode wants its own DB index (`xlm_balance DESC`,
`last_seen_ledger DESC`, `first_seen_ledger DESC`).

> **⚠ Data-feasibility — resolve before implementing.** This shape writes
> cheques the current schema can't fully cash:
>
> - **`xlm_supply_percent` has no backing data.** No network-wide XLM total /
>   circulating supply is stored anywhere (`assets.total_supply` exists only
>   per-asset, never for native XLM). The mock fakes it with a hardcoded
>   constant (`useAccountsList.ts`). Decide: hardcode a constant, compute
>   `SUM(balance)` over native rows (cost), or drop the column for v1.
> - **`xlm_balance` + `sort=xlm_desc` cross a table boundary.** Balance lives in
>   `account_balances_current` (asset_type=0), not on `accounts`. Ordering
>   `accounts` by a column in another table complicates the keyset cursor and
>   needs a new index on the native balance (none today).
> - **`sort=first_seen_desc` has no index** (`accounts.first_seen_ledger`);
>   only `last_seen` is indexed.
> - **`rank` (`#` column)** is only stable for one sort mode and breaks under
>   filtering — needs a deliberate design, not an afterthought.

---

## 2. Missing fields on existing endpoints

### `GET /v1/liquidity-pools/{pool_id}/transactions` — per-op LP amounts

The "Amount" column in the pool-tx table is intentionally hidden
because the endpoint doesn't return per-operation reserves moved.
Tracked as backend **0247** (research). FE follow-up task TBD — the
original "0249" cite was wrong (0249 = archived AWS-teardown task).

Proposed: opt-in expansion via query param.

```ts
// Query — new optional param
type Query = {
  limit?: number;
  cursor?: string;
  expand?: 'lp_op_details'; // NEW
};

// Response — add this field to each existing PoolTransactionItem
// when `expand=lp_op_details` is set
interface LpOperationDetail {
  operation_type: 'deposit' | 'withdraw' | 'trade';
  amount_a: string; // NUMERIC(28,7)
  amount_b: string; // NUMERIC(28,7)
}

interface PoolTransactionItem {
  // ... existing fields ...
  lp_operation_detail?: LpOperationDetail; // present when expand requested
}
```

### `GET /v1/contracts/{contract_id}` — events count metric ✅ SHIPPED (c6bec5ee)

`stats.recent_unique_callers` is a **callers** metric, but the contract
detail page's "Events" tab pill currently borrows it as a stand-in for
an event count. Need a real `recent_events` (or similar) on
`ContractStats` so the pill shows actual event volume.

### `GET /v1/ledgers` — undocumented `order` param ✅ SHIPPED (c6bec5ee)

FE passes `order` today but — correction — the real backend **silently
ignored** it (only the mock honoured it); Axum dropped the unknown param,
so the list was never actually re-sorted. Now wired (`LedgersListQuery`)
and declared in OpenAPI: `order=asc` flips the first-page walk to
oldest-first; ignored once a cursor is supplied.

```ts
type Query = {
  limit?: number;
  cursor?: string;
  order?: 'asc' | 'desc'; // missing from spec
};
```

### `GET /v1/liquidity-pools/{pool_id}/chart` — fields always null

The endpoint exists, the response schema (`ChartDataPoint`) is fully
typed, but **every value field is `null`** in production until backend
task **0199** (LP analytics + price oracle) ships. FE renders a
"Chart data not yet available" placeholder for the entire chart card.

```ts
// What we get back today, for every bucket:
interface ChartDataPoint {
  bucket: string; // OK
  samples_in_bucket: number; // OK
  tvl: null; // need real numbers
  volume: null; // need real numbers
  fee_revenue: null; // need real numbers
}
```

Once 0199 lands, the `null`s become NUMERIC strings and FE renders
the chart with no further code change. (FE placeholder-removal follow-up
TBD — the original "0250" cite was wrong; 0250 = ClickHouse quota
enforcement, unrelated.)

### `PoolAssetLeg` — missing `icon_url`

`AssetItem` carries `icon_url`; `PoolAssetLeg` does not. Pool-list
and pool-detail avatars fall back to drawing the first letter of the
asset code (e.g. `X` for XLM). Adding the field would let pools
share the same icon rendering as the assets list.

> **Note — not a column copy.** `PoolAssetLeg` carries only XDR
> `(asset_code, issuer)` (+ optional SAC `contract_id`); `icon_url` lives on
> the `assets` row. So this is a LEFT JOIN to `assets` per leg (2 per pool) —
> on the pool **list** endpoint that's an N+1-style cost to design for, not a
> trivial field add.

```ts
interface PoolAssetLeg {
  // ... existing fields ...
  icon_url: string | null; // NEW — mirror AssetItem
}
```

---

## 3. Type clarifications needed

### `GET /v1/contracts/{contract_id}/interface` — `interface_metadata` ✅ SHIPPED (c6bec5ee)

OpenAPI types this as `unknown` (`{}`). FE hand-parses it through
[`parseInterfaceMetadata`](../../web/src/pages/contracts/interfaceMetadata.ts)
and re-discovers the shape at runtime. Please codify the schema so
typegen produces a real type and FE can drop the defensive parser.

```ts
// What the FE actually expects (mirrors indexer
// `crates/indexer/src/handler/persist/staging.rs`).
// `null` is valid for SAC / pre-upload / stub contracts.
interface ContractInterfaceMetadata {
  functions: Array<{
    name: string;
    doc: string;
    inputs: Array<{ name: string; type_name: string }>;
    outputs: string[]; // empty array == void return
  }>;
  wasm_byte_len: number;
}
```

---

## 4. Returned but unused (info only)

| Endpoint                        | Unused field                             |
| ------------------------------- | ---------------------------------------- |
| `GET /v1/assets/{id}`           | `description`, `home_page` (issuer TOML) |
| `GET /v1/accounts/{account_id}` | `home_domain`                            |

NFT `metadata: null` fail-soft is intentional (ADR 0043).

---

**Must-have**

- New endpoint: `GET /v1/accounts` (list) — Accounts page is dead without it.
- Document `order` param on `GET /v1/ledgers`.

**Should-have**

- Real numbers in `GET /v1/liquidity-pools/:id/chart` (task 0199 — the chart card is a placeholder until then).
- `?expand=lp_op_details` on pool transactions (0247 / 0249).
- `icon_url` on `PoolAssetLeg`.
- Real events count on `ContractStats` (currently the Events tab pill borrows `recent_unique_callers`).
- Real schema for `interface_metadata`.
