import { getIdentifierHref } from '@rumblefish/soroban-block-explorer-ui';
import type { EntityType } from '@rumblefish/api-types';

import { routes } from '../router/routes.js';

interface HitLike {
  entity_type: EntityType;
  identifier: string;
  surrogate_id?: number | null;
  contract_id?: string | null;
  token_id?: string | null;
}

export function routeForHit(hit: HitLike): string {
  // NFT identity is composite `(contract_id, token_id)` per ADR 0030 /
  // task 0264 Phase 8a. Route through the dedicated 2-arg builder so
  // React Router matches `/nfts/:contractId/:tokenId` — single-segment
  // surrogate dispatch produced a hard 404 (regression introduced by
  // 0264 Phase 8a, the explicit revert in `4716d5f3` deferred the
  // proper fix to here).
  if (hit.entity_type === 'nft' && hit.contract_id && hit.token_id) {
    return routes.nft(hit.contract_id, hit.token_id);
  }
  const idForUrl =
    hit.surrogate_id != null ? String(hit.surrogate_id) : hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
