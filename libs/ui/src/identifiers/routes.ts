import { routes } from '../routes.js';
import type { EntityType } from './types.js';

// Single-arg href builders per entity, derived from the canonical `routes`
// table (`../routes.js`) — there is no second route definition here (task
// 0299). NFT identity is composite `(contract_id, token_id)` per ADR 0030 /
// task 0264 Phase 8a, so a single-arg build is impossible: `nft` throws loud
// rather than emit a broken `/nfts/:c/:t`. Kept for `Record<EntityType, …>`
// exhaustiveness. `IdentifierDisplay` callers rendering an NFT identifier must
// pass `href` explicitly (no production callsite does today); NFT search hits
// route through `routeForHit`, which builds the composite URL from `routes.nft`.
const hrefBuilders: Record<EntityType, (id: string) => string> = {
  transaction: routes.transaction,
  account: routes.account,
  contract: routes.contract,
  asset: routes.asset,
  pool: routes.pool,
  ledger: routes.ledger,
  nft: () => {
    throw new Error(
      'getIdentifierHref("nft", id) is not supported — NFT routing is ' +
        'composite `(contract_id, token_id)`. Use `routeForHit(hit)` for ' +
        'search hits, or pass `href` explicitly to `IdentifierDisplay`.'
    );
  },
};

export function getIdentifierHref(type: EntityType, id: string): string {
  return hrefBuilders[type](id);
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
 * because the single-arg `hrefBuilders.nft` throws — NFT identity is
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
      return routes.nft(hit.contract_id, hit.token_id);
    }
    // Backend `nft_hits` CTE always projects both. Missing payload
    // would be a contract bug — fall back to the NFT index.
    return routes.nfts;
  }
  const idForUrl = hit.route_token ?? hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
