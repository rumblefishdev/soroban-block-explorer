import type { SearchHit } from '@rumblefish/api-types';
import {
  getIdentifierHref,
  routeSegments,
} from '@rumblefish/soroban-block-explorer-ui';

/**
 * The fields of a `SearchHit` that routing actually reads. Derived from the
 * generated `SearchHit` (codegen from the OpenAPI spec) rather than
 * hand-declared, so a backend rename of any routing field breaks this at
 * compile time. Callers pass a full `SearchHit`; it satisfies this subset.
 */
type RoutableHit = Pick<
  SearchHit,
  'entity_type' | 'identifier' | 'route_token' | 'contract_id' | 'token_id'
>;

/**
 * Build a navigation URL from a search hit or redirect payload.
 *
 * Lives in `web` (not `libs/ui`) because it is search-domain logic whose only
 * consumers are here, and because it depends on the generated `SearchHit`
 * type — a coupling the presentational `libs/ui` deliberately avoids.
 *
 * Handles NFT composite routing inline (`/nfts/:contract/:token`) because NFT
 * identity is composite `(contract_id, token_id)` per ADR 0030 / task 0264
 * Phase 8a and has no single-id build via `getIdentifierHref`.
 *
 * For every other type the routing key is `route_token` when the backend
 * supplies one (today only `asset`, whose display `identifier` is the
 * non-routable asset code — `route_token` carries the canonical `/assets/:id`
 * token: contract StrKey | `CODE-ISSUER` | `native`). For
 * transaction / account / contract / pool the display `identifier` IS the
 * routable id, so `route_token` is absent and we route on it.
 */
export function routeForHit(hit: RoutableHit): string {
  if (hit.entity_type === 'nft') {
    if (hit.contract_id && hit.token_id) {
      return `/${routeSegments.nft}/${encodeURIComponent(
        hit.contract_id
      )}/${encodeURIComponent(hit.token_id)}`;
    }
    // Backend `nft_hits` CTE always projects both. Missing payload
    // would be a contract bug — fall back to the NFT index.
    return `/${routeSegments.nft}`;
  }
  const idForUrl = hit.route_token ?? hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
