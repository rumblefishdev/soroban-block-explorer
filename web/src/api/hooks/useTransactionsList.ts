import {
  listTransactionsOptions,
  type ListTransactionsData,
} from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListTransactionsData['query']>;

/**
 * `GET /transactions` — cursor-paginated transaction list with optional
 * filters (`q`, `op`, etc.). Cursor + filters together form the queryKey,
 * so each (filter, cursor) combination is a separate cache entry.
 */
export const useTransactionsList = (
  cursor: string | null = null,
  filters?: Filters
) =>
  useQuery({
    ...listTransactionsOptions({
      query: { ...(filters ?? {}), ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
  });
