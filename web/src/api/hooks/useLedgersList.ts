import { listLedgersInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

export const useLedgersList = () =>
  useInfiniteQuery({
    ...listLedgersInfiniteOptions(),
    ...listPolicy,
    initialPageParam: {},
    getNextPageParam: (lastPage) =>
      lastPage.page.has_more ? lastPage.page.cursor ?? undefined : undefined,
  });
