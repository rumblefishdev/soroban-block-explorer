import { listEventsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches the paginated event history for a contract
 * (`GET /contracts/:contract_id/events`). Cursor pagination; disabled until
 * an id is present. Note: a single appearance can expand to multiple rows,
 * so a page's `data.length` may exceed `limit` — never derive counts from it.
 */
export const useContractEvents = (contractId: string) =>
  useInfiniteQuery({
    ...listEventsInfiniteOptions({
      path: { contract_id: contractId },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: contractId.length > 0,
    initialPageParam: { path: { contract_id: contractId } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
