import { listParticipantsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches paginated liquidity providers for a single pool
 * (`GET /liquidity-pools/:id/participants`). Cursor pagination ordered
 * by shares DESC.
 */
export const usePoolParticipants = (poolId: string) =>
  useInfiniteQuery({
    ...listParticipantsInfiniteOptions({
      path: { pool_id: poolId },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: poolId.length > 0,
    initialPageParam: { path: { pool_id: poolId } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
