import { listAccountTransactionsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

type Order = 'asc' | 'desc';

/**
 * `GET /accounts/:account_id/transactions` — cursor-paginated transactions
 * involving an account. URL-as-state pagination; disabled until id present.
 */
export const useAccountTransactions = (
  accountId: string,
  cursor: string | null = null,
  order: Order = 'desc'
) => {
  const query: Record<string, string | number> = { limit: PAGE_SIZE, order };
  if (cursor) query.cursor = cursor;
  return useQuery({
    ...listAccountTransactionsOptions({
      path: { account_id: accountId },
      query: query as { cursor?: string; limit?: number },
    }),
    ...listPolicy,
    enabled: accountId.length > 0,
  });
};
