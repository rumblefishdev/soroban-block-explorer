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
  nft: (id: string) => `/nfts/${encodeURIComponent(id)}`,

  pools: '/liquidity-pools',
  pool: (id: string) => `/liquidity-pools/${encodeURIComponent(id)}`,

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
