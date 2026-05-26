export const routes = {
  home: '/',

  transactions: '/transactions',
  transaction: (hash: string) => `/transactions/${hash}`,

  ledgers: '/ledgers',
  ledger: (sequence: number | string) => `/ledgers/${sequence}`,

  account: (accountId: string) => `/accounts/${accountId}`,

  assets: '/assets',
  asset: (id: string) => `/assets/${encodeURIComponent(id)}`,

  contract: (contractId: string) => `/contracts/${contractId}`,

  nfts: '/nfts',
  // Composite key `(contract_id, token_id)`: `contract_id` is the C-strkey
  // of the issuing contract (56 chars), `tokenId` is an opaque
  // contract-defined string (≤128 ASCII) — encode to guard `/`, `?`, `#`.
  nft: (contractId: string, tokenId: string) =>
    `/nfts/${contractId}/${encodeURIComponent(tokenId)}`,

  pools: '/liquidity-pools',
  // `strkey` is the CAP-38 `L...` form (56 chars, base32). Canonical
  // everywhere — backend `/v1/liquidity-pools/:id` accepts strkey only.
  pool: (strkey: string) => `/liquidity-pools/${encodeURIComponent(strkey)}`,

  search: (q: string) => `/search?q=${encodeURIComponent(q)}`,
} as const;

export const NAV_LINKS = [
  { to: routes.home, label: 'Home' },
  { to: routes.transactions, label: 'Transactions' },
  { to: routes.ledgers, label: 'Ledgers' },
  { to: routes.assets, label: 'Assets' },
  { to: routes.nfts, label: 'NFTs' },
  { to: routes.pools, label: 'Pools' },
] as const;
