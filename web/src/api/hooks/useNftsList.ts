import {
  listNftsInfiniteOptions,
  type ListNftsData,
} from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListNftsData['query']>;

/**
 * `GET /nfts` — cursor-paginated NFT list, optionally filtered by collection,
 * contract id or name. Each page carries `data` plus a `page` cursor.
 */
export const useNftsList = (filters?: Filters) =>
  useInfiniteQuery({
    ...listNftsInfiniteOptions(filters ? { query: filters } : undefined),
    ...listPolicy,
    initialPageParam: {},
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
