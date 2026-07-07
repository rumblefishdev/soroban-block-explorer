import {
  getIdentifierHref,
  routeSegments,
} from '@rumblefish/soroban-block-explorer-ui';

// URL shapes come from the single source in `libs/ui`: the segment word from
// `routeSegments` and the one builder `getIdentifierHref`. So these app
// routes, the router `path` patterns and the `IdentifierDisplay` links all
// stay in lock-step (task 0299). This file adds only the app-only routes:
// index pages, `NAV_LINKS`, the composite NFT route, and `search`.
//
// Note: `getIdentifierHref` `encodeURIComponent`s every id. For tx hashes
// (hex), account/contract StrKeys (base32) and ledger sequences (numeric)
// that is a no-op; `asset` and `pool` were already encoded. The `L...`
// pool StrKey and `CODE-ISSUER | native` asset token remain canonical.
export const routes = {
  home: '/',

  transactions: `/${routeSegments.transaction}`,
  transaction: (hash: string) => getIdentifierHref('transaction', hash),

  ledgers: `/${routeSegments.ledger}`,
  ledger: (sequence: string | number) =>
    getIdentifierHref('ledger', String(sequence)),

  accounts: `/${routeSegments.account}`,
  account: (accountId: string) => getIdentifierHref('account', accountId),

  assets: `/${routeSegments.asset}`,
  asset: (id: string) => getIdentifierHref('asset', id),

  contracts: `/${routeSegments.contract}`,
  contract: (contractId: string) => getIdentifierHref('contract', contractId),

  nfts: `/${routeSegments.nft}`,
  // Composite key `(contract_id, token_id)`: `contract_id` is the C-strkey
  // of the issuing contract (56 chars), `tokenId` is an opaque
  // contract-defined string (≤128 ASCII) — encode to guard `/`, `?`, `#`.
  // Not via `getIdentifierHref` (single-id cannot build a composite URL).
  nft: (contractId: string, tokenId: string) =>
    `/${routeSegments.nft}/${contractId}/${encodeURIComponent(tokenId)}`,

  pools: `/${routeSegments.pool}`,
  pool: (strkey: string) => getIdentifierHref('pool', strkey),

  search: (q: string) => `/search?q=${encodeURIComponent(q)}`,
} as const;

export const NAV_LINKS = [
  { to: routes.transactions, label: 'Transactions' },
  { to: routes.accounts, label: 'Accounts' },
  { to: routes.ledgers, label: 'Ledgers' },
  { to: routes.assets, label: 'Assets' },
  { to: routes.contracts, label: 'Contracts' },
  { to: routes.nfts, label: 'NFTs' },
  { to: routes.pools, label: 'Liquidity Pools' },
] as const;
