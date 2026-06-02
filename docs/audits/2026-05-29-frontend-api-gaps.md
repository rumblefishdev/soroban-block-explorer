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

---

## 2. Missing fields on existing endpoints

### `GET /v1/liquidity-pools/{pool_id}/transactions` — per-op LP amounts

The "Amount" column in the pool-tx table is intentionally hidden
because the endpoint doesn't return per-operation reserves moved.
Tracked as backend **0247** (research) → FE **0249**.

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

### `GET /v1/contracts/{contract_id}` — events count metric

`stats.recent_unique_callers` is a **callers** metric, but the contract
detail page's "Events" tab pill currently borrows it as a stand-in for
an event count. Need a real `recent_events` (or similar) on
`ContractStats` so the pill shows actual event volume.

### `GET /v1/ledgers` — undocumented `order` param

FE passes `order` today and backend accepts it (mock does too), but
the OpenAPI spec doesn't declare it. Either add it or have FE drop it.

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
the chart with no further code change. Tracked FE-side as task
**0250** (placeholder removal).

### `PoolAssetLeg` — missing `icon_url`

`AssetItem` carries `icon_url`; `PoolAssetLeg` does not. Pool-list
and pool-detail avatars fall back to drawing the first letter of the
asset code (e.g. `X` for XLM). Adding the field would let pools
share the same icon rendering as the assets list.

```ts
interface PoolAssetLeg {
  // ... existing fields ...
  icon_url: string | null; // NEW — mirror AssetItem
}
```

---

## 3. Type clarifications needed

### `GET /v1/contracts/{contract_id}/interface` — `interface_metadata`

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
