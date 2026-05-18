import {
  listAssetsInfiniteOptions,
  type ListAssetsData,
} from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListAssetsData['query']>;

/**
 * Fetches the paginated asset list (`GET /assets`) with optional type and
 * code filters. Cursor pagination.
 */
export const useAssetsList = (filters?: Filters) =>
  useInfiniteQuery({
    ...listAssetsInfiniteOptions(filters ? { query: filters } : undefined),
    ...listPolicy,
    initialPageParam: {},
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
