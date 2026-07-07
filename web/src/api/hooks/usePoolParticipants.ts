import { listParticipantsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy, PAGE_SIZE } from '../polling.js';

/**
 * `GET /liquidity-pools/:id/participants` — cursor-paginated liquidity
 * providers for a single pool, ordered by shares DESC. Each cursor is a
 * distinct queryKey, so revisiting a cursor is a cache hit. URL-as-state
 * pagination — caller passes the current cursor from `useCursorPagination`.
 */
export const usePoolParticipants = (
  poolId: string,
  cursor: string | null = null
) =>
  useQuery({
    ...listParticipantsOptions({
      path: { pool_id: poolId },
      query: { limit: PAGE_SIZE, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: poolId.length > 0,
  });
