import { listAssetsOptions, type ListAssetsData } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListAssetsData['query']>;
type Order = 'asc' | 'desc';

/**
 * `GET /assets` — cursor-paginated asset list with optional type / code
 * filters. URL-as-state pagination via `useCursorPagination`. `order`
 * flips the `total_supply` sort direction — forwarded via cast since
 * the codegen does not (yet) describe it.
 */
export const useAssetsList = (
  cursor: string | null = null,
  filters?: Filters,
  order: Order = 'desc'
) => {
  const query: Record<string, unknown> = {
    ...(filters ?? {}),
    order,
    ...(cursor ? { cursor } : {}),
  };
  return useQuery({
    ...listAssetsOptions({ query: query as Filters }),
    ...listPolicy,
  });
};
