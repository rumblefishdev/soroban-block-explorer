import { getIdentifierHref } from '@rumblefish/soroban-block-explorer-ui';
import type { EntityType } from '@rumblefish/api-types';

interface HitLike {
  entity_type: EntityType;
  identifier: string;
  surrogate_id?: number | null;
}

export function routeForHit(hit: HitLike): string {
  const idForUrl =
    hit.surrogate_id != null ? String(hit.surrogate_id) : hit.identifier;
  return getIdentifierHref(hit.entity_type, idForUrl);
}
