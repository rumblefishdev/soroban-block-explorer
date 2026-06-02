import {
  listContractsOptions,
  type ListContractsData,
} from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListContractsData['query']>;

/**
 * `GET /contracts` — cursor-paginated contract list with optional
 * `filter[type]` (class) and `filter[q]` (id/name search). No user sort:
 * the backend orders newest-ingested first (`id DESC`). URL-as-state
 * pagination via `useCursorPagination`.
 */
export const useContractsList = (
  cursor: string | null = null,
  filters?: Filters
) => {
  const query: Filters = {
    ...(filters ?? {}),
    ...(cursor ? { cursor } : {}),
  };
  return useQuery({
    ...listContractsOptions({ query }),
    ...listPolicy,
  });
};
