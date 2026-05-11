import {
  listTransactionsInfiniteOptions,
  type ListTransactionsData,
} from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListTransactionsData['query']>;

export const useTransactionsList = (filters?: Filters) =>
  useInfiniteQuery({
    ...listTransactionsInfiniteOptions(
      filters ? { query: filters } : undefined
    ),
    ...listPolicy,
    initialPageParam: {},
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
