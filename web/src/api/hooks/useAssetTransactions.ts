import { listAssetTransactionsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches the paginated transactions involving an asset
 * (`GET /assets/:id/transactions`). Cursor pagination; disabled until an id
 * is present.
 */
export const useAssetTransactions = (id: string) =>
  useInfiniteQuery({
    ...listAssetTransactionsInfiniteOptions({
      path: { id },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: id.length > 0,
    initialPageParam: { path: { id } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
