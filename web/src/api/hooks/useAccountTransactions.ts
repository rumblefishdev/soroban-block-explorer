import { listAccountTransactionsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches the paginated transactions involving an account
 * (`GET /accounts/:account_id/transactions`). Cursor pagination; disabled
 * until an id is present.
 */
export const useAccountTransactions = (accountId: string) =>
  useInfiniteQuery({
    ...listAccountTransactionsInfiniteOptions({
      path: { account_id: accountId },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: accountId.length > 0,
    initialPageParam: { path: { account_id: accountId } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
