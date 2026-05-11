import {
  listTransactionsInfiniteOptions,
  type ListTransactionsData,
  type Options,
} from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListTransactionsData['query']>;

export const useTransactionsList = (filters?: Filters) =>
  useInfiniteQuery({
    ...listTransactionsInfiniteOptions(
      filters
        ? ({ query: filters } as Options<ListTransactionsData>)
        : undefined
    ),
    ...listPolicy,
    initialPageParam: {} as { query?: ListTransactionsData['query'] },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
