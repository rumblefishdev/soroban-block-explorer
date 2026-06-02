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
  surrogate_id?: number | null;
  contract_id?: string | null;
  token_id?: string | null;
}

/**
 * Build a navigation URL from a search hit or redirect payload.
 * Handles NFT composite routing inline (`/nfts/:contract/:token`)
 * because the single-arg `routes.nft` throws — NFT identity is
 * composite per ADR 0030 / task 0264 Phase 8a.
 *
 * For non-NFT hits prefers `surrogate_id` (when present) over the
 * human-shown `identifier` because the surrogate is the form the
 * polymorphic `/v1/assets/:id` validator accepts.
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
  const idForUrl =
    hit.surrogate_id != null ? String(hit.surrogate_id) : hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
