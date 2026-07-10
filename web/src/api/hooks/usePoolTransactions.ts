import { listPoolTransactionsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy, PAGE_SIZE } from '../polling.js';

/**
 * `GET /liquidity-pools/:id/transactions` — cursor-paginated transactions
 * for a single liquidity pool. Each cursor is a distinct queryKey, so
 * revisiting a cursor is a cache hit. URL-as-state pagination — caller
 * passes the current cursor from `useCursorPagination`.
 *
 * Per-transaction LP amounts (deposit / withdraw / trade amount_a, amount_b)
 * are NOT included — that surface lives behind `?expand=lp_op_details`,
 * implementation pending 0247 RESEARCH conclusion + 0249 follow-up.
 */
export const usePoolTransactions = (
  poolId: string,
  cursor: string | null = null
) =>
  useQuery({
    ...listPoolTransactionsOptions({
      path: { pool_id: poolId },
      query: { limit: PAGE_SIZE, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: poolId.length > 0,
  });
