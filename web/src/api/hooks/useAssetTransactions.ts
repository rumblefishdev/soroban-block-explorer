import { listAssetTransactionsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * `GET /assets/:id/transactions` — cursor-paginated transactions involving
 * an asset. URL-as-state pagination; disabled until id present.
 */
export const useAssetTransactions = (
  id: string,
  cursor: string | null = null
) =>
  useQuery({
    ...listAssetTransactionsOptions({
      path: { id },
      query: { limit: PAGE_SIZE, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: id.length > 0,
  });
