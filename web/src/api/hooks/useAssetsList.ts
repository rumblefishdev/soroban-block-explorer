import { listAssetsOptions, type ListAssetsData } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListAssetsData['query']>;

/**
 * `GET /assets` — cursor-paginated asset list with optional type / code
 * filters. URL-as-state pagination via `useCursorPagination`. The backend
 * orders by `id` only (no client-selectable sort), so no `order` param is
 * sent.
 */
export const useAssetsList = (
  cursor: string | null = null,
  filters?: Filters
) => {
  const query: Filters = {
    ...(filters ?? {}),
    ...(cursor ? { cursor } : {}),
  };
  return useQuery({
    ...listAssetsOptions({ query }),
    ...listPolicy,
  });
};
