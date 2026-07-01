// Canonical URL-shape table — the SINGLE source of truth for every entity
// route in the app (task 0299). It lives in libs/ui, not web/, because libs/ui
// components (`IdentifierDisplay`, `routeForHit`, `OperationFlowTree`) must
// build URLs and cannot import from web/ (dependency direction is
// web → libs/ui). web re-exports this from `web/src/router/routes.ts`, so app
// callsites are unchanged and a URL-shape change is a one-file edit here.
//
// Every id arg is `encodeURIComponent`-guarded uniformly. For the current id
// shapes (G/C/L-strkey, hex tx hash, numeric ledger) that is a no-op, but it
// removes the old drift where the two duplicate tables encoded inconsistently.
export const routes = {
  home: '/',

  transactions: '/transactions',
  transaction: (hash: string) => `/transactions/${encodeURIComponent(hash)}`,

  ledgers: '/ledgers',
  ledger: (sequence: number | string) =>
    `/ledgers/${encodeURIComponent(String(sequence))}`,

  accounts: '/accounts',
  account: (accountId: string) => `/accounts/${encodeURIComponent(accountId)}`,

  assets: '/assets',
  asset: (id: string) => `/assets/${encodeURIComponent(id)}`,

  contracts: '/contracts',
  contract: (contractId: string) =>
    `/contracts/${encodeURIComponent(contractId)}`,

  nfts: '/nfts',
  // Composite key `(contract_id, token_id)`: `contract_id` is the C-strkey of
  // the issuing contract (56 chars), `tokenId` is an opaque contract-defined
  // string (≤128 ASCII) — both encoded to guard `/`, `?`, `#`.
  nft: (contractId: string, tokenId: string) =>
    `/nfts/${encodeURIComponent(contractId)}/${encodeURIComponent(tokenId)}`,

  pools: '/liquidity-pools',
  // `strkey` is the CAP-38 `L...` form (56 chars, base32). Canonical
  // everywhere — backend `/v1/liquidity-pools/:id` accepts strkey only.
  pool: (strkey: string) => `/liquidity-pools/${encodeURIComponent(strkey)}`,

  search: (q: string) => `/search?q=${encodeURIComponent(q)}`,
} as const;
