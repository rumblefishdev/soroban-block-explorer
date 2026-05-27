import type { EntityType } from './types.js';

const routes: Record<EntityType, (id: string) => string> = {
  transaction: (id) => `/transactions/${encodeURIComponent(id)}`,
  account: (id) => `/accounts/${encodeURIComponent(id)}`,
  contract: (id) => `/contracts/${encodeURIComponent(id)}`,
  asset: (id) => `/assets/${encodeURIComponent(id)}`,
  pool: (id) => `/liquidity-pools/${encodeURIComponent(id)}`,
  ledger: (id) => `/ledgers/${encodeURIComponent(id)}`,
  // Defensive: NFT identity is composite `(contract_id, token_id)` per
  // ADR 0030 / task 0264 Phase 8a, so single-arg dispatch cannot build a
  // valid `/nfts/:c/:t` URL. Kept here for `Record<EntityType, …>`
  // exhaustiveness; loud throw beats a silent broken URL if a future
  // regression routes an `'nft'` through this builder.
  //
  // The only NFT search call site (`web/src/search/routeForHit.ts`)
  // short-circuits on `entity_type === 'nft'` and uses the composite
  // `routes.nft(c, t)` builder from `web/src/router/routes.ts`. No
  // `IdentifierDisplay` callers pass `type="nft"` today.
  nft: () => {
    throw new Error(
      'getIdentifierHref("nft", id) is not supported — NFT routing is composite ' +
        '`(contract_id, token_id)`. Use routes.nft(contractId, tokenId) from ' +
        '`web/src/router/routes.ts` instead.'
    );
  },
};

export function getIdentifierHref(type: EntityType, id: string): string {
  return routes[type](id);
}
