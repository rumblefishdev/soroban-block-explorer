import { listLedgersOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Order = 'asc' | 'desc';

/**
 * `GET /ledgers` — cursor-paginated ledger list. Each `cursor` value
 * is a distinct React Query cache entry (queryKey carries the cursor),
 * so revisiting an already-loaded cursor is a cache hit. URL-as-state
 * pagination — caller passes the current cursor from `useCursorPagination`.
 *

 */
export const useLedgersList = (
  cursor: string | null = null,
  order: Order = 'desc'
) => {
  const query: Record<string, string | number> = { order };
  if (cursor) query.cursor = cursor;
  return useQuery({
    ...listLedgersOptions({
      query: query as { cursor?: string; limit?: number },
    }),
    ...listPolicy,
  });
};
