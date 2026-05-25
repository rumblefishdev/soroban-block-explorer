import { listAccountTransactionsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * `GET /accounts/:account_id/transactions` — cursor-paginated transactions
 * involving an account. URL-as-state pagination; disabled until id present.
 */
export const useAccountTransactions = (
  accountId: string,
  cursor: string | null = null
) =>
  useQuery({
    ...listAccountTransactionsOptions({
      path: { account_id: accountId },
      query: { limit: PAGE_SIZE, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: accountId.length > 0,
  });
