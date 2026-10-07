import type { SearchHit } from '@rumblefish/api-types';

import type { SoranMemo } from './soran.js';

/** Explorer-only enrichment; the indexer's search response is unchanged. */
export type ExplorerSearchHit = SearchHit & {
  soran?: { name: string; memo: SoranMemo };
};

/** An M destination must never navigate to its underlying G account. */
export function isMuxedSoranHit(hit: ExplorerSearchHit): boolean {
  return (
    hit.soran != null &&
    hit.entity_type === 'account' &&
    hit.identifier.startsWith('M')
  );
}
