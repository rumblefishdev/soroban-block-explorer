// The canonical URL-shape table is the SINGLE source of truth in libs/ui
// (`@rumblefish/soroban-block-explorer-ui`), so the app and the lib components
// (`IdentifierDisplay`, `routeForHit`, `OperationFlowTree`) share one
// definition — a URL-shape change is a one-file edit there (task 0299). This
// module re-exports it unchanged so existing `import { routes } from
// '../router/routes'` callsites keep working, and adds the app-only nav config.
import { routes } from '@rumblefish/soroban-block-explorer-ui';

export { routes };

export const NAV_LINKS = [
  { to: routes.transactions, label: 'Transactions' },
  { to: routes.accounts, label: 'Accounts' },
  { to: routes.ledgers, label: 'Ledgers' },
  { to: routes.assets, label: 'Assets' },
  { to: routes.contracts, label: 'Contracts' },
  { to: routes.nfts, label: 'NFTs' },
  { to: routes.pools, label: 'Liquidity Pools' },
] as const;
