import type { EntityType } from './types.js';

const routes: Record<EntityType, (id: string) => string> = {
  transaction: (id) => `/transactions/${encodeURIComponent(id)}`,
  account: (id) => `/accounts/${encodeURIComponent(id)}`,
  contract: (id) => `/contracts/${encodeURIComponent(id)}`,
  asset: (id) => `/assets/${encodeURIComponent(id)}`,
  pool: (id) => `/liquidity-pools/${encodeURIComponent(id)}`,
  ledger: (id) => `/ledgers/${encodeURIComponent(id)}`,
  nft: (id) => `/nfts/${encodeURIComponent(id)}`,
};

export function getIdentifierHref(type: EntityType, id: string): string {
  return routes[type](id);
}
