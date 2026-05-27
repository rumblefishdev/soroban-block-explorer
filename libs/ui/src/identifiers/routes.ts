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

/**
 * Canonical entity URL builders re-exported as `entityRoutes` for
 * callers that need direct access to the raw builders (e.g. composing
 * a higher-level `routes` table at the app level). NFT entry throws
 * on call — composite identity requires a separate 2-arg path
 * builder; see `web/src/router/routes.ts::routes.nft`.
 */
export const entityRoutes = routes;

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
