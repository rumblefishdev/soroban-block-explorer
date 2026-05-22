import { listAssetsOptions, type ListAssetsData } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListAssetsData['query']>;

/**
 * `GET /assets` — cursor-paginated asset list with optional type / code
 * filters. URL-as-state pagination via `useCursorPagination`.
 */
export const useAssetsList = (
  cursor: string | null = null,
  filters?: Filters
) =>
  useQuery({
    ...listAssetsOptions({
      query: { ...(filters ?? {}), ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
  });
