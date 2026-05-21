import { listPoolTransactionsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches paginated transactions for a single liquidity pool
 * (`GET /liquidity-pools/:id/transactions`). Cursor pagination.
 *
 * Per-transaction LP amounts (deposit / withdraw / trade amount_a, amount_b)
 * are NOT included — that surface lives behind `?expand=lp_op_details`,
 * implementation pending 0247 RESEARCH conclusion + 0249 follow-up.
 */
export const usePoolTransactions = (poolId: string) =>
  useInfiniteQuery({
    ...listPoolTransactionsInfiniteOptions({
      path: { pool_id: poolId },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: poolId.length > 0,
    initialPageParam: { path: { pool_id: poolId } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
