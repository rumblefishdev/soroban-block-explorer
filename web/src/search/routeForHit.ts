import { getIdentifierHref } from '@rumblefish/soroban-block-explorer-ui';
import type { EntityType } from '@rumblefish/api-types';

import { routes } from '../router/routes.js';

interface HitLike {
  entity_type: EntityType;
  identifier: string;
  surrogate_id?: number | null;
}

/**
 * Resolve a search-result hit to the URL the frontend should navigate
 * to when the user clicks the row.
 *
 * NFT entity_type is special-cased: NFT detail routes are composite
 * (`/nfts/:contract_id/:token_id`) and the backend `SearchHit` does
 * not yet carry the composite payload — that wire change is deferred
 * to the search follow-up task. Until then, an NFT search hit falls
 * back to the NFT list page rather than producing a single-segment
 * `/nfts/<surrogate>` URL that React Router can no longer match
 * (would render as a 404). The list page is at worst a small extra
 * click for the user; producing a broken link would be silently bad.
 */
export function routeForHit(hit: HitLike): string {
  if (hit.entity_type === 'nft') {
    return routes.nfts;
  }
  const idForUrl =
    hit.surrogate_id != null ? String(hit.surrogate_id) : hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
