import type { PoolEvent } from '@rumblefish/api-types';
import { listPoolActivityOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy, PAGE_SIZE } from '../polling.js';

/**
 * The LP detail page's activity list — one row per OPERATION against the pool
 * (task 0491). `event` maps to `filter[event]`; omitted means everything.
 */
export const usePoolActivity = (
  poolId: string,
  cursor: string | null = null,
  event?: PoolEvent
) =>
  useQuery({
    ...listPoolActivityOptions({
      path: { pool_id: poolId },
      query: {
        limit: PAGE_SIZE,
        ...(cursor ? { cursor } : {}),
        ...(event ? { 'filter[event]': event } : {}),
      },
    }),
    ...listPolicy,
    enabled: poolId.length > 0,
  });
