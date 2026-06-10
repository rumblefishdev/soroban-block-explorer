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
  // The NFT search-hit dispatch lives in `routeForHit` (this module)
  // which short-circuits on `entity_type === 'nft'` and builds the
  // composite URL inline. `IdentifierDisplay` callers that render an
  // NFT identifier must pass `href` explicitly (no production callsite
  // does today).
  nft: () => {
    throw new Error(
      'getIdentifierHref("nft", id) is not supported — NFT routing is ' +
        'composite `(contract_id, token_id)`. Use `routeForHit(hit)` for ' +
        'search hits, or pass `href` explicitly to `IdentifierDisplay`.'
    );
  },
};

export function getIdentifierHref(type: EntityType, id: string): string {
  return routes[type](id);
}

interface HitLike {
  entity_type: EntityType;
  identifier: string;
  route_token?: string | null;
  contract_id?: string | null;
  token_id?: string | null;
}

/**
 * Build a navigation URL from a search hit or redirect payload.
 * Handles NFT composite routing inline (`/nfts/:contract/:token`)
 * because the single-arg `routes.nft` throws — NFT identity is
 * composite per ADR 0030 / task 0264 Phase 8a.
 *
 * For every other type the routing key is `route_token` when the
 * backend supplies one (today only `asset`, whose display `identifier`
 * is the non-routable asset code — `route_token` carries the canonical
 * `/assets/:id` token: contract StrKey | `CODE-ISSUER` | `native`).
 * For transaction / account / contract / pool the display `identifier`
 * IS the routable id, so `route_token` is absent and we route on it.
 */
export function routeForHit(hit: HitLike): string {
  if (hit.entity_type === 'nft') {
    if (hit.contract_id && hit.token_id) {
      return `/nfts/${encodeURIComponent(hit.contract_id)}/${encodeURIComponent(
        hit.token_id
      )}`;
    }
    // Backend `nft_hits` CTE always projects both. Missing payload
    // would be a contract bug — fall back to the NFT index.
    return '/nfts';
  }
  const idForUrl = hit.route_token ?? hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
