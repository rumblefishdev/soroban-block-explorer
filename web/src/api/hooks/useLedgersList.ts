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
  // Explicit page size — matches the 20/page used by every other list view
  // (the other pages set it via `PAGE_SIZE`; ledgers has no filters object so
  // it goes here). Without it the endpoint silently falls back to the backend
  // default.
  const query: Record<string, string | number> = { order, limit: 20 };
  if (cursor) query.cursor = cursor;
  return useQuery({
    ...listLedgersOptions({
      query: query as { cursor?: string; limit?: number },
    }),
    ...listPolicy,
  });
};
